pub mod adapters;
pub mod diagnostics;
pub mod inventory;
mod languages;
pub mod litehouse;
pub mod model;
pub mod output;
mod path_filters;
pub mod registry;
pub mod rule_pack;
pub mod rules;
pub mod scoring;
pub mod syntax;
pub mod testing;
pub mod web_audit;
pub mod web_audit_runner;
pub mod web_lighthouse;

use std::path::Path;

use anyhow::Result;

pub use model::{
    DxCheckEngineOptions, DxCheckEngineReport, DxCheckOutputFormat, DxDiagnostic, DxFinding,
    DxMeasurementKind, DxRuleCategoryDefinition, DxRulePackStatus, DxScoreBucketSummary,
    DxScoreStatus, DxScoreSummary, DxSeverity, DxToolTarget, DxWebAuditResult, DxWebAuditTarget,
};
pub use scoring::{summarize_score, summarize_score_with_categories};

pub fn analyze_project(
    root: impl AsRef<Path>,
    options: DxCheckEngineOptions,
) -> Result<DxCheckEngineReport> {
    let root = root.as_ref();
    let inventory = inventory::scan_project(root)?;
    let loaded_rules = rules::load_rule_pack_set_with_options(
        root,
        rules::RulePackLoadOptions {
            allow_writes: options.allow_writes,
            strict_rule_packs: options.strict_rule_packs,
        },
    )?;
    let test_inventory = testing::discover_tests(root);
    let web_audit = web_audit::load_project_web_audit(root);
    let rule_categories = loaded_rules.categories;
    let mut report = DxCheckEngineReport {
        score: Default::default(),
        rule_packs: loaded_rules.summaries,
        findings: rules::evaluate_rules(
            &inventory,
            &test_inventory,
            &web_audit.results,
            &loaded_rules.rules,
        ),
        diagnostics: loaded_rules.diagnostics,
        adapter_plans: adapters::plan_tools(root, &options.run_targets),
        web_audit_targets: web_audit.targets,
        web_audit_results: web_audit.results,
        test_inventory,
        checked_paths: inventory
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect(),
    };
    report
        .diagnostics
        .extend(syntax::syntax_diagnostics(&inventory));
    report.diagnostics.extend(web_audit.diagnostics);
    report.diagnostics.extend(
        report
            .adapter_plans
            .iter()
            .filter_map(adapters::blocked_adapter_plan_diagnostic),
    );
    report.score = scoring::summarize_score_with_categories(
        &report.findings,
        &report.diagnostics,
        &rule_categories,
    );

    for finding in &report.findings {
        report.diagnostics.push(DxDiagnostic {
            id: finding.id.clone(),
            source: "dx-check-rule".to_string(),
            severity: finding.severity,
            file: finding.file.clone(),
            line: None,
            column: None,
            message: finding.message.clone(),
            next_action: finding.next_action.clone(),
            measurement: finding.measurement,
        });
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::{
        DxCheckEngineOptions, DxMeasurementKind, DxScoreStatus, DxSeverity, DxToolTarget,
        analyze_project,
    };

    #[test]
    fn analyze_project_includes_structured_syntax_diagnostics() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("package.json"), "{ \"scripts\": [ }").unwrap();
        fs::write(temp.path().join("workflow.yml"), "jobs:\n  build: [").unwrap();

        let report = analyze_project(
            temp.path(),
            DxCheckEngineOptions {
                allow_writes: false,
                ..DxCheckEngineOptions::default()
            },
        )
        .unwrap();

        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "json-syntax-error"
                && diagnostic.file.as_deref() == Some("package.json")
        }));
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "yaml-syntax-error"
                && diagnostic.file.as_deref() == Some("workflow.yml")
        }));
    }

    #[test]
    fn legacy_engine_report_json_defaults_score_summary() {
        let report: crate::DxCheckEngineReport = serde_json::from_str(
            r#"{
  "rule_packs": [],
  "findings": [],
  "diagnostics": [],
  "adapter_plans": [],
  "test_inventory": {
    "rust_tests": 0,
    "js_tests": 0,
    "python_tests": 0,
    "go_tests": 0
  },
  "checked_paths": []
}"#,
        )
        .expect("legacy engine report");

        assert_eq!(report.score.score, 500);
        assert_eq!(report.score.max_score, 500);
    }

    #[test]
    fn analyze_project_populates_score_from_findings_and_real_diagnostics() {
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
tiny-line-budget structure warning 7 line_count max 1 docs/check/tiny.md local
)
"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("src").join("large.rs"),
            "fn one() {}\nfn two() {}\n",
        )
        .unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        fs::write(temp.path().join("package.json"), "{ \"scripts\": [ }").unwrap();

        let report = analyze_project(
            temp.path(),
            DxCheckEngineOptions {
                allow_writes: false,
                ..DxCheckEngineOptions::default()
            },
        )
        .unwrap();

        assert_eq!(report.score.finding_weight_total, 7);
        assert_eq!(report.score.score, 493);
        assert_eq!(report.score.max_score, 500);
        assert_eq!(report.score.warning_count, 1);
        assert!(
            report.score.failure_count >= 1,
            "JSON syntax diagnostics should block the engine score status"
        );
        assert_eq!(report.score.status, crate::model::DxScoreStatus::Blocked);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "tiny-line-budget"
                && diagnostic.source == "dx-check-rule"
                && diagnostic.severity == DxSeverity::Warning
        }));
    }

    #[test]
    fn analyze_project_scores_blocked_javascript_adapter_plans() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        fs::write(temp.path().join("app.ts"), "export const app = true;\n").unwrap();
        fs::write(temp.path().join("bun.lock"), "").unwrap();
        fs::write(temp.path().join("package-lock.json"), "{}\n").unwrap();
        fs::write(
            temp.path().join("package.json"),
            r#"{
  "scripts": {
    "lint": "eslint ."
  },
  "devDependencies": {
    "eslint": "^9.0.0"
  }
}"#,
        )
        .unwrap();

        let report = analyze_project(
            temp.path(),
            DxCheckEngineOptions {
                allow_writes: false,
                run_targets: vec![DxToolTarget::Lint],
                ..DxCheckEngineOptions::default()
            },
        )
        .unwrap();

        let plan = report
            .adapter_plans
            .iter()
            .find(|plan| plan.id == "js-lint")
            .expect("blocked JavaScript lint adapter plan");
        assert_eq!(plan.executable, "dx-check-blocked");

        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.id == "js-lint:adapter-blocked")
            .expect("blocked JavaScript adapter diagnostic");
        assert_eq!(diagnostic.source, "js-lint");
        assert_eq!(diagnostic.severity, DxSeverity::Failure);
        assert_eq!(diagnostic.measurement, DxMeasurementKind::Skipped);
        assert!(diagnostic.message.contains("multiple JavaScript lockfiles"));
        assert_eq!(report.score.status, DxScoreStatus::Blocked);
        assert!(report.score.failure_count >= 1);
    }

    #[test]
    fn analyze_project_scores_blocked_javascript_write_risk_scripts() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
        fs::write(temp.path().join("app.ts"), "export const app = true;\n").unwrap();
        fs::write(
            temp.path().join("package.json"),
            r#"{
  "scripts": {
    "lint": "eslint . --fix"
  },
  "devDependencies": {
    "eslint": "^9.0.0"
  }
}"#,
        )
        .unwrap();

        let report = analyze_project(
            temp.path(),
            DxCheckEngineOptions {
                allow_writes: false,
                run_targets: vec![DxToolTarget::Lint],
                ..DxCheckEngineOptions::default()
            },
        )
        .unwrap();

        let plan = report
            .adapter_plans
            .iter()
            .find(|plan| plan.id == "js-lint")
            .expect("blocked JavaScript lint adapter plan");
        assert_eq!(plan.executable, "dx-check-blocked");

        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.id == "js-lint:adapter-blocked")
            .expect("blocked JavaScript adapter diagnostic");
        assert_eq!(diagnostic.severity, DxSeverity::Failure);
        assert!(diagnostic.message.contains("--fix"));
        assert_eq!(report.score.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn analyze_project_populates_category_score_buckets_from_sr_categories() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".dx").join("check")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(
            temp.path().join(".dx").join("check").join("local.sr"),
            r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 70
test-readiness "Test readiness" 30
)

rules[id category severity weight metric op threshold docs provenance](
component-budget structure warning 23 component_lines max 1 docs/check/components.md local
missing-tests test-readiness warning 10 test_count min 1 docs/check/tests.md local
)
"#,
        )
        .unwrap();
        fs::write(
            temp.path().join("src").join("Widget.tsx"),
            "export function Widget() {\n  return <div />\n}\n",
        )
        .unwrap();

        let report = analyze_project(
            temp.path(),
            DxCheckEngineOptions {
                allow_writes: false,
                ..DxCheckEngineOptions::default()
            },
        )
        .unwrap();

        let structure = report
            .score
            .buckets
            .iter()
            .find(|bucket| bucket.id == "structure")
            .expect("structure category bucket");
        assert_eq!(structure.label, "Structure");
        assert_eq!(structure.score, 47);
        assert_eq!(structure.max_score, 70);
        assert_eq!(structure.finding_weight_total, 23);
        assert_eq!(structure.warning_count, 1);

        let tests = report
            .score
            .buckets
            .iter()
            .find(|bucket| bucket.id == "test-readiness")
            .expect("test-readiness category bucket");
        assert_eq!(tests.label, "Test readiness");
        assert_eq!(tests.score, 20);
        assert_eq!(tests.max_score, 30);
        assert_eq!(tests.finding_weight_total, 10);
        assert_eq!(tests.warning_count, 1);
    }
}
