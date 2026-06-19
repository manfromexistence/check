use std::fs;

use tempfile::tempdir;

use crate::{
    analyze_project,
    model::{DxCheckEngineOptions, DxRulePackStatus},
};

#[test]
fn local_sr_rule_pack_drives_line_count_findings() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("local.sr"),
        r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
tiny-line-budget structure warning 8 line_count max 20 docs/check/tiny.md local
)
"#,
    )
    .unwrap();
    fs::write(
        temp.path().join("src").join("small.rs"),
        "fn example() {}\n".repeat(21),
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-local.machine")
            .is_file()
    );
    assert!(report.findings.iter().any(|finding| {
        finding.id == "tiny-line-budget"
            && finding.file.as_deref() == Some("src/small.rs")
            && finding.message.contains("above the rule threshold 20")
    }));
}

#[test]
fn local_sr_rule_pack_reports_min_line_count_direction() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("local.sr"),
        r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
minimum-line-budget structure warning 8 line_count min 10 docs/check/minimum.md local
)
"#,
    )
    .unwrap();
    fs::write(temp.path().join("src").join("tiny.rs"), "fn main() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.id == "minimum-line-budget"
            && finding.file.as_deref() == Some("src/tiny.rs")
            && finding.message.contains("below the rule threshold 10")
    }));
}

#[test]
fn local_rule_pack_unknown_metric_emits_diagnostic() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("bad-metric.sr"),
        r#"
rule_pack(id=bad-metric version=1 title=BadMetric kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
unknown-metric-rule structure warning 8 made_up_metric max 10 docs/check/bad.md local
)
"#,
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-unknown-metric"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|path| path.ends_with("bad-metric.sr"))
            && diagnostic.message.contains("made_up_metric")
    }));
}

#[test]
fn read_only_non_rule_check_sr_is_skipped_and_builtin_rules_load() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("doctor.sr"),
        receipt(),
    )
    .unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), false).unwrap();

    assert!(loaded.summaries.iter().any(|pack| {
        pack.id == "dx-check-default"
            && pack.status == DxRulePackStatus::BuiltIn
            && pack.rule_count > 0
    }));
    assert!(!loaded.summaries.iter().any(|pack| pack.id == "doctor"));
    assert!(
        loaded
            .rules
            .iter()
            .any(|rule| rule.id == "source-file-line-count")
    );
    assert!(loaded.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-skipped-non-rule-source"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("doctor.sr"))
    }));
}

#[test]
fn non_rule_check_sr_generates_machine_when_writes_are_allowed() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("doctor.sr"),
        receipt(),
    )
    .unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), true).unwrap();

    assert!(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-doctor.machine")
            .is_file(),
        "non-rule check details should generate a serializer machine artifact"
    );
    assert!(!loaded.summaries.iter().any(|pack| pack.id == "doctor"));
    assert!(
        loaded
            .rules
            .iter()
            .any(|rule| rule.id == "source-file-line-count")
    );
    assert!(loaded.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-skipped-non-rule-source"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("doctor.sr"))
    }));
}

#[test]
fn receipt_shaped_rules_table_without_rule_pack_marker_is_skipped() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path()
            .join(".dx")
            .join("check")
            .join("browser-proof.sr"),
        r#"
schema="dx.www.launch.browserRenderProof"
route_count=4
browser_runtime_proof=false

rules[id category severity weight metric op threshold docs provenance](
receipt-owned-rule structure failure 10 line_count max 1 docs/check/receipt.md receipt
)
"#,
    )
    .unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), true).unwrap();

    assert!(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-browser-proof.machine")
            .is_file(),
        "receipt-shaped check details should serialize without becoming rules"
    );
    assert!(
        !loaded
            .rules
            .iter()
            .any(|rule| rule.id == "receipt-owned-rule")
    );
    assert!(
        loaded
            .rules
            .iter()
            .any(|rule| rule.id == "source-file-line-count")
    );
    assert!(loaded.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-skipped-non-rule-source"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("browser-proof.sr"))
    }));
}

#[test]
fn rule_pack_id_without_dx_check_kind_is_skipped() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("generic.sr"),
        r#"
rule_pack(id=generic version=1 title=GenericRules)

rules[id category severity weight metric op threshold docs provenance](
generic-rule structure warning 8 line_count max 1 docs/check/generic.md generic
)
"#,
    )
    .unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), true).unwrap();

    assert!(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-generic.machine")
            .is_file(),
        "generic non-DX rule packs should serialize without being scored"
    );
    assert!(!loaded.rules.iter().any(|rule| rule.id == "generic-rule"));
    assert!(
        loaded
            .rules
            .iter()
            .any(|rule| rule.id == "source-file-line-count")
    );
    assert!(loaded.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-skipped-non-rule-source"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("generic.sr"))
    }));
}

#[test]
fn stale_machine_artifact_does_not_override_changed_sr_rules() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    let source = temp.path().join(".dx").join("check").join("local.sr");
    fs::write(&source, local_rule_pack("old-line-budget", 20, "old")).unwrap();

    let first = super::load_rule_pack_set(temp.path(), true).unwrap();
    assert!(first.rules.iter().any(|rule| rule.id == "old-line-budget"));
    let machine = temp
        .path()
        .join(".dx")
        .join("serializer")
        .join("check-local.machine");
    let stale_machine_bytes = fs::read(&machine).unwrap();

    fs::write(&source, local_rule_pack("new-line-budget", 5, "new")).unwrap();
    fs::write(&machine, stale_machine_bytes).unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), true).unwrap();

    assert!(
        loaded
            .rules
            .iter()
            .any(|rule| rule.id == "new-line-budget" && rule.threshold == Some(5))
    );
    assert!(!loaded.rules.iter().any(|rule| rule.id == "old-line-budget"));
    assert!(
        loaded.summaries.iter().any(|pack| {
            pack.id == "local-check" && pack.status != DxRulePackStatus::MachineFresh
        })
    );
}

#[test]
fn source_only_rule_pack_does_not_advertise_machine_path() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
    fs::write(
        temp.path().join(".dx").join("check").join("local.sr"),
        local_rule_pack("tiny-line-budget", 20, "tiny"),
    )
    .unwrap();

    let loaded = super::load_rule_pack_set(temp.path(), false).unwrap();
    let pack = loaded
        .summaries
        .iter()
        .find(|pack| pack.id == "local-check")
        .expect("local check pack");

    assert_eq!(pack.status, DxRulePackStatus::SourceOnly);
    assert_eq!(pack.machine_path, None);
    assert!(
        !temp
            .path()
            .join(".dx")
            .join("serializer")
            .join("check-local.machine")
            .exists(),
        "read-only source loads must not imply a generated machine artifact"
    );
}

fn receipt() -> &'static str {
    r#"
tool="dx doctor"
command="run"
passed=true
score=500
route_count=4
"#
}

fn local_rule_pack(rule_id: &str, threshold: u32, docs_name: &str) -> String {
    format!(
        r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
{rule_id} structure warning 8 line_count max {threshold} docs/check/{docs_name}.md local
)
"#
    )
}
