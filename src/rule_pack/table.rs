use std::path::Path;

use serializer::{DxLlmValue, DxSection};

use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

#[derive(Debug, Clone, Copy)]
pub(super) enum RulePackTable {
    Rules,
    Categories,
}

impl RulePackTable {
    pub(super) fn table_label(self) -> &'static str {
        match self {
            Self::Rules => "rules table",
            Self::Categories => "categories table",
        }
    }

    fn diagnostic_id(self) -> &'static str {
        match self {
            Self::Rules => "rule-pack-rule-invalid",
            Self::Categories => "rule-pack-category-invalid",
        }
    }

    fn required_column_next_action(self) -> &'static str {
        match self {
            Self::Rules => "Add the required rule-pack column to the serializer .sr rules table.",
            Self::Categories => {
                "Add the required rule-pack column to the serializer .sr categories table."
            }
        }
    }

    fn required_cell_next_action(self) -> &'static str {
        match self {
            Self::Rules => "Fill every required cell in the serializer .sr rules table.",
            Self::Categories => "Fill every required cell in the serializer .sr categories table.",
        }
    }

    fn required_numeric_cell_next_action(self) -> &'static str {
        match self {
            Self::Rules => "Fill every required numeric cell in the serializer .sr rules table.",
            Self::Categories => {
                "Fill every required numeric cell in the serializer .sr categories table."
            }
        }
    }

    fn invalid_u16_next_action(self) -> &'static str {
        match self {
            Self::Rules => "Use a non-negative integer that fits the DX Check rule weight range.",
            Self::Categories => {
                "Use a non-negative integer that fits the DX Check category weight range in the serializer .sr categories table."
            }
        }
    }
}

pub(super) fn required_column_index(
    section: &DxSection,
    source: &Path,
    table: RulePackTable,
    column: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<usize> {
    let index = section.column_index(column);
    if index.is_none() {
        diagnostics.push(invalid_table_row(
            source,
            table,
            table.table_label(),
            format!("missing required column `{column}`"),
            table.required_column_next_action(),
        ));
    }
    index
}

pub(super) fn required_string(
    source: &Path,
    table: RulePackTable,
    row_label: &str,
    field: &str,
    value: Option<&DxLlmValue>,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<String> {
    let value = value.and_then(value_string);
    if value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        value
    } else {
        diagnostics.push(invalid_table_row(
            source,
            table,
            row_label,
            format!("missing required `{field}` value"),
            table.required_cell_next_action(),
        ));
        None
    }
}

pub(super) fn required_u16(
    source: &Path,
    table: RulePackTable,
    row_label: &str,
    field: &str,
    value: Option<&DxLlmValue>,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<u16> {
    let Some(value) = value else {
        diagnostics.push(invalid_table_row(
            source,
            table,
            row_label,
            format!("missing required `{field}` value"),
            table.required_numeric_cell_next_action(),
        ));
        return None;
    };
    value_u16(value).or_else(|| {
        diagnostics.push(invalid_table_row(
            source,
            table,
            row_label,
            format!("invalid {field} `{}`", value),
            table.invalid_u16_next_action(),
        ));
        None
    })
}

pub(super) fn optional_u64(
    source: &Path,
    row_label: &str,
    field: &str,
    value: &DxLlmValue,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<Option<u64>> {
    if value_string(value).is_some_and(|value| value.trim().is_empty()) {
        return Some(None);
    }
    value_u64(value).map(Some).or_else(|| {
        diagnostics.push(invalid_table_row(
            source,
            RulePackTable::Rules,
            row_label,
            format!("invalid {field} `{}`", value),
            "Use a non-negative integer for numeric rule thresholds.",
        ));
        None
    })
}

pub(super) fn invalid_table_row(
    source: &Path,
    table: RulePackTable,
    row_label: impl AsRef<str>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxDiagnostic {
    let row_label = row_label.as_ref();
    let message = match table {
        RulePackTable::Rules => {
            format!("Rule pack row `{row_label}` is invalid: {}", message.into())
        }
        RulePackTable::Categories if row_label == table.table_label() => {
            format!("Rule pack categories table is invalid: {}", message.into())
        }
        RulePackTable::Categories => {
            format!(
                "Rule pack category row `{row_label}` is invalid: {}",
                message.into()
            )
        }
    };

    DxDiagnostic {
        id: table.diagnostic_id().to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message,
        next_action: next_action.into(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub(super) fn duplicate_rule_id(source: &Path, row_label: &str, rule_id: &str) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-duplicate-rule-id".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule pack row `{row_label}` duplicates rule id `{rule_id}`; only the first rule id is loaded"
        ),
        next_action: "Give every serializer .sr rule row a unique id before scoring the pack."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub(super) fn duplicate_category_id(
    source: &Path,
    row_label: &str,
    category_id: &str,
) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-duplicate-category-id".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule pack category row `{row_label}` duplicates category id `{category_id}`; only the first category id is loaded"
        ),
        next_action: "Give every serializer .sr category row a unique id before scoring the pack."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub(super) fn value_string(value: &DxLlmValue) -> Option<String> {
    match value {
        DxLlmValue::Str(value) | DxLlmValue::Ref(value) => Some(value.clone()),
        DxLlmValue::Num(_) | DxLlmValue::Bool(_) => Some(value.to_string()),
        DxLlmValue::Null | DxLlmValue::Arr(_) | DxLlmValue::Obj(_) => None,
    }
}

fn value_u16(value: &DxLlmValue) -> Option<u16> {
    value_u64(value).and_then(|value| u16::try_from(value).ok())
}

fn value_u64(value: &DxLlmValue) -> Option<u64> {
    match value {
        DxLlmValue::Num(value) if value.is_finite() && *value >= 0.0 && value.fract() == 0.0 => {
            Some(*value as u64)
        }
        DxLlmValue::Str(value) => value.parse().ok(),
        _ => None,
    }
}
