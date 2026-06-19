use std::path::PathBuf;

use serializer::DxLlmValue;

use crate::model::{
    DxCheckEngineReport, DxDiagnostic, DxFinding, DxMeasurementKind, DxRulePackStatus,
    DxRulePackSummary, DxScoreBucketSummary, DxScoreStatus, DxScoreSummary, DxSeverity,
    DxTestInventory, DxToolPlan, DxToolTarget, DxWebAuditResult, DxWebAuditTarget,
    DxWebLighthouseMode,
};
use crate::output::{report_to_llm, report_to_machine};

#[test]
fn report_to_llm_emits_full_engine_sections() {
    let report = sample_report();
    let llm = report_to_llm(&report);
    let document = serializer::llm_to_document(&llm).unwrap();

    assert!(document.section_by_name("rule_packs").is_some());
    assert!(document.section_by_name("findings").is_some());
    assert!(document.section_by_name("diagnostics").is_some());
    assert!(document.section_by_name("adapter_plans").is_some());
    assert!(document.section_by_name("web_audit_targets").is_some());
    assert!(document.section_by_name("web_audit_results").is_some());
    assert!(document.section_by_name("test_inventory").is_some());
    assert!(document.section_by_name("checked_paths").is_some());
    assert!(document.section_by_name("score").is_some());
    assert!(document.section_by_name("score_buckets").is_some());
    assert_eq!(
        document
            .context
            .get("dx_check_engine.score")
            .and_then(DxLlmValue::as_num),
        Some(500.0)
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.score_status")
            .and_then(DxLlmValue::as_str),
        Some("ready")
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.max_score")
            .and_then(DxLlmValue::as_num),
        Some(500.0)
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.score_profile")
            .and_then(DxLlmValue::as_str),
        Some("dx-check-engine.rules.v1")
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.web_audit_target_count")
            .and_then(DxLlmValue::as_num),
        Some(1.0)
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.web_audit_result_count")
            .and_then(DxLlmValue::as_num),
        Some(1.0)
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.score_bucket_count")
            .and_then(DxLlmValue::as_num),
        Some(1.0)
    );
    let score = document.section_by_name("score").unwrap();
    assert_eq!(
        score
            .value_by_key("id", "engine-score", "schema")
            .and_then(DxLlmValue::as_str),
        Some("dx.check.engine_score.v1")
    );
    assert_eq!(
        document
            .section_by_name("score_buckets")
            .unwrap()
            .value_by_key("id", "structure", "score")
            .unwrap()
            .as_num(),
        Some(92.0)
    );
    assert_eq!(
        document
            .section_by_name("rule_packs")
            .unwrap()
            .value_by_key("id", "local-check", "machine")
            .unwrap()
            .as_str(),
        Some(".dx/serializer/check-local.machine")
    );
    assert_eq!(
        document
            .section_by_name("rule_packs")
            .unwrap()
            .value_by_key("id", "local-check", "provenance")
            .unwrap()
            .as_str(),
        Some("forge/demo")
    );
    assert_eq!(
        document
            .section_by_name("rule_packs")
            .unwrap()
            .value_by_key("id", "local-check", "lock")
            .unwrap()
            .as_str(),
        Some("locked")
    );
    assert_eq!(
        document
            .section_by_name("rule_packs")
            .unwrap()
            .value_by_key("id", "local-check", "signer")
            .unwrap()
            .as_str(),
        Some("forge-test-key")
    );
    assert_eq!(
        document
            .section_by_name("rule_packs")
            .unwrap()
            .value_by_key("id", "local-check", "signature")
            .unwrap()
            .as_str(),
        Some("verified")
    );
    assert_eq!(
        document
            .section_by_name("web_audit_targets")
            .unwrap()
            .value_by_key("id", "home", "url")
            .unwrap()
            .as_str(),
        Some("http://localhost:3000/")
    );
    assert_eq!(
        document
            .section_by_name("web_audit_targets")
            .unwrap()
            .value_by_key("id", "home", "lighthouse")
            .unwrap()
            .as_str(),
        Some("auto")
    );
    assert_eq!(
        document
            .section_by_name("web_audit_results")
            .unwrap()
            .value_by_key("id", "home-run", "security_headers")
            .unwrap()
            .as_num(),
        Some(4.0)
    );
}

#[test]
fn report_to_machine_round_trips_full_engine_sections() {
    let machine = report_to_machine(&sample_report());
    let document = serializer::machine_to_document(&machine).unwrap();

    assert!(document.section_by_name("diagnostics").is_some());
    assert!(document.section_by_name("adapter_plans").is_some());
    assert!(document.section_by_name("web_audit_targets").is_some());
    assert!(document.section_by_name("web_audit_results").is_some());
    assert_eq!(
        document
            .section_by_name("score")
            .unwrap()
            .value_by_key("id", "engine-score", "score")
            .unwrap()
            .as_num(),
        Some(500.0)
    );
    assert_eq!(
        document
            .section_by_name("score_buckets")
            .unwrap()
            .value_by_key("id", "structure", "status")
            .unwrap()
            .as_str(),
        Some("warning")
    );
    assert_eq!(
        document
            .section_by_name("test_inventory")
            .unwrap()
            .value_by_key("id", "tests", "rust")
            .unwrap()
            .as_num(),
        Some(2.0)
    );
    assert_eq!(
        document
            .section_by_name("checked_paths")
            .unwrap()
            .value_by_key("id", "1", "path")
            .unwrap()
            .as_str(),
        Some("src/small.rs")
    );
    assert_eq!(
        document
            .section_by_name("web_audit_results")
            .unwrap()
            .value_by_key("id", "home-run", "source")
            .unwrap()
            .as_str(),
        Some(".dx/receipts/check/web-home.json")
    );
}

#[test]
fn report_to_machine_keeps_blocked_adapter_diagnostics_and_status() {
    let reason = "multiple JavaScript lockfiles were found; set packageManager in package.json";
    let mut report = sample_report();
    report.score.status = DxScoreStatus::Blocked;
    report.score.failure_count = 1;
    report.diagnostics = vec![DxDiagnostic {
        id: "js-lint:adapter-blocked".to_string(),
        source: "js-lint".to_string(),
        severity: DxSeverity::Failure,
        file: None,
        line: None,
        column: None,
        message: format!("Adapter plan `js-lint` was blocked: {reason}"),
        next_action:
            "Resolve the adapter configuration or toolchain evidence, then rerun dx check."
                .to_string(),
        measurement: DxMeasurementKind::Skipped,
    }];
    report.adapter_plans = vec![DxToolPlan {
        id: "js-lint".to_string(),
        target: DxToolTarget::Lint,
        executable: "dx-check-blocked".to_string(),
        args: vec![reason.to_string()],
        cwd: PathBuf::from("G:\\Dx\\demo"),
        detected_from: vec![
            "package.json".to_string(),
            "bun.lock".to_string(),
            "package-lock.json".to_string(),
        ],
        parser: "blocked".to_string(),
    }];

    let machine = report_to_machine(&report);
    let document = serializer::machine_to_document(&machine).unwrap();

    assert_eq!(
        document
            .context
            .get("dx_check_engine.score_status")
            .and_then(DxLlmValue::as_str),
        Some("blocked")
    );
    assert_eq!(
        document
            .context
            .get("dx_check_engine.diagnostic_count")
            .and_then(DxLlmValue::as_num),
        Some(1.0)
    );
    assert_eq!(
        document
            .section_by_name("score")
            .unwrap()
            .value_by_key("id", "engine-score", "status")
            .unwrap()
            .as_str(),
        Some("blocked")
    );
    assert_eq!(
        document
            .section_by_name("score")
            .unwrap()
            .value_by_key("id", "engine-score", "failures")
            .unwrap()
            .as_num(),
        Some(1.0)
    );
    assert_eq!(
        document
            .section_by_name("diagnostics")
            .unwrap()
            .value_by_key("id", "js-lint:adapter-blocked", "severity")
            .unwrap()
            .as_str(),
        Some("failure")
    );
    assert_eq!(
        document
            .section_by_name("diagnostics")
            .unwrap()
            .value_by_key("id", "js-lint:adapter-blocked", "measurement")
            .unwrap()
            .as_str(),
        Some("skipped")
    );
    assert_eq!(
        document
            .section_by_name("adapter_plans")
            .unwrap()
            .value_by_key("id", "js-lint", "executable")
            .unwrap()
            .as_str(),
        Some("dx-check-blocked")
    );
}

fn sample_report() -> DxCheckEngineReport {
    let mut score = DxScoreSummary::default();
    score.buckets = vec![DxScoreBucketSummary {
        id: "structure".to_string(),
        label: "Structure".to_string(),
        score: 92,
        max_score: 100,
        status: DxScoreStatus::Warning,
        finding_weight_total: 8,
        failure_count: 0,
        warning_count: 1,
        info_count: 0,
    }];

    DxCheckEngineReport {
        score,
        rule_packs: vec![DxRulePackSummary {
            id: "local-check".to_string(),
            version: "1".to_string(),
            status: DxRulePackStatus::MachineFresh,
            source_path: Some(".dx/check/local.sr".to_string()),
            machine_path: Some(".dx/serializer/check-local.machine".to_string()),
            source_hash: Some("abc123".to_string()),
            registry_source: Some("r2://forge/check/demo".to_string()),
            provenance: Some("forge/demo".to_string()),
            lock_status: Some("locked".to_string()),
            signed: Some(true),
            signer: Some("forge-test-key".to_string()),
            signature_status: Some("verified".to_string()),
            rule_count: 1,
        }],
        findings: vec![DxFinding {
            id: "tiny-line-budget".to_string(),
            category: "structure".to_string(),
            severity: DxSeverity::Warning,
            message: "src/small.rs has 21 lines".to_string(),
            next_action: "Split the file.".to_string(),
            measurement: DxMeasurementKind::Measured,
            file: Some("src/small.rs".to_string()),
            actual: Some("21".to_string()),
            threshold: Some("20".to_string()),
            weight: 8,
            docs: Some("docs/check/tiny.md".to_string()),
            provenance: Some("local".to_string()),
        }],
        diagnostics: vec![DxDiagnostic {
            id: "cargo-check:error".to_string(),
            source: "cargo-check".to_string(),
            severity: DxSeverity::Failure,
            file: Some("src/small.rs".to_string()),
            line: Some(7),
            column: Some(3),
            message: "expected item".to_string(),
            next_action: "Fix the Rust diagnostic.".to_string(),
            measurement: DxMeasurementKind::Measured,
        }],
        adapter_plans: vec![DxToolPlan {
            id: "cargo-check".to_string(),
            target: DxToolTarget::Typecheck,
            executable: "cargo".to_string(),
            args: vec!["check".to_string(), "-j".to_string(), "1".to_string()],
            cwd: PathBuf::from("G:\\Dx\\demo"),
            detected_from: vec!["Cargo.toml".to_string()],
            parser: "cargo-json".to_string(),
        }],
        web_audit_targets: vec![DxWebAuditTarget {
            id: "home".to_string(),
            url: "http://localhost:3000/".to_string(),
            required_status: Some(200),
            max_html_bytes: Some(200000),
            lighthouse_mode: Some(DxWebLighthouseMode::Auto),
        }],
        web_audit_results: vec![DxWebAuditResult {
            id: "home-run".to_string(),
            target_id: "home".to_string(),
            url: "http://localhost:3000/".to_string(),
            status: Some(200),
            final_url: Some("http://localhost:3000/".to_string()),
            response_time_ms: Some(42),
            html_bytes: Some(18200),
            title_present: true,
            description_present: true,
            canonical_present: true,
            viewport_present: true,
            security_header_count: 4,
            source: Some(PathBuf::from(".dx/receipts/check/web-home.json")),
        }],
        test_inventory: DxTestInventory {
            rust_tests: 2,
            js_tests: 1,
            python_tests: 0,
            go_tests: 0,
            c_tests: 1,
            cpp_tests: 1,
        },
        checked_paths: vec!["src/small.rs".to_string()],
    }
}
