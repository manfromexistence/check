use std::collections::HashSet;
use std::path::Path;

use serializer::{DxDocument, DxLlmValue};

use crate::model::{DxDiagnostic, DxRuleCategoryDefinition, DxRuleDefinition, DxSeverity};

use super::table::{
    RulePackTable, duplicate_category_id, duplicate_rule_id, invalid_table_row, optional_u64,
    required_column_index, required_string, required_u16, value_string,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedRulePackRules {
    pub categories: Vec<DxRuleCategoryDefinition>,
    pub rules: Vec<DxRuleDefinition>,
    pub diagnostics: Vec<DxDiagnostic>,
}

pub fn rules_from_document(document: &DxDocument) -> Vec<DxRuleDefinition> {
    parse_rules_from_document(document, Path::new("<memory>")).rules
}

pub fn categories_from_document(document: &DxDocument) -> Vec<DxRuleCategoryDefinition> {
    parse_rules_from_document(document, Path::new("<memory>")).categories
}

pub fn parse_rules_from_document(document: &DxDocument, source: &Path) -> ParsedRulePackRules {
    let mut diagnostics = Vec::new();
    let categories = parse_categories_from_document(document, source, &mut diagnostics);

    let Some(section) = document.section_by_name("rules") else {
        return ParsedRulePackRules {
            categories,
            rules: Vec::new(),
            diagnostics,
        };
    };

    let Some(id_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "id",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };
    let Some(category_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "category",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };
    let Some(severity_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "severity",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };
    let Some(weight_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "weight",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };
    let Some(metric_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "metric",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };
    let Some(operator_index) = required_column_index(
        section,
        source,
        RulePackTable::Rules,
        "op",
        &mut diagnostics,
    ) else {
        return empty_rules(categories, diagnostics);
    };

    let threshold_index = section.column_index("threshold");
    let docs_index = section.column_index("docs");
    let provenance_index = section.column_index("provenance");
    let mut seen_rule_ids = HashSet::new();

    let rules = section
        .rows
        .iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            let row_label = row_label(row.get(id_index), row_index, "row");
            let id = required_string(
                source,
                RulePackTable::Rules,
                &row_label,
                "id",
                row.get(id_index),
                &mut diagnostics,
            )?;
            if !seen_rule_ids.insert(id.clone()) {
                diagnostics.push(duplicate_rule_id(source, &row_label, &id));
                return None;
            }
            let category = required_string(
                source,
                RulePackTable::Rules,
                &row_label,
                "category",
                row.get(category_index),
                &mut diagnostics,
            )?;
            let severity_text = required_string(
                source,
                RulePackTable::Rules,
                &row_label,
                "severity",
                row.get(severity_index),
                &mut diagnostics,
            )?;
            let severity = parse_severity(&severity_text).or_else(|| {
                diagnostics.push(invalid_table_row(
                    source,
                    RulePackTable::Rules,
                    &row_label,
                    format!("invalid severity `{severity_text}`"),
                    "Use one of: info, warning, failure.",
                ));
                None
            })?;
            let weight = required_u16(
                source,
                RulePackTable::Rules,
                &row_label,
                "weight",
                row.get(weight_index),
                &mut diagnostics,
            )?;
            let metric = required_string(
                source,
                RulePackTable::Rules,
                &row_label,
                "metric",
                row.get(metric_index),
                &mut diagnostics,
            )?;
            let operator = required_string(
                source,
                RulePackTable::Rules,
                &row_label,
                "op",
                row.get(operator_index),
                &mut diagnostics,
            )?;
            let threshold = threshold_index
                .and_then(|index| row.get(index))
                .map_or(Some(None), |value| {
                    optional_u64(source, &row_label, "threshold", value, &mut diagnostics)
                })?;
            Some(DxRuleDefinition {
                id,
                category,
                severity,
                weight,
                metric,
                operator,
                threshold,
                docs: docs_index
                    .and_then(|index| row.get(index))
                    .and_then(value_string)
                    .filter(|value| !value.trim().is_empty()),
                provenance: provenance_index
                    .and_then(|index| row.get(index))
                    .and_then(value_string)
                    .filter(|value| !value.trim().is_empty()),
            })
        })
        .collect();

    ParsedRulePackRules {
        categories,
        rules,
        diagnostics,
    }
}

fn parse_categories_from_document(
    document: &DxDocument,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Vec<DxRuleCategoryDefinition> {
    let Some(section) = document.section_by_name("categories") else {
        return Vec::new();
    };
    let Some(id_index) = required_column_index(
        section,
        source,
        RulePackTable::Categories,
        "id",
        diagnostics,
    ) else {
        return Vec::new();
    };
    let Some(label_index) = required_column_index(
        section,
        source,
        RulePackTable::Categories,
        "label",
        diagnostics,
    ) else {
        return Vec::new();
    };
    let Some(weight_index) = required_column_index(
        section,
        source,
        RulePackTable::Categories,
        "weight",
        diagnostics,
    ) else {
        return Vec::new();
    };

    let mut seen_category_ids = HashSet::new();
    section
        .rows
        .iter()
        .enumerate()
        .filter_map(|(row_index, row)| {
            let row_label = row_label(row.get(id_index), row_index, "category row");
            let id = required_string(
                source,
                RulePackTable::Categories,
                &row_label,
                "id",
                row.get(id_index),
                diagnostics,
            )?;
            if !seen_category_ids.insert(id.clone()) {
                diagnostics.push(duplicate_category_id(source, &row_label, &id));
                return None;
            }
            let label = required_string(
                source,
                RulePackTable::Categories,
                &row_label,
                "label",
                row.get(label_index),
                diagnostics,
            )?;
            let weight = required_u16(
                source,
                RulePackTable::Categories,
                &row_label,
                "weight",
                row.get(weight_index),
                diagnostics,
            )?;
            Some(DxRuleCategoryDefinition { id, label, weight })
        })
        .collect()
}

fn empty_rules(
    categories: Vec<DxRuleCategoryDefinition>,
    diagnostics: Vec<DxDiagnostic>,
) -> ParsedRulePackRules {
    ParsedRulePackRules {
        categories,
        rules: Vec::new(),
        diagnostics,
    }
}

fn row_label(value: Option<&DxLlmValue>, row_index: usize, fallback: &str) -> String {
    value
        .and_then(value_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{fallback} {}", row_index + 1))
}

fn parse_severity(value: &str) -> Option<DxSeverity> {
    match value {
        "info" => Some(DxSeverity::Info),
        "warning" => Some(DxSeverity::Warning),
        "failure" => Some(DxSeverity::Failure),
        _ => None,
    }
}
