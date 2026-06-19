use std::collections::HashSet;
use std::path::Path;

use crate::model::{
    DxDiagnostic, DxMeasurementKind, DxRuleCategoryDefinition, DxRuleDefinition, DxSeverity,
};

pub(super) fn validate_rule_pack_rules(
    source: &Path,
    categories: &[DxRuleCategoryDefinition],
    rules: &[DxRuleDefinition],
) -> Vec<DxDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_categories = categories
        .iter()
        .map(|category| category.id.as_str())
        .collect::<HashSet<_>>();
    if !rules.is_empty() && categories.is_empty() {
        diagnostics.push(missing_categories(source));
    }
    for rule in rules {
        if !declared_categories.is_empty() && !declared_categories.contains(rule.category.as_str())
        {
            diagnostics.push(unknown_category(source, categories, rule));
        }
        if !known_metric(&rule.metric) {
            diagnostics.push(unknown_metric(source, rule));
        }
        if !known_operator(&rule.operator) {
            diagnostics.push(unknown_operator(source, rule));
        }
    }
    diagnostics
}

fn missing_categories(source: &Path) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-categories-missing".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Warning,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: "Rule pack has scoring rules but no categories[id label weight] table; score buckets will use derived category fallbacks."
            .to_string(),
        next_action: "Add a serializer .sr categories[id label weight] table with one row for each rule category to control bucket labels and weights."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn unknown_category(
    source: &Path,
    categories: &[DxRuleCategoryDefinition],
    rule: &DxRuleDefinition,
) -> DxDiagnostic {
    let declared = categories
        .iter()
        .map(|category| category.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    DxDiagnostic {
        id: "rule-pack-unknown-category".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Warning,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule `{}` uses undeclared category `{}`; declared categories: {}",
            rule.id, rule.category, declared
        ),
        next_action: "Use one of the declared serializer .sr categories or add a category row before scoring the pack."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn unknown_metric(source: &Path, rule: &DxRuleDefinition) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-unknown-metric".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Warning,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule `{}` uses unknown metric `{}` and will not be evaluated",
            rule.id, rule.metric
        ),
        next_action: "Use a supported declarative DX Check metric or update the engine registry."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn unknown_operator(source: &Path, rule: &DxRuleDefinition) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-unknown-operator".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Warning,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule `{}` uses unknown operator `{}` and will not be evaluated",
            rule.id, rule.operator
        ),
        next_action: "Use a supported declarative DX Check operator or update the engine registry."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn known_metric(metric: &str) -> bool {
    matches!(
        metric,
        "line_count"
            | "byte_size"
            | "node_modules"
            | "generated_machine"
            | "generated_source"
            | "naming_convention"
            | "component_lines"
            | "component_boundary"
            | "component_quality"
            | "rust_unwraps"
            | "insecure_source"
            | "project_structure"
            | "test_count"
            | "web_http_status"
            | "web_html_bytes"
            | "web_title_present"
            | "web_description_present"
            | "web_canonical_present"
            | "web_viewport_present"
            | "web_security_header_count"
    )
}

fn known_operator(operator: &str) -> bool {
    matches!(
        operator,
        "max" | "<=" | "min" | ">=" | "eq" | "=" | "==" | "absent" | "present"
    )
}
