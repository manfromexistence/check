use std::fs;

use dx_check_engine::{
    DxCheckEngineOptions, DxRulePackStatus, DxScoreStatus, DxSeverity, analyze_project,
};
use tempfile::tempdir;

mod support;

use support::rule_pack::write_rule_pack;

#[test]
fn invalid_rule_severity_is_diagnostic_not_scored() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "bad-severity.sr",
        "bad-severity-rule structure critical 8 line_count max 1 docs/check/bad.md local",
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("bad-severity-rule")
            && diagnostic.message.contains("severity")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "bad-severity-rule"),
        "malformed severity rows must not become scored findings"
    );
}

#[test]
fn invalid_rule_weight_is_diagnostic_not_scored() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "bad-weight.sr",
        "bad-weight-rule structure warning too-heavy line_count max 1 docs/check/bad.md local",
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("bad-weight-rule")
            && diagnostic.message.contains("weight")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "bad-weight-rule"),
        "malformed weight rows must not become scored findings"
    );
}

#[test]
fn invalid_rule_threshold_is_diagnostic_not_loaded() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "bad-threshold.sr",
        "bad-threshold-rule structure warning 8 line_count max too-many docs/check/bad.md local",
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("bad-threshold-rule")
            && diagnostic.message.contains("threshold")
    }));
    assert!(
        report
            .rule_packs
            .iter()
            .any(|pack| pack.id == "local-check" && pack.rule_count == 0),
        "a rule with an invalid threshold must not count as a loaded rule"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "bad-threshold-rule"),
        "malformed threshold rows must not become scored findings"
    );
}

#[test]
fn missing_required_rule_cell_is_diagnostic_not_non_rule_skip() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "missing-op.sr",
        "missing-op-rule structure warning 8 line_count \"\" 1 docs/check/bad.md local",
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("missing-op-rule")
            && diagnostic.message.contains("op")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "rule-pack-skipped-non-rule-source"),
        "marked DX Check rule packs with malformed rows should not be mislabeled as non-rule"
    );
}

#[test]
fn invalid_only_rule_pack_does_not_fall_back_to_builtin_rules() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "invalid-only.sr",
        "bad-severity-rule structure critical 8 line_count max 1 docs/check/bad.md local",
    );
    std::fs::write(
        root.path().join("src").join("large.rs"),
        "fn example() {}\n".repeat(450),
    )
    .unwrap();

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("bad-severity-rule")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "source-file-line-count"),
        "a marked but invalid local pack should not silently fall back to built-in scoring"
    );
}

#[test]
fn duplicate_rule_ids_emit_diagnostic_and_do_not_double_score() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "duplicate.sr",
        r#"duplicate-line-budget structure warning 8 line_count max 1 docs/check/first.md local
duplicate-line-budget structure failure 20 line_count max 1 docs/check/second.md local"#,
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-duplicate-rule-id"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("duplicate-line-budget")
    }));
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|finding| {
                finding.id == "duplicate-line-budget"
                    && finding.file.as_deref() == Some("src/small.rs")
            })
            .count(),
        1,
        "duplicate rule ids must not score the same source file more than once"
    );
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "local-check" && pack.status == DxRulePackStatus::Invalid && pack.rule_count == 1
    }));
}

#[test]
fn duplicate_rule_ids_across_rule_packs_emit_diagnostic_and_do_not_double_score() {
    let root = tempdir().unwrap();
    write_named_rule_pack(
        root.path(),
        "alpha-check",
        "alpha.sr",
        "shared-line-budget structure warning 8 line_count max 1 docs/check/alpha.md local/alpha",
    );
    write_named_rule_pack(
        root.path(),
        "beta-check",
        "beta.sr",
        "shared-line-budget structure failure 20 line_count max 1 docs/check/beta.md local/beta",
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-duplicate-rule-id"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("shared-line-budget")
            && diagnostic.message.contains("already loaded")
    }));
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|finding| {
                finding.id == "shared-line-budget"
                    && finding.file.as_deref() == Some("src/small.rs")
            })
            .count(),
        1,
        "cross-pack duplicate rule ids must not score the same source file more than once"
    );
    assert_eq!(
        report.score.finding_weight_total, 8,
        "only the first loaded shared rule should contribute finding weight"
    );
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "alpha-check" && pack.status != DxRulePackStatus::Invalid && pack.rule_count == 1
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "beta-check" && pack.status == DxRulePackStatus::Invalid && pack.rule_count == 0
    }));
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn duplicate_rule_ids_across_rule_packs_do_not_override_category_buckets() {
    let root = tempdir().unwrap();
    write_named_rule_pack_with_categories(
        root.path(),
        "alpha-check",
        "alpha.sr",
        "structure TrustedStructure 70",
        "shared-line-budget structure warning 8 line_count max 1 docs/check/alpha.md local/alpha",
    );
    write_named_rule_pack_with_categories(
        root.path(),
        "beta-check",
        "beta.sr",
        r#"structure ShadowStructure 1
shadow ShadowOnly 100"#,
        "shared-line-budget shadow failure 20 line_count max 1 docs/check/beta.md local/beta",
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    let structure = report
        .score
        .buckets
        .iter()
        .find(|bucket| bucket.id == "structure")
        .expect("trusted structure bucket");
    assert_eq!(structure.label, "TrustedStructure");
    assert_eq!(structure.max_score, 70);
    assert_eq!(structure.score, 62);
    assert!(
        !report
            .score
            .buckets
            .iter()
            .any(|bucket| bucket.id == "shadow"),
        "categories from duplicate-invalid rule packs must not leak into scoring buckets"
    );
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "beta-check" && pack.status == DxRulePackStatus::Invalid && pack.rule_count == 0
    }));
    assert_eq!(report.score.finding_weight_total, 8);
}

#[test]
fn local_duplicate_rule_id_cannot_shadow_default_rule_pack_precedence() {
    let root = tempdir().unwrap();
    write_named_rule_pack_with_categories(
        root.path(),
        "shadow-default",
        "aa-shadow-default.sr",
        "structure ShadowStructure 100",
        "source-file-line-count structure failure 90 line_count max 1 docs/check/shadow.md local/shadow",
    );
    write_named_rule_pack_with_categories(
        root.path(),
        "dx-check-default",
        "dx-default.sr",
        "structure DefaultStructure 100",
        "source-file-line-count structure warning 8 line_count max 1 docs/check/default.md dx/default",
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert_eq!(
        report
            .findings
            .iter()
            .filter(|finding| {
                finding.id == "source-file-line-count"
                    && finding.file.as_deref() == Some("src/small.rs")
            })
            .count(),
        1,
        "default and shadow duplicates must not both score"
    );
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "source-file-line-count")
        .expect("default source-file-line-count finding");
    assert_eq!(finding.weight, 8);
    assert_eq!(finding.severity, DxSeverity::Warning);
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "dx-check-default"
            && pack.status != DxRulePackStatus::Invalid
            && pack.rule_count == 1
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "shadow-default"
            && pack.status == DxRulePackStatus::Invalid
            && pack.rule_count == 0
    }));
    assert_eq!(report.score.finding_weight_total, 8);
}

#[test]
fn valid_rule_rows_still_score_beside_invalid_rows() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "mixed.sr",
        r#"tiny-line-budget structure warning 8 line_count max 1 docs/check/tiny.md local
bad-row structure critical 8 line_count max 1 docs/check/bad.md local"#,
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.id == "tiny-line-budget" && finding.file.as_deref() == Some("src/small.rs")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("bad-row")
            && diagnostic.message.contains("severity")
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "local-check" && pack.status == DxRulePackStatus::Invalid && pack.rule_count == 1
    }));
    assert!(
        !root
            .path()
            .join(".dx")
            .join("serializer")
            .join("check-local.machine")
            .exists(),
        "a semantically invalid source pack must not refresh the generated machine cache"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "bad-row"),
        "invalid rows beside valid rows must not become findings"
    );
}

fn write_named_rule_pack(root: &std::path::Path, pack_id: &str, name: &str, rules: &str) {
    write_named_rule_pack_with_categories(root, pack_id, name, "structure Structure 100", rules);
}

fn write_named_rule_pack_with_categories(
    root: &std::path::Path,
    pack_id: &str,
    name: &str,
    categories: &str,
    rules: &str,
) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("small.rs"), "fn one() {}\n".repeat(6)).unwrap();
    fs::write(
        root.join(".dx").join("check").join(name),
        format!(
            r#"
rule_pack(id={pack_id} version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
{categories}
)

rules[id category severity weight metric op threshold docs provenance](
{rules}
)
"#
        ),
    )
    .unwrap();
}

fn read_only_options() -> DxCheckEngineOptions {
    DxCheckEngineOptions {
        allow_writes: false,
        ..DxCheckEngineOptions::default()
    }
}
