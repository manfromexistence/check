use std::fs;

use tempfile::tempdir;

use crate::{analyze_project, model::DxCheckEngineOptions};

#[test]
fn default_rules_do_not_flag_ai_maintainable_structure_when_orientation_exists() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(temp.path().join("TODO.md"), "- [ ] Ship\n").unwrap();
    fs::write(temp.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();
    fs::write(
        temp.path().join("dx"),
        r#"
name="demo"
kind="dx-project"
score_max=500
"#,
    )
    .unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("dx.machine")
            .is_file(),
        "extensionless dx config should generate a serializer machine artifact"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "ai-maintainable-project-structure"),
        "AI-maintainable structure should not be reported when orientation files exist"
    );
}

#[test]
fn default_rules_flag_missing_ai_maintainable_project_structure() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "ai-maintainable-project-structure")
        .expect("missing AI-maintainable project structure finding");
    assert_eq!(finding.file, None);
    assert_eq!(finding.actual.as_deref(), Some("0"));
    assert_eq!(finding.threshold.as_deref(), Some("1"));
    assert!(finding.message.contains("missing"));
    assert!(finding.next_action.contains("README.md"));
}

#[test]
fn default_rules_flag_missing_test_readiness_when_no_tests() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "test-readiness-missing")
        .expect("missing test readiness finding");
    assert_eq!(finding.category, "test-readiness");
    assert_eq!(finding.file, None);
    assert_eq!(finding.actual.as_deref(), Some("0"));
    assert_eq!(finding.threshold.as_deref(), Some("1"));
    assert!(finding.message.contains("No tests were discovered"));
    assert!(report.score.score < report.score.max_score);
}

#[test]
fn default_rules_do_not_flag_test_readiness_when_tests_exist() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(
        temp.path().join("src").join("lib.rs"),
        "#[test]\nfn smoke() {}\n",
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(report.test_inventory.rust_tests, 1);
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "test-readiness-missing"),
        "projects with discovered tests should not be penalized"
    );
}

#[test]
fn default_rules_ignore_vendored_tests_for_project_readiness() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("third_party").join("widgets")).unwrap();
    fs::create_dir_all(temp.path().join("vendor").join("runtime")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(
        temp.path()
            .join("third_party")
            .join("widgets")
            .join("widget.test.ts"),
        "test('vendored widget', () => {})\n",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join("vendor")
            .join("runtime")
            .join("runtime_test.cpp"),
        "TEST(Runtime, VendorSmoke) {}\n",
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(report.test_inventory.js_tests, 0);
    assert_eq!(report.test_inventory.cpp_tests, 0);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "test-readiness-missing"),
        "vendored tests must not satisfy project-owned test readiness"
    );
}

#[test]
fn default_rules_ignore_generated_c_family_tests_for_project_readiness() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(
        temp.path().join("src").join("schema.generated.cpp"),
        "TEST(GeneratedSchema, Smoke) {}\n",
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(report.test_inventory.cpp_tests, 0);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "generated-source-leak"
                && finding.file.as_deref() == Some("src/schema.generated.cpp")),
        "generated source should still be reported as a source-ownership finding"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "test-readiness-missing"),
        "generated tests must not satisfy project-owned test readiness"
    );
}

#[test]
fn default_rules_treat_dx_worker_contract_files_as_ai_orientation() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("AGENTS.md"),
        "Keep source-owned intent visible for future DX workers.\n",
    )
    .unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "ai-maintainable-project-structure"),
        "AGENTS.md should count as AI-maintainable worker orientation"
    );
}
