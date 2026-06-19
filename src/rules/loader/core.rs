use std::{cmp::Ordering, collections::BTreeMap, path::Path};

use anyhow::Result;

use crate::model::{
    DxDiagnostic, DxMeasurementKind, DxRuleDefinition, DxRulePackStatus, DxRulePackSummary,
    DxSeverity,
};
use crate::rule_pack::{
    has_rule_pack_marker, pack_id_from_document, pack_version_from_document,
    parse_rules_from_document,
};

use super::super::validation::validate_rule_pack_rules;
use super::artifacts::{
    builtin_rule_pack_set, discover_sources, generated_or_source_status,
    machine_document_matches_source, non_rule_source_skipped, prepare_default_source,
    process_check_artifact, process_project_dx_config, read_source_document, rule_pack_document,
    rule_pack_load_failed, rule_pack_status, serializer_for,
};
use super::registry_sources::{
    is_registry_lock_failure, local_sources_for_lock_scope, locked_sources,
};
use super::summaries::{pack_provenance, summary_for_source};
use super::types::{
    LoadedRulePackSet, RulePackLoadOptions, RulePackSource, RulePackSourceTrust,
    RulePackSummaryInput,
};

pub fn load_rule_packs(root: &Path, allow_writes: bool) -> Result<Vec<DxRulePackSummary>> {
    Ok(load_rule_pack_set(root, allow_writes)?.summaries)
}

pub fn load_rule_pack_set(root: &Path, allow_writes: bool) -> Result<LoadedRulePackSet> {
    load_rule_pack_set_with_options(
        root,
        RulePackLoadOptions {
            allow_writes,
            strict_rule_packs: false,
        },
    )
}

pub fn load_rule_pack_set_with_options(
    root: &Path,
    options: RulePackLoadOptions,
) -> Result<LoadedRulePackSet> {
    let check_dir = root.join(".dx").join("check");
    let serializer_dir = root.join(".dx").join("serializer");

    prepare_default_source(&check_dir, options.allow_writes)?;
    let serializer = serializer_for(&serializer_dir);

    let mut packs = Vec::new();
    let mut categories = Vec::new();
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();
    let mut loaded_rule_ids = LoadedRuleIdRegistry::default();
    process_project_dx_config(root, &serializer, options.allow_writes, &mut diagnostics);

    let lock_scope = locked_sources(
        root,
        &check_dir,
        &serializer,
        options,
        &mut packs,
        &mut diagnostics,
    )?;
    let locked_paths = lock_scope
        .sources
        .iter()
        .map(|source| source.path.clone())
        .collect::<Vec<_>>();
    let mut sources = lock_scope.sources;
    sources.extend(local_sources_for_lock_scope(
        discover_sources(&check_dir)?,
        lock_scope.strict_local_sources,
        lock_scope.lock_present,
        &locked_paths,
        &mut packs,
        &mut diagnostics,
    )?);
    let registry_blocked_fallback =
        !packs.is_empty() || diagnostics.iter().any(is_registry_lock_failure);

    if sources.is_empty() {
        if !packs.is_empty() || !diagnostics.is_empty() {
            return Ok(LoadedRulePackSet {
                summaries: packs,
                categories,
                rules,
                diagnostics,
            });
        }
        return builtin_rule_pack_set();
    }

    let mut saw_rule_pack_source = false;
    sources.sort_by(source_processing_order);
    for source in sources {
        let mut accumulator = RulePackAccumulator {
            packs: &mut packs,
            categories: &mut categories,
            rules: &mut rules,
            diagnostics: &mut diagnostics,
            loaded_rule_ids: &mut loaded_rule_ids,
        };
        if process_source(&source, &serializer, options, &mut accumulator)? {
            saw_rule_pack_source = true;
        }
    }

    if rules.is_empty() && !saw_rule_pack_source && !registry_blocked_fallback {
        let builtin = builtin_rule_pack_set()?;
        packs.extend(builtin.summaries);
        categories.extend(builtin.categories);
        rules.extend(builtin.rules);
        diagnostics.extend(builtin.diagnostics);
    }

    Ok(LoadedRulePackSet {
        summaries: packs,
        categories,
        rules,
        diagnostics,
    })
}

struct RulePackAccumulator<'a> {
    packs: &'a mut Vec<DxRulePackSummary>,
    categories: &'a mut Vec<crate::model::DxRuleCategoryDefinition>,
    rules: &'a mut Vec<crate::model::DxRuleDefinition>,
    diagnostics: &'a mut Vec<crate::model::DxDiagnostic>,
    loaded_rule_ids: &'a mut LoadedRuleIdRegistry,
}

#[derive(Debug, Default)]
struct LoadedRuleIdRegistry {
    rules: BTreeMap<String, LoadedRuleOrigin>,
}

#[derive(Debug)]
struct LoadedRuleOrigin {
    pack_id: String,
    source_path: String,
}

struct UniqueRuleSelection {
    rules: Vec<DxRuleDefinition>,
    duplicate_found: bool,
}

impl LoadedRuleIdRegistry {
    fn accept_unique_rules(
        &mut self,
        source: &RulePackSource,
        pack_id: &str,
        rules: Vec<DxRuleDefinition>,
        diagnostics: &mut Vec<DxDiagnostic>,
    ) -> UniqueRuleSelection {
        let mut accepted_rules = Vec::with_capacity(rules.len());
        let mut duplicate_found = false;

        for rule in rules {
            if let Some(first) = self.rules.get(&rule.id) {
                duplicate_found = true;
                diagnostics.push(duplicate_loaded_rule_id(source, pack_id, &rule.id, first));
                continue;
            }

            self.rules.insert(
                rule.id.clone(),
                LoadedRuleOrigin {
                    pack_id: pack_id.to_string(),
                    source_path: source.path.display().to_string(),
                },
            );
            accepted_rules.push(rule);
        }

        UniqueRuleSelection {
            rules: accepted_rules,
            duplicate_found,
        }
    }
}

fn process_source(
    source: &RulePackSource,
    serializer: &serializer::SerializerOutput,
    options: RulePackLoadOptions,
    accumulator: &mut RulePackAccumulator<'_>,
) -> Result<bool> {
    let source_document = match read_source_document(&source.path) {
        Ok(document) => document,
        Err(error) => {
            accumulator
                .diagnostics
                .push(rule_pack_load_failed(&source.path, error));
            return Ok(false);
        }
    };
    if !has_rule_pack_marker(&source_document) {
        process_check_artifact(
            &source.path,
            serializer,
            options.allow_writes,
            accumulator.diagnostics,
        );
        accumulator
            .diagnostics
            .push(non_rule_source_skipped(&source.path));
        return Ok(false);
    }

    let mut source_parse = parse_rules_from_document(&source_document, &source.path);
    let source_has_rule_section = source_document.section_by_name("rules").is_some();
    let source_has_rule_diagnostics = !source_parse.diagnostics.is_empty();
    accumulator
        .diagnostics
        .append(&mut source_parse.diagnostics);
    if source_parse.rules.is_empty() && !source_has_rule_section {
        accumulator
            .diagnostics
            .push(non_rule_source_skipped(&source.path));
        return Ok(true);
    }
    if source_parse.rules.is_empty() {
        push_invalid_empty_source_summary(source, &source_document, accumulator.packs);
        return Ok(true);
    }
    if source_has_rule_diagnostics {
        push_invalid_scoring_source(source, &source_document, source_parse, accumulator);
        return Ok(true);
    }

    process_valid_scoring_source(source, &source_document, serializer, options, accumulator);
    Ok(true)
}

fn push_invalid_empty_source_summary(
    source: &RulePackSource,
    document: &serializer::DxDocument,
    packs: &mut Vec<DxRulePackSummary>,
) {
    packs.push(summary_for_source(
        source,
        RulePackSummaryInput {
            id: pack_id_from_document(document, &source.path),
            version: pack_version_from_document(document),
            status: DxRulePackStatus::Invalid,
            machine_path: None,
            provenance: source.trust.default_provenance(),
            lock_status: source.trust.invalid_lock_status(),
            rule_count: 0,
        },
    ));
}

fn push_invalid_scoring_source(
    source: &RulePackSource,
    document: &serializer::DxDocument,
    source_parse: crate::rule_pack::ParsedRulePackRules,
    accumulator: &mut RulePackAccumulator<'_>,
) {
    let pack_categories = source_parse.categories;
    let pack_id = pack_id_from_document(document, &source.path);
    let selection = accumulator.loaded_rule_ids.accept_unique_rules(
        source,
        &pack_id,
        source_parse.rules,
        accumulator.diagnostics,
    );
    let duplicate_found = selection.duplicate_found;
    let pack_rules = selection.rules;
    accumulator.packs.push(summary_for_source(
        source,
        RulePackSummaryInput {
            id: pack_id,
            version: pack_version_from_document(document),
            status: DxRulePackStatus::Invalid,
            machine_path: None,
            provenance: pack_provenance(&pack_rules),
            lock_status: source.trust.invalid_lock_status(),
            rule_count: pack_rules.len(),
        },
    ));
    accumulator.diagnostics.extend(validate_rule_pack_rules(
        &source.path,
        &pack_categories,
        &pack_rules,
    ));
    if !duplicate_found {
        accumulator.categories.extend(pack_categories);
    }
    accumulator.rules.extend(pack_rules);
}

fn process_valid_scoring_source(
    source: &RulePackSource,
    source_document: &serializer::DxDocument,
    serializer: &serializer::SerializerOutput,
    options: RulePackLoadOptions,
    accumulator: &mut RulePackAccumulator<'_>,
) {
    let paths = serializer.get_paths(&source.path);
    let mut status = rule_pack_status(
        &source.path,
        &paths.machine,
        options.allow_writes,
        serializer,
    );
    let mut document = match rule_pack_document(&source.path, &paths.machine, status.clone()) {
        Ok(document) => document,
        Err(error) => {
            accumulator
                .diagnostics
                .push(rule_pack_load_failed(&source.path, error));
            return;
        }
    };

    if matches!(status, DxRulePackStatus::MachineFresh)
        && !machine_document_matches_source(source_document, &document, &source.path)
    {
        status = generated_or_source_status(&source.path, options.allow_writes, serializer);
        document = match rule_pack_document(&source.path, &paths.machine, status.clone()) {
            Ok(document) => document,
            Err(error) => {
                accumulator
                    .diagnostics
                    .push(rule_pack_load_failed(&source.path, error));
                return;
            }
        };
    }

    let pack_parse = parse_rules_from_document(&document, &source.path);
    let pack_categories = pack_parse.categories;
    let parsed_pack_rules = pack_parse.rules;
    if !has_rule_pack_marker(&document) || parsed_pack_rules.is_empty() {
        accumulator
            .diagnostics
            .push(non_rule_source_skipped(&source.path));
        return;
    }
    let machine_path = match status {
        DxRulePackStatus::MachineFresh | DxRulePackStatus::MachineGenerated => {
            Some(paths.machine.display().to_string())
        }
        _ => None,
    };
    let pack_id = pack_id_from_document(&document, &source.path);
    let selection = accumulator.loaded_rule_ids.accept_unique_rules(
        source,
        &pack_id,
        parsed_pack_rules,
        accumulator.diagnostics,
    );
    let duplicate_found = selection.duplicate_found;
    let pack_rules = selection.rules;
    let status = if duplicate_found {
        DxRulePackStatus::Invalid
    } else {
        status
    };
    let machine_path = if status == DxRulePackStatus::Invalid {
        None
    } else {
        machine_path
    };
    let lock_status = if status == DxRulePackStatus::Invalid {
        source.trust.invalid_lock_status()
    } else {
        source.trust.lock_status(&status).to_string()
    };

    accumulator.packs.push(summary_for_source(
        source,
        RulePackSummaryInput {
            id: pack_id,
            version: pack_version_from_document(&document),
            status,
            machine_path,
            provenance: pack_provenance(&pack_rules),
            lock_status,
            rule_count: pack_rules.len(),
        },
    ));
    accumulator.diagnostics.extend(validate_rule_pack_rules(
        &source.path,
        &pack_categories,
        &pack_rules,
    ));
    if !duplicate_found {
        accumulator.categories.extend(pack_categories);
    }
    accumulator.rules.extend(pack_rules);
}

fn source_processing_order(left: &RulePackSource, right: &RulePackSource) -> Ordering {
    source_trust_rank(&left.trust)
        .cmp(&source_trust_rank(&right.trust))
        .then_with(|| source_default_rank(left).cmp(&source_default_rank(right)))
        .then_with(|| left.path.cmp(&right.path))
}

fn source_default_rank(source: &RulePackSource) -> u8 {
    if source
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("dx-default.sr"))
    {
        0
    } else {
        1
    }
}

fn source_trust_rank(trust: &RulePackSourceTrust) -> u8 {
    match trust {
        RulePackSourceTrust::Locked { .. } => 0,
        RulePackSourceTrust::Local => 1,
    }
}

fn duplicate_loaded_rule_id(
    source: &RulePackSource,
    pack_id: &str,
    rule_id: &str,
    first: &LoadedRuleOrigin,
) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-duplicate-rule-id".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.path.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule id `{rule_id}` in rule pack `{pack_id}` duplicates a rule already loaded from rule pack `{}` at {}; only the first loaded rule id is scored",
            first.pack_id, first.source_path
        ),
        next_action:
            "Rename one rule id so every loaded locked, local, and default rule pack has globally unique rule ids."
                .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}
