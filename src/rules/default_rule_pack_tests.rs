use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use super::builtin::DEFAULT_RULE_PACK;
use crate::{
    analyze_project,
    model::{DxCheckEngineOptions, DxRulePackStatus},
};

#[test]
fn default_rule_pack_is_valid_serializer_source() {
    serializer::llm_to_document(DEFAULT_RULE_PACK).unwrap();
}

#[test]
fn default_rule_docs_exist_for_every_builtin_rule() {
    let document = serializer::llm_to_document(DEFAULT_RULE_PACK).unwrap();
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing = crate::rule_pack::rules_from_document(&document)
        .into_iter()
        .filter_map(|rule| rule.docs)
        .filter(|docs| !crate_root.join(docs).is_file())
        .collect::<BTreeSet<_>>();

    assert!(
        missing.is_empty(),
        "default rule docs must exist: {}",
        missing.into_iter().collect::<Vec<_>>().join(", ")
    );
}

#[test]
fn generates_default_machine_rule_pack() {
    let temp = tempdir().unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    let machine = temp
        .path()
        .join(".dx")
        .join("serializer")
        .join("check-dx-default.machine");
    assert!(machine.is_file());
    assert!(
        report
            .rule_packs
            .iter()
            .any(|pack| pack.machine_path.as_deref() == Some(machine.to_str().unwrap()))
    );

    let second = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();
    assert!(
        second
            .rule_packs
            .iter()
            .any(|pack| pack.status == DxRulePackStatus::MachineFresh)
    );
}

#[test]
fn generated_default_rule_pack_migrates_legacy_ai_structure_row() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();
    let default_source = temp.path().join(".dx").join("check").join("dx-default.sr");
    fs::write(
        &default_source,
        DEFAULT_RULE_PACK.replace(
            "ai-maintainable-project-structure dx-framework-health info 4 project_structure min 1 docs/check/structure.md dx-default",
            "ai-maintainable-project-structure dx-framework-health info 4 project_structure present 0 docs/check/structure.md dx-default",
        ),
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();
    let source = fs::read_to_string(&default_source).unwrap();

    assert!(source.contains("project_structure min 1"));
    assert!(!source.contains("project_structure present 0"));
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "ai-maintainable-project-structure")
        .expect("missing orientation finding");
    assert_eq!(finding.actual.as_deref(), Some("0"));
    assert_eq!(finding.threshold.as_deref(), Some("1"));
}

#[test]
fn generated_default_rule_pack_migrates_missing_generated_source_row() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    let default_source = temp.path().join(".dx").join("check").join("dx-default.sr");
    let legacy_default = DEFAULT_RULE_PACK.replace(
        "generated-source-leak structure warning 8 generated_source absent 0 docs/check/generated.md dx-default\n",
        "",
    );
    fs::write(&default_source, legacy_default).unwrap();
    fs::write(
        temp.path().join("src").join("schema.generated.ts"),
        "export type GeneratedSchema = {};\n",
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();
    let source = fs::read_to_string(&default_source).unwrap();

    assert!(source.contains("generated_source absent"));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "generated-source-leak"
            && finding.file.as_deref() == Some("src/schema.generated.ts")
    }));
}

#[test]
fn generated_default_rule_pack_migrates_missing_test_readiness_row() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    let default_source = temp.path().join(".dx").join("check").join("dx-default.sr");
    let legacy_default = DEFAULT_RULE_PACK.replace(
        "test-readiness-missing test-readiness warning 6 test_count min 1 docs/check/tests.md dx-default\n",
        "",
    );
    fs::write(&default_source, legacy_default).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();
    let source = fs::read_to_string(&default_source).unwrap();

    assert!(source.contains("test_count min 1"));
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "test-readiness-missing")
    );
}
