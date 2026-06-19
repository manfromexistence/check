use std::fs;
use std::path::Path;

use anyhow::Result;
use serializer::{
    DxDocument, MachineFormat, SerializerOutput, SerializerOutputConfig, llm_to_document,
    machine_to_document,
};

use crate::model::{DxDiagnostic, DxMeasurementKind, DxRulePackStatus, DxSeverity};
use crate::rule_pack::{
    has_rule_pack_marker, pack_id_from_document, pack_version_from_document,
    parse_rules_from_document,
};

use super::super::builtin::{
    DEFAULT_RULE_PACK, default_categories, default_rules, default_summary,
};
use super::super::validation::validate_rule_pack_rules;
use super::types::LoadedRulePackSet;

const LEGACY_AI_STRUCTURE_ROW: &str = "ai-maintainable-project-structure dx-framework-health info 4 project_structure present 0 docs/check/structure.md dx-default";
const AI_STRUCTURE_ROW: &str = "ai-maintainable-project-structure dx-framework-health info 4 project_structure min 1 docs/check/structure.md dx-default";
const GENERATED_MACHINE_ROW: &str = "generated-machine-leak structure warning 8 generated_machine absent 0 docs/check/generated.md dx-default";
const GENERATED_SOURCE_ROW: &str = "generated-source-leak structure warning 8 generated_source absent 0 docs/check/generated.md dx-default";
const TEST_READINESS_ROW: &str = "test-readiness-missing test-readiness warning 6 test_count min 1 docs/check/tests.md dx-default";

pub(super) fn prepare_default_source(check_dir: &Path, allow_writes: bool) -> Result<()> {
    if !allow_writes {
        return Ok(());
    }

    let default_source = check_dir.join("dx-default.sr");
    fs::create_dir_all(check_dir)?;
    if !default_source.exists() {
        fs::write(&default_source, DEFAULT_RULE_PACK)?;
    } else {
        migrate_legacy_default_source(&default_source)?;
    }
    Ok(())
}

pub(super) fn serializer_for(serializer_dir: &Path) -> SerializerOutput {
    SerializerOutput::with_config(
        SerializerOutputConfig::new()
            .with_output_dir(serializer_dir)
            .with_llm(false)
            .with_machine(true),
    )
}

pub(super) fn process_project_dx_config(
    root: &Path,
    serializer: &SerializerOutput,
    allow_writes: bool,
    diagnostics: &mut Vec<DxDiagnostic>,
) {
    if !allow_writes {
        return;
    }

    let source = root.join("dx");
    if source.is_file() {
        process_serializer_artifact(&source, serializer, diagnostics);
    }
}

pub(super) fn process_check_artifact(
    source: &Path,
    serializer: &SerializerOutput,
    allow_writes: bool,
    diagnostics: &mut Vec<DxDiagnostic>,
) {
    if allow_writes {
        process_serializer_artifact(source, serializer, diagnostics);
    }
}

pub(super) fn builtin_rule_pack_set() -> Result<LoadedRulePackSet> {
    let categories = default_categories()?;
    let rules = default_rules()?;
    let diagnostics = validate_rule_pack_rules(Path::new("dx-default.sr"), &categories, &rules);
    Ok(LoadedRulePackSet {
        summaries: vec![default_summary(rules.len())],
        categories,
        rules,
        diagnostics,
    })
}

pub(super) fn discover_sources(check_dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut sources = Vec::new();
    if check_dir.is_dir() {
        for entry in fs::read_dir(check_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("sr")
                && path.file_name().and_then(|name| name.to_str()) != Some("rule-pack-lock.sr")
            {
                sources.push(path);
            }
        }
    }
    Ok(sources)
}

pub(super) fn rule_pack_status(
    source: &Path,
    machine: &Path,
    allow_writes: bool,
    serializer: &SerializerOutput,
) -> DxRulePackStatus {
    if machine.exists()
        && machine_is_fresh(source, machine)
        && machine_document_is_readable(machine)
    {
        DxRulePackStatus::MachineFresh
    } else {
        generated_or_source_status(source, allow_writes, serializer)
    }
}

pub(super) fn generated_or_source_status(
    source: &Path,
    allow_writes: bool,
    serializer: &SerializerOutput,
) -> DxRulePackStatus {
    if allow_writes {
        match serializer.process_file(source) {
            Ok(_) => DxRulePackStatus::MachineGenerated,
            Err(_) => DxRulePackStatus::Invalid,
        }
    } else {
        DxRulePackStatus::SourceOnly
    }
}

pub(super) fn rule_pack_document(
    source: &Path,
    machine: &Path,
    status: DxRulePackStatus,
) -> Result<DxDocument> {
    if matches!(
        status,
        DxRulePackStatus::MachineFresh | DxRulePackStatus::MachineGenerated
    ) {
        return read_machine_document(machine).or_else(|_| read_source_document(source));
    }

    read_source_document(source)
}

pub(super) fn read_source_document(source: &Path) -> Result<DxDocument> {
    Ok(llm_to_document(&fs::read_to_string(source)?)?)
}

pub(super) fn machine_document_matches_source(
    source_document: &DxDocument,
    machine_document: &DxDocument,
    source: &Path,
) -> bool {
    has_rule_pack_marker(machine_document)
        && pack_id_from_document(machine_document, source)
            == pack_id_from_document(source_document, source)
        && pack_version_from_document(machine_document)
            == pack_version_from_document(source_document)
        && parse_rules_from_document(machine_document, source)
            == parse_rules_from_document(source_document, source)
}

pub(super) fn rule_pack_load_failed(source: &Path, error: anyhow::Error) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-load-failed".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!("Rule pack failed to load: {error}"),
        next_action: "Fix the serializer .sr source or regenerate the .machine artifact."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub(super) fn non_rule_source_skipped(source: &Path) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-skipped-non-rule-source".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Info,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "{} is a serializer .sr artifact without DX Check rules",
            source.display()
        ),
        next_action: "Keep non-rule check artifacts in .dx/check when needed; DX Check will skip them for scoring."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn process_serializer_artifact(
    source: &Path,
    serializer: &SerializerOutput,
    diagnostics: &mut Vec<DxDiagnostic>,
) {
    if let Err(error) = serializer.process_file(source) {
        diagnostics.push(serializer_artifact_failed(source, error.into()));
    }
}

fn migrate_legacy_default_source(default_source: &Path) -> Result<()> {
    let mut source = fs::read_to_string(default_source)?;
    let mut changed = false;

    if source.contains(LEGACY_AI_STRUCTURE_ROW) {
        source = source.replace(LEGACY_AI_STRUCTURE_ROW, AI_STRUCTURE_ROW);
        changed = true;
    }

    if !source.contains(GENERATED_SOURCE_ROW) && source.contains(GENERATED_MACHINE_ROW) {
        source = source.replace(
            GENERATED_MACHINE_ROW,
            &format!("{GENERATED_MACHINE_ROW}\n{GENERATED_SOURCE_ROW}"),
        );
        changed = true;
    }

    if !source.contains(TEST_READINESS_ROW) && source.contains(AI_STRUCTURE_ROW) {
        source = source.replace(
            AI_STRUCTURE_ROW,
            &format!("{AI_STRUCTURE_ROW}\n{TEST_READINESS_ROW}"),
        );
        changed = true;
    }

    if changed {
        fs::write(default_source, source)?;
    }
    Ok(())
}

fn read_machine_document(machine: &Path) -> Result<DxDocument> {
    let format = MachineFormat::new(fs::read(machine)?);
    Ok(machine_to_document(&format)?)
}

fn machine_is_fresh(source: &Path, machine: &Path) -> bool {
    let source_modified = fs::metadata(source)
        .and_then(|metadata| metadata.modified())
        .ok();
    let machine_modified = fs::metadata(machine)
        .and_then(|metadata| metadata.modified())
        .ok();
    matches!((source_modified, machine_modified), (Some(source), Some(machine)) if machine >= source)
}

fn machine_document_is_readable(machine: &Path) -> bool {
    read_machine_document(machine).is_ok()
}

fn serializer_artifact_failed(source: &Path, error: anyhow::Error) -> DxDiagnostic {
    DxDiagnostic {
        id: "serializer-artifact-failed".to_string(),
        source: "dx-check-serializer".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!("Serializer artifact failed to generate: {error}"),
        next_action: "Fix the serializer source so DX Check can generate its .machine artifact."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}
