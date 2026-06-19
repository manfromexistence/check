use std::fs;

use dx_check_engine::{DxCheckEngineOptions, DxSeverity, analyze_project};
use tempfile::tempdir;

mod support;

use support::rule_pack::write_rule_pack;

#[test]
fn missing_required_rule_column_is_diagnostic_not_non_rule_skip() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src").join("small.rs"), "fn one() {}\n").unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("missing-op-column.sr"),
        r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric threshold docs provenance](
missing-op-column structure warning 8 line_count 1 docs/check/bad.md local
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-rule-invalid"
            && diagnostic.message.contains("missing required column `op`")
    }));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "rule-pack-skipped-non-rule-source"),
        "marked DX Check rule packs with malformed rules tables should not be mislabeled as non-rule"
    );
}

#[test]
fn missing_required_category_column_is_category_diagnostic_not_rule_table() {
    let root = tempdir().unwrap();
    write_category_pack(
        root.path(),
        "missing-category-weight.sr",
        "categories[id label](\nstructure Structure\n)",
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.id == "rule-pack-category-invalid"
                && diagnostic
                    .message
                    .contains("missing required column `weight`")
        })
        .expect("missing category-column diagnostic");

    assert!(diagnostic.message.contains("categories table"));
    assert!(!diagnostic.message.contains("rules table"));
    assert!(
        !report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "rule-pack-rule-invalid"
                && diagnostic
                    .message
                    .contains("missing required column `weight`")
        }),
        "category table errors must not be reported as rule-row errors"
    );
}

#[test]
fn invalid_category_weight_is_category_diagnostic_not_rule_row() {
    let root = tempdir().unwrap();
    write_category_pack(
        root.path(),
        "bad-category-weight.sr",
        "categories[id label weight](\nstructure Structure too-heavy\n)",
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.id == "rule-pack-category-invalid"
                && diagnostic.message.contains("structure")
                && diagnostic.message.contains("weight")
        })
        .expect("missing category-weight diagnostic");

    assert!(diagnostic.message.contains("category row"));
    assert!(!diagnostic.message.contains("Rule pack row"));
    assert!(diagnostic.next_action.contains("categories table"));
}

#[test]
fn undeclared_rule_category_warns_without_blocking_scoring() {
    let root = tempdir().unwrap();
    write_rule_pack(
        root.path(),
        "undeclared-category.sr",
        "typo-category-line-budget strucutre warning 8 line_count max 1 docs/check/local.md local",
    );

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-unknown-category"
            && diagnostic.severity == DxSeverity::Warning
            && diagnostic.message.contains("typo-category-line-budget")
            && diagnostic.message.contains("strucutre")
            && diagnostic.message.contains("structure")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "typo-category-line-budget"
            && finding.category == "strucutre"
            && finding.file.as_deref() == Some("src/small.rs")
    }));
    assert!(
        report
            .score
            .buckets
            .iter()
            .any(|bucket| bucket.id == "strucutre"),
        "score buckets should retain the derived fallback for unknown categories"
    );
}

#[test]
fn missing_categories_table_warns_without_blocking_scoring() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("small.rs"),
        "fn one() {}\n".repeat(6),
    )
    .unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("no-categories.sr"),
        r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

rules[id category severity weight metric op threshold docs provenance](
line-budget structure warning 8 line_count max 1 docs/check/local.md local
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-categories-missing"
            && diagnostic.severity == DxSeverity::Warning
            && diagnostic.message.contains("categories[id label weight]")
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("no-categories.sr"))
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "line-budget"
            && finding.category == "structure"
            && finding.file.as_deref() == Some("src/small.rs")
    }));
    assert!(
        report
            .score
            .buckets
            .iter()
            .any(|bucket| bucket.id == "structure" && bucket.label == "structure"),
        "category-less packs should keep the derived score bucket fallback"
    );
}

fn write_category_pack(root: &std::path::Path, name: &str, categories_table: &str) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("small.rs"), "fn one() {}\n").unwrap();
    fs::write(
        root.join(".dx").join("check").join(name),
        format!(
            r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

{categories_table}

rules[id category severity weight metric op threshold docs provenance](
line-budget structure warning 8 line_count max 1 docs/check/local.md local
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
