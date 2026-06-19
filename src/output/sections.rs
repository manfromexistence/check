use serializer::{DxLlmValue, DxSection};

use crate::model::DxCheckEngineReport;

use super::labels::{
    measurement_label, rule_pack_status_label, score_status_label, severity_label,
    tool_target_label,
};

pub(super) fn score_section(report: &DxCheckEngineReport) -> DxSection {
    let mut score = DxSection::new(vec![
        "id".to_string(),
        "schema".to_string(),
        "profile".to_string(),
        "score".to_string(),
        "max_score".to_string(),
        "status".to_string(),
        "finding_weight_total".to_string(),
        "failures".to_string(),
        "warnings".to_string(),
        "info".to_string(),
    ]);
    let _ = score.add_row(vec![
        DxLlmValue::Str("engine-score".to_string()),
        DxLlmValue::Str(report.score.schema_version.clone()),
        DxLlmValue::Str(report.score.profile.clone()),
        DxLlmValue::Num(report.score.score as f64),
        DxLlmValue::Num(report.score.max_score as f64),
        DxLlmValue::Str(score_status_label(report.score.status)),
        DxLlmValue::Num(report.score.finding_weight_total as f64),
        DxLlmValue::Num(report.score.failure_count as f64),
        DxLlmValue::Num(report.score.warning_count as f64),
        DxLlmValue::Num(report.score.info_count as f64),
    ]);
    score
}

pub(super) fn score_buckets_section(report: &DxCheckEngineReport) -> DxSection {
    let mut buckets = DxSection::new(vec![
        "id".to_string(),
        "label".to_string(),
        "score".to_string(),
        "max_score".to_string(),
        "status".to_string(),
        "finding_weight_total".to_string(),
        "failures".to_string(),
        "warnings".to_string(),
        "info".to_string(),
    ]);
    for bucket in &report.score.buckets {
        let _ = buckets.add_row(vec![
            DxLlmValue::Str(bucket.id.clone()),
            DxLlmValue::Str(bucket.label.clone()),
            DxLlmValue::Num(bucket.score as f64),
            DxLlmValue::Num(bucket.max_score as f64),
            DxLlmValue::Str(score_status_label(bucket.status)),
            DxLlmValue::Num(bucket.finding_weight_total as f64),
            DxLlmValue::Num(bucket.failure_count as f64),
            DxLlmValue::Num(bucket.warning_count as f64),
            DxLlmValue::Num(bucket.info_count as f64),
        ]);
    }
    buckets
}

pub(super) fn rule_packs_section(report: &DxCheckEngineReport) -> DxSection {
    let mut rule_packs = DxSection::new(vec![
        "id".to_string(),
        "version".to_string(),
        "status".to_string(),
        "source".to_string(),
        "machine".to_string(),
        "rules".to_string(),
        "hash".to_string(),
        "registry_source".to_string(),
        "provenance".to_string(),
        "lock".to_string(),
        "signed".to_string(),
        "signer".to_string(),
        "signature".to_string(),
    ]);
    for pack in &report.rule_packs {
        let _ = rule_packs.add_row(vec![
            DxLlmValue::Str(pack.id.clone()),
            DxLlmValue::Str(pack.version.clone()),
            DxLlmValue::Str(rule_pack_status_label(&pack.status)),
            DxLlmValue::Str(pack.source_path.clone().unwrap_or_default()),
            DxLlmValue::Str(pack.machine_path.clone().unwrap_or_default()),
            DxLlmValue::Num(pack.rule_count as f64),
            DxLlmValue::Str(pack.source_hash.clone().unwrap_or_default()),
            DxLlmValue::Str(pack.registry_source.clone().unwrap_or_default()),
            DxLlmValue::Str(pack.provenance.clone().unwrap_or_default()),
            DxLlmValue::Str(pack.lock_status.clone().unwrap_or_default()),
            DxLlmValue::Str(
                pack.signed
                    .map(|signed| signed.to_string())
                    .unwrap_or_default(),
            ),
            DxLlmValue::Str(pack.signer.clone().unwrap_or_default()),
            DxLlmValue::Str(pack.signature_status.clone().unwrap_or_default()),
        ]);
    }
    rule_packs
}

pub(super) fn findings_section(report: &DxCheckEngineReport) -> DxSection {
    let mut findings = DxSection::new(vec![
        "id".to_string(),
        "category".to_string(),
        "severity".to_string(),
        "file".to_string(),
        "message".to_string(),
        "next_action".to_string(),
        "measurement".to_string(),
        "actual".to_string(),
        "threshold".to_string(),
        "weight".to_string(),
        "docs".to_string(),
        "provenance".to_string(),
    ]);
    for finding in &report.findings {
        let _ = findings.add_row(vec![
            DxLlmValue::Str(finding.id.clone()),
            DxLlmValue::Str(finding.category.clone()),
            DxLlmValue::Str(severity_label(finding.severity)),
            DxLlmValue::Str(finding.file.clone().unwrap_or_default()),
            DxLlmValue::Str(finding.message.clone()),
            DxLlmValue::Str(finding.next_action.clone()),
            DxLlmValue::Str(measurement_label(finding.measurement)),
            DxLlmValue::Str(finding.actual.clone().unwrap_or_default()),
            DxLlmValue::Str(finding.threshold.clone().unwrap_or_default()),
            DxLlmValue::Num(finding.weight as f64),
            DxLlmValue::Str(finding.docs.clone().unwrap_or_default()),
            DxLlmValue::Str(finding.provenance.clone().unwrap_or_default()),
        ]);
    }
    findings
}

pub(super) fn diagnostics_section(report: &DxCheckEngineReport) -> DxSection {
    let mut diagnostics = DxSection::new(vec![
        "id".to_string(),
        "source".to_string(),
        "severity".to_string(),
        "file".to_string(),
        "line".to_string(),
        "column".to_string(),
        "message".to_string(),
        "next_action".to_string(),
        "measurement".to_string(),
    ]);
    for diagnostic in &report.diagnostics {
        let _ = diagnostics.add_row(vec![
            DxLlmValue::Str(diagnostic.id.clone()),
            DxLlmValue::Str(diagnostic.source.clone()),
            DxLlmValue::Str(severity_label(diagnostic.severity)),
            DxLlmValue::Str(diagnostic.file.clone().unwrap_or_default()),
            optional_u32(diagnostic.line),
            optional_u32(diagnostic.column),
            DxLlmValue::Str(diagnostic.message.clone()),
            DxLlmValue::Str(diagnostic.next_action.clone()),
            DxLlmValue::Str(measurement_label(diagnostic.measurement)),
        ]);
    }
    diagnostics
}

pub(super) fn adapter_plans_section(report: &DxCheckEngineReport) -> DxSection {
    let mut plans = DxSection::new(vec![
        "id".to_string(),
        "target".to_string(),
        "executable".to_string(),
        "args".to_string(),
        "cwd".to_string(),
        "detected_from".to_string(),
        "parser".to_string(),
    ]);
    for plan in &report.adapter_plans {
        let _ = plans.add_row(vec![
            DxLlmValue::Str(plan.id.clone()),
            DxLlmValue::Str(tool_target_label(plan.target)),
            DxLlmValue::Str(plan.executable.clone()),
            DxLlmValue::Str(plan.args.join(" ")),
            DxLlmValue::Str(plan.cwd.display().to_string()),
            DxLlmValue::Str(plan.detected_from.join("|")),
            DxLlmValue::Str(plan.parser.clone()),
        ]);
    }
    plans
}

pub(super) fn test_inventory_section(report: &DxCheckEngineReport) -> DxSection {
    let mut inventory = DxSection::new(vec![
        "id".to_string(),
        "rust".to_string(),
        "js".to_string(),
        "python".to_string(),
        "go".to_string(),
        "c".to_string(),
        "cpp".to_string(),
    ]);
    let _ = inventory.add_row(vec![
        DxLlmValue::Str("tests".to_string()),
        DxLlmValue::Num(report.test_inventory.rust_tests as f64),
        DxLlmValue::Num(report.test_inventory.js_tests as f64),
        DxLlmValue::Num(report.test_inventory.python_tests as f64),
        DxLlmValue::Num(report.test_inventory.go_tests as f64),
        DxLlmValue::Num(report.test_inventory.c_tests as f64),
        DxLlmValue::Num(report.test_inventory.cpp_tests as f64),
    ]);
    inventory
}

pub(super) fn web_audit_targets_section(report: &DxCheckEngineReport) -> DxSection {
    let mut targets = DxSection::new(vec![
        "id".to_string(),
        "url".to_string(),
        "required_status".to_string(),
        "max_html_bytes".to_string(),
        "lighthouse".to_string(),
    ]);
    for target in &report.web_audit_targets {
        let _ = targets.add_row(vec![
            DxLlmValue::Str(target.id.clone()),
            DxLlmValue::Str(target.url.clone()),
            optional_u16(target.required_status),
            optional_u64(target.max_html_bytes),
            DxLlmValue::Str(
                target
                    .lighthouse_mode
                    .map(|mode| mode.as_str().to_string())
                    .unwrap_or_default(),
            ),
        ]);
    }
    targets
}

pub(super) fn web_audit_results_section(report: &DxCheckEngineReport) -> DxSection {
    let mut results = DxSection::new(vec![
        "id".to_string(),
        "target".to_string(),
        "url".to_string(),
        "status".to_string(),
        "html_bytes".to_string(),
        "response_time_ms".to_string(),
        "title".to_string(),
        "description".to_string(),
        "canonical".to_string(),
        "viewport".to_string(),
        "security_headers".to_string(),
        "source".to_string(),
    ]);
    for result in &report.web_audit_results {
        let _ = results.add_row(vec![
            DxLlmValue::Str(result.id.clone()),
            DxLlmValue::Str(result.target_id.clone()),
            DxLlmValue::Str(result.url.clone()),
            optional_u16(result.status),
            optional_u64(result.html_bytes),
            optional_u128(result.response_time_ms),
            DxLlmValue::Bool(result.title_present),
            DxLlmValue::Bool(result.description_present),
            DxLlmValue::Bool(result.canonical_present),
            DxLlmValue::Bool(result.viewport_present),
            DxLlmValue::Num(result.security_header_count as f64),
            DxLlmValue::Str(
                result
                    .source
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default(),
            ),
        ]);
    }
    results
}

pub(super) fn checked_paths_section(report: &DxCheckEngineReport) -> DxSection {
    let mut paths = DxSection::new(vec!["id".to_string(), "path".to_string()]);
    for (index, path) in report.checked_paths.iter().enumerate() {
        let _ = paths.add_row(vec![
            DxLlmValue::Num((index + 1) as f64),
            DxLlmValue::Str(path.clone()),
        ]);
    }
    paths
}

fn optional_u32(value: Option<u32>) -> DxLlmValue {
    DxLlmValue::Str(value.map_or_else(String::new, |value| value.to_string()))
}

fn optional_u16(value: Option<u16>) -> DxLlmValue {
    DxLlmValue::Str(value.map_or_else(String::new, |value| value.to_string()))
}

fn optional_u64(value: Option<u64>) -> DxLlmValue {
    DxLlmValue::Str(value.map_or_else(String::new, |value| value.to_string()))
}

fn optional_u128(value: Option<u128>) -> DxLlmValue {
    DxLlmValue::Str(value.map_or_else(String::new, |value| value.to_string()))
}
