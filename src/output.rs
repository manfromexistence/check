use crate::model::DxCheckEngineReport;
use serializer::{DxDocument, DxLlmValue, MachineFormat};

mod labels;
mod sections;
#[cfg(test)]
mod tests;

use labels::score_status_label;
use sections::{
    adapter_plans_section, checked_paths_section, diagnostics_section, findings_section,
    rule_packs_section, score_buckets_section, score_section, test_inventory_section,
    web_audit_results_section, web_audit_targets_section,
};

pub fn report_to_llm(report: &DxCheckEngineReport) -> String {
    serializer::document_to_llm(&report_to_document(report))
}

pub fn report_to_machine(report: &DxCheckEngineReport) -> MachineFormat {
    serializer::document_to_machine(&report_to_document(report))
}

pub fn report_to_document(report: &DxCheckEngineReport) -> DxDocument {
    let mut document = DxDocument::new();
    document.context.insert(
        "dx_check_engine.schema".to_string(),
        DxLlmValue::Str("dx.check.engine_report.v1".to_string()),
    );
    document.context.insert(
        "dx_check_engine.rule_pack_count".to_string(),
        DxLlmValue::Num(report.rule_packs.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.finding_count".to_string(),
        DxLlmValue::Num(report.findings.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.diagnostic_count".to_string(),
        DxLlmValue::Num(report.diagnostics.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.adapter_plan_count".to_string(),
        DxLlmValue::Num(report.adapter_plans.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.web_audit_target_count".to_string(),
        DxLlmValue::Num(report.web_audit_targets.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.web_audit_result_count".to_string(),
        DxLlmValue::Num(report.web_audit_results.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.checked_path_count".to_string(),
        DxLlmValue::Num(report.checked_paths.len() as f64),
    );
    document.context.insert(
        "dx_check_engine.score".to_string(),
        DxLlmValue::Num(report.score.score as f64),
    );
    document.context.insert(
        "dx_check_engine.max_score".to_string(),
        DxLlmValue::Num(report.score.max_score as f64),
    );
    document.context.insert(
        "dx_check_engine.score_status".to_string(),
        DxLlmValue::Str(score_status_label(report.score.status)),
    );
    document.context.insert(
        "dx_check_engine.score_profile".to_string(),
        DxLlmValue::Str(report.score.profile.clone()),
    );
    document.context.insert(
        "dx_check_engine.score_bucket_count".to_string(),
        DxLlmValue::Num(report.score.buckets.len() as f64),
    );

    insert_named_section(&mut document, 's', "score", score_section(report));
    insert_named_section(
        &mut document,
        'b',
        "score_buckets",
        score_buckets_section(report),
    );
    insert_named_section(&mut document, 'r', "rule_packs", rule_packs_section(report));
    insert_named_section(&mut document, 'f', "findings", findings_section(report));
    insert_named_section(
        &mut document,
        'd',
        "diagnostics",
        diagnostics_section(report),
    );
    insert_named_section(
        &mut document,
        'a',
        "adapter_plans",
        adapter_plans_section(report),
    );
    insert_named_section(
        &mut document,
        'w',
        "web_audit_targets",
        web_audit_targets_section(report),
    );
    insert_named_section(
        &mut document,
        'u',
        "web_audit_results",
        web_audit_results_section(report),
    );
    insert_named_section(
        &mut document,
        't',
        "test_inventory",
        test_inventory_section(report),
    );
    insert_named_section(
        &mut document,
        'p',
        "checked_paths",
        checked_paths_section(report),
    );

    document
}

fn insert_named_section(
    document: &mut DxDocument,
    key: char,
    name: &str,
    section: serializer::DxSection,
) {
    document.sections.insert(key, section);
    document.section_names.insert(key, name.to_string());
}
