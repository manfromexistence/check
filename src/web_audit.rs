use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serializer::{DxDocument, DxLlmValue, DxSection};

use crate::model::{
    DxDiagnostic, DxMeasurementKind, DxSeverity, DxWebAuditResult, DxWebAuditTarget,
    DxWebLighthouseMode,
};

mod runtime;
pub use runtime::WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE;
use runtime::load_project_lighthouse_runtime;
pub use runtime::{DxWebLighthouseRuntime, WEB_LIGHTHOUSE_RUNTIMES_SOURCE};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DxWebAuditProject {
    pub targets: Vec<DxWebAuditTarget>,
    pub results: Vec<DxWebAuditResult>,
    pub diagnostics: Vec<DxDiagnostic>,
    pub lighthouse_runtime: Option<DxWebLighthouseRuntime>,
}

pub fn load_project_web_audit(root: &Path) -> DxWebAuditProject {
    let source = root.join("dx");
    if !source.is_file() {
        let mut project = DxWebAuditProject::default();
        project.lighthouse_runtime =
            load_project_lighthouse_runtime(root, &mut project.diagnostics);
        return project;
    }

    let body = match fs::read_to_string(&source) {
        Ok(body) => body,
        Err(error) => {
            let mut project = DxWebAuditProject {
                diagnostics: vec![config_diagnostic(
                    "web-audit-config-read-failed",
                    &source,
                    format!("Web audit config could not be read: {error}"),
                    "Fix the extensionless dx project config so web audit targets can be loaded.",
                )],
                ..DxWebAuditProject::default()
            };
            project.lighthouse_runtime =
                load_project_lighthouse_runtime(root, &mut project.diagnostics);
            return project;
        }
    };
    let document = match serializer::llm_to_document(&body) {
        Ok(document) => document,
        Err(error) => {
            let mut project = DxWebAuditProject {
                diagnostics: vec![config_diagnostic(
                    "web-audit-config-parse-failed",
                    &source,
                    format!("Web audit config could not be parsed: {error}"),
                    "Fix the extensionless dx serializer source before loading web audit targets.",
                )],
                ..DxWebAuditProject::default()
            };
            project.lighthouse_runtime =
                load_project_lighthouse_runtime(root, &mut project.diagnostics);
            return project;
        }
    };

    let mut diagnostics = Vec::new();
    let targets = parse_targets(&document, &source, &mut diagnostics);
    let results = parse_results(&document, &source, &targets, &mut diagnostics);
    let lighthouse_runtime = load_project_lighthouse_runtime(root, &mut diagnostics);

    DxWebAuditProject {
        targets,
        results,
        diagnostics,
        lighthouse_runtime,
    }
}

pub fn metric_value(result: &DxWebAuditResult, metric: &str) -> Option<u64> {
    match metric {
        "web_http_status" => result.status.map(u64::from),
        "web_html_bytes" => result.html_bytes,
        "web_title_present" => Some(presence_value(result.title_present)),
        "web_description_present" => Some(presence_value(result.description_present)),
        "web_canonical_present" => Some(presence_value(result.canonical_present)),
        "web_viewport_present" => Some(presence_value(result.viewport_present)),
        "web_security_header_count" => Some(u64::from(result.security_header_count)),
        _ => None,
    }
}

fn presence_value(value: bool) -> u64 {
    if value { 1 } else { 0 }
}

fn parse_targets(
    document: &DxDocument,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Vec<DxWebAuditTarget> {
    let Some(section) = document.section_by_name("web_audit_targets") else {
        return Vec::new();
    };
    let Some(id_index) = column(section, "id", source, diagnostics) else {
        return Vec::new();
    };
    let Some(url_index) = column(section, "url", source, diagnostics) else {
        return Vec::new();
    };
    let required_status_index = section.column_index("required_status");
    let max_html_bytes_index = section.column_index("max_html_bytes");
    let lighthouse_index = section
        .column_index("lighthouse")
        .or_else(|| section.column_index("lighthouse_mode"));
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    for (row_index, row) in section.rows.iter().enumerate() {
        let row_label = row_label(row_index);
        let Some(id) = string_cell(row.get(id_index)) else {
            diagnostics.push(row_diagnostic(
                "web-audit-target-missing-id",
                source,
                &row_label,
                "Web audit target is missing an id",
                "Give every web_audit_targets row a stable id.",
            ));
            continue;
        };
        let duplicate = !seen.insert(id.clone());
        if duplicate {
            diagnostics.push(row_diagnostic(
                "web-audit-target-duplicate-id",
                source,
                &row_label,
                format!("Web audit target id `{id}` is duplicated"),
                "Give every web audit target a unique id before generating adapter plans.",
            ));
        }
        let valid_id = valid_identifier(&id);
        if !valid_id {
            diagnostics.push(row_diagnostic(
                "web-audit-target-invalid-id",
                source,
                &row_label,
                format!("Web audit target id `{id}` is not a safe identifier"),
                "Use lowercase letters, digits, hyphen, underscore, or dot for web audit target ids.",
            ));
        }
        let Some(url) = string_cell(row.get(url_index)) else {
            diagnostics.push(row_diagnostic(
                "web-audit-target-missing-url",
                source,
                &row_label,
                format!("Web audit target `{id}` is missing a URL"),
                "Provide an absolute http:// or https:// URL for every web audit target.",
            ));
            continue;
        };
        let valid_url = supported_url(&url);
        if !valid_url {
            diagnostics.push(row_diagnostic(
                "web-audit-target-unsupported-url",
                source,
                &row_label,
                format!("Web audit target `{id}` uses unsupported URL `{url}`"),
                "Use only absolute http:// or https:// URLs for web audit targets.",
            ));
        }
        let required_status = parse_optional_status(
            row.get_index(required_status_index),
            source,
            &row_label,
            "web-audit-target-invalid-status",
            diagnostics,
        );
        let max_html_bytes = parse_optional_byte_limit(
            row.get_index(max_html_bytes_index),
            source,
            &row_label,
            diagnostics,
        );
        let lighthouse_mode = parse_optional_lighthouse_mode(
            row.get_index(lighthouse_index),
            source,
            &row_label,
            diagnostics,
        );

        if duplicate
            || !valid_id
            || !valid_url
            || required_status.is_invalid()
            || max_html_bytes.is_invalid()
            || lighthouse_mode.is_invalid()
        {
            continue;
        }

        targets.push(DxWebAuditTarget {
            id,
            url,
            required_status: required_status.into_value(),
            max_html_bytes: max_html_bytes.into_value(),
            lighthouse_mode: lighthouse_mode.into_value(),
        });
    }

    targets
}

fn parse_results(
    document: &DxDocument,
    source: &Path,
    targets: &[DxWebAuditTarget],
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Vec<DxWebAuditResult> {
    let Some(section) = document.section_by_name("web_audit_results") else {
        return Vec::new();
    };
    let Some(id_index) = column(section, "id", source, diagnostics) else {
        return Vec::new();
    };
    let Some(target_id_index) = column(section, "target_id", source, diagnostics) else {
        return Vec::new();
    };
    let Some(url_index) = column(section, "url", source, diagnostics) else {
        return Vec::new();
    };
    let status_index = section.column_index("status");
    let html_bytes_index = section.column_index("html_bytes");
    let title_index = section.column_index("title_present");
    let description_index = section.column_index("description_present");
    let canonical_index = section.column_index("canonical_present");
    let viewport_index = section.column_index("viewport_present");
    let security_header_count_index = section.column_index("security_header_count");
    let final_url_index = section.column_index("final_url");
    let response_time_index = section.column_index("response_time_ms");
    let source_index = section.column_index("source");
    let target_ids = targets
        .iter()
        .map(|target| target.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut results = Vec::new();

    for (row_index, row) in section.rows.iter().enumerate() {
        let row_label = row_label(row_index);
        let Some(id) = string_cell(row.get(id_index)) else {
            diagnostics.push(row_diagnostic(
                "web-audit-result-missing-id",
                source,
                &row_label,
                "Web audit result is missing an id",
                "Give every web_audit_results row a stable id.",
            ));
            continue;
        };
        if !seen.insert(id.clone()) {
            diagnostics.push(row_diagnostic(
                "web-audit-result-duplicate-id",
                source,
                &row_label,
                format!("Web audit result id `{id}` is duplicated"),
                "Give every web audit result a unique id before scoring.",
            ));
            continue;
        }
        let Some(target_id) = string_cell(row.get(target_id_index)) else {
            diagnostics.push(row_diagnostic(
                "web-audit-result-missing-target",
                source,
                &row_label,
                format!("Web audit result `{id}` is missing target_id"),
                "Point each web audit result at a configured web audit target.",
            ));
            continue;
        };
        if !target_ids.is_empty() && !target_ids.contains(target_id.as_str()) {
            diagnostics.push(row_diagnostic(
                "web-audit-result-unknown-target",
                source,
                &row_label,
                format!("Web audit result `{id}` references unknown target `{target_id}`"),
                "Use a target_id from web_audit_targets before scoring the web audit result.",
            ));
            continue;
        }
        let Some(url) = string_cell(row.get(url_index)).filter(|url| supported_url(url)) else {
            diagnostics.push(row_diagnostic(
                "web-audit-result-invalid-url",
                source,
                &row_label,
                format!("Web audit result `{id}` is missing a supported URL"),
                "Use only absolute http:// or https:// URLs in web audit results.",
            ));
            continue;
        };
        let status = parse_optional_status(
            row.get_index(status_index),
            source,
            &row_label,
            "web-audit-result-invalid-status",
            diagnostics,
        );
        let html_bytes = parse_optional_u64(
            row.get_index(html_bytes_index),
            source,
            &row_label,
            "web-audit-result-invalid-html-bytes",
            "html_bytes",
            diagnostics,
        );
        let response_time_ms = parse_optional_u128(
            row.get_index(response_time_index),
            source,
            &row_label,
            "web-audit-result-invalid-response-time",
            "response_time_ms",
            diagnostics,
        );
        if status.is_invalid() || html_bytes.is_invalid() || response_time_ms.is_invalid() {
            continue;
        }

        results.push(DxWebAuditResult {
            id,
            target_id,
            url,
            status: status.into_value(),
            final_url: string_cell(row.get_index(final_url_index)),
            response_time_ms: response_time_ms.into_value(),
            html_bytes: html_bytes.into_value(),
            title_present: bool_cell(row.get_index(title_index)).unwrap_or(false),
            description_present: bool_cell(row.get_index(description_index)).unwrap_or(false),
            canonical_present: bool_cell(row.get_index(canonical_index)).unwrap_or(false),
            viewport_present: bool_cell(row.get_index(viewport_index)).unwrap_or(false),
            security_header_count: parse_optional_u16(row.get_index(security_header_count_index))
                .unwrap_or(0),
            source: string_cell(row.get_index(source_index)).map(PathBuf::from),
        });
    }

    results
}

trait OptionalRowCell {
    fn get_index(&self, index: Option<usize>) -> Option<&DxLlmValue>;
}

impl OptionalRowCell for [DxLlmValue] {
    fn get_index(&self, index: Option<usize>) -> Option<&DxLlmValue> {
        index.and_then(|index| self.get(index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedOptional<T> {
    Missing,
    Value(T),
    Invalid,
}

impl<T> ParsedOptional<T> {
    fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid)
    }

    fn into_value(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Missing | Self::Invalid => None,
        }
    }
}

fn parse_optional_status(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    diagnostic_id: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> ParsedOptional<u16> {
    let Some(value) = value else {
        return ParsedOptional::Missing;
    };
    if string_cell(Some(value)).is_some_and(|value| value.trim().is_empty()) {
        return ParsedOptional::Missing;
    }
    let Some(status) = u64_cell(value).and_then(|value| u16::try_from(value).ok()) else {
        diagnostics.push(row_diagnostic(
            diagnostic_id,
            source,
            row_label,
            "Web audit status must be an integer",
            "Use an HTTP status code between 100 and 599.",
        ));
        return ParsedOptional::Invalid;
    };
    if !(100..=599).contains(&status) {
        diagnostics.push(row_diagnostic(
            diagnostic_id,
            source,
            row_label,
            format!("Web audit status `{status}` is outside the HTTP status range"),
            "Use an HTTP status code between 100 and 599.",
        ));
        return ParsedOptional::Invalid;
    }
    ParsedOptional::Value(status)
}

fn parse_optional_lighthouse_mode(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> ParsedOptional<DxWebLighthouseMode> {
    let Some(value) = value else {
        return ParsedOptional::Missing;
    };
    let Some(value) = string_cell(Some(value)) else {
        diagnostics.push(row_diagnostic(
            "web-audit-target-invalid-lighthouse-mode",
            source,
            row_label,
            "Web audit target lighthouse mode must be native, official, or auto",
            "Use native for deterministic HTTP checks, official for Google Lighthouse, or auto for official with native fallback.",
        ));
        return ParsedOptional::Invalid;
    };
    if value.trim().is_empty() {
        return ParsedOptional::Missing;
    }
    let Some(mode) = DxWebLighthouseMode::parse(&value) else {
        diagnostics.push(row_diagnostic(
            "web-audit-target-invalid-lighthouse-mode",
            source,
            row_label,
            format!("Web audit target lighthouse mode `{value}` is not supported"),
            "Use native, official, or auto for the lighthouse column.",
        ));
        return ParsedOptional::Invalid;
    };
    ParsedOptional::Value(mode)
}

fn parse_optional_byte_limit(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> ParsedOptional<u64> {
    let parsed = parse_optional_u64(
        value,
        source,
        row_label,
        "web-audit-target-invalid-byte-limit",
        "max_html_bytes",
        diagnostics,
    );
    if matches!(parsed, ParsedOptional::Value(0)) {
        diagnostics.push(row_diagnostic(
            "web-audit-target-invalid-byte-limit",
            source,
            row_label,
            "Web audit max_html_bytes must be greater than zero",
            "Use a positive byte budget for each web audit target.",
        ));
        return ParsedOptional::Invalid;
    }
    parsed
}

fn parse_optional_u64(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    diagnostic_id: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> ParsedOptional<u64> {
    let Some(value) = value else {
        return ParsedOptional::Missing;
    };
    if string_cell(Some(value)).is_some_and(|value| value.trim().is_empty()) {
        return ParsedOptional::Missing;
    }
    match u64_cell(value) {
        Some(value) => ParsedOptional::Value(value),
        None => {
            diagnostics.push(row_diagnostic(
                diagnostic_id,
                source,
                row_label,
                format!("Web audit field `{field}` must be a non-negative integer"),
                "Use numeric values in web audit serializer tables.",
            ));
            ParsedOptional::Invalid
        }
    }
}

fn parse_optional_u128(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    diagnostic_id: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> ParsedOptional<u128> {
    match parse_optional_u64(value, source, row_label, diagnostic_id, field, diagnostics) {
        ParsedOptional::Missing => ParsedOptional::Missing,
        ParsedOptional::Invalid => ParsedOptional::Invalid,
        ParsedOptional::Value(value) => ParsedOptional::Value(u128::from(value)),
    }
}

fn parse_optional_u16(value: Option<&DxLlmValue>) -> Option<u16> {
    value
        .and_then(u64_cell)
        .and_then(|value| u16::try_from(value).ok())
}

fn column(
    section: &DxSection,
    column: &str,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<usize> {
    let index = section.column_index(column);
    if index.is_none() {
        diagnostics.push(config_diagnostic(
            "web-audit-table-invalid",
            source,
            format!("Web audit table is missing required column `{column}`"),
            "Add the required web audit serializer table column before running dx check.",
        ));
    }
    index
}

fn row_diagnostic(
    id: &str,
    source: &Path,
    row_label: &str,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxDiagnostic {
    config_diagnostic(
        id,
        source,
        format!("Web audit row `{row_label}` is invalid: {}", message.into()),
        next_action,
    )
}

fn config_diagnostic(
    id: &str,
    source: &Path,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxDiagnostic {
    DxDiagnostic {
        id: id.to_string(),
        source: "dx-check-web-audit".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: message.into(),
        next_action: next_action.into(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn row_label(index: usize) -> String {
    format!("row {}", index + 1)
}

fn string_cell(value: Option<&DxLlmValue>) -> Option<String> {
    let value = value?;
    match value {
        DxLlmValue::Str(value) | DxLlmValue::Ref(value) => Some(value.clone()),
        DxLlmValue::Num(_) | DxLlmValue::Bool(_) => Some(value.to_string()),
        DxLlmValue::Null | DxLlmValue::Arr(_) | DxLlmValue::Obj(_) => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn bool_cell(value: Option<&DxLlmValue>) -> Option<bool> {
    match value? {
        DxLlmValue::Bool(value) => Some(*value),
        DxLlmValue::Num(value) if *value == 0.0 => Some(false),
        DxLlmValue::Num(value) if *value == 1.0 => Some(true),
        DxLlmValue::Str(value) => match value.trim() {
            "1" | "true" | "yes" | "present" => Some(true),
            "0" | "false" | "no" | "absent" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn u64_cell(value: &DxLlmValue) -> Option<u64> {
    match value {
        DxLlmValue::Num(value) if value.is_finite() && *value >= 0.0 && value.fract() == 0.0 => {
            Some(*value as u64)
        }
        DxLlmValue::Str(value) => value.parse().ok(),
        _ => None,
    }
}

fn valid_identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn supported_url(url: &str) -> bool {
    let trimmed = url.trim();
    let scheme_ok = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    scheme_ok && trimmed.len() <= 2048 && !trimmed.chars().any(char::is_control)
}
