use serde_json::Value;

use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

use super::common::{combined_lossy, invalid_runner_output, to_u32};

pub fn parse_cargo_json_lines(source: impl Into<String>, output: &str) -> Vec<DxDiagnostic> {
    let source = source.into();
    let mut diagnostics = Vec::new();
    let mut invalid_output_seen = false;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match serde_json::from_str::<Value>(line) {
            Ok(value) => match parse_cargo_json_value(&source, &value) {
                Ok(Some(diagnostic)) => diagnostics.push(diagnostic),
                Ok(None) => {}
                Err(reason) if !invalid_output_seen => {
                    invalid_output_seen = true;
                    diagnostics.push(invalid_runner_output(&source, reason, line));
                }
                Err(_) => {}
            },
            Err(_) if !invalid_output_seen => {
                invalid_output_seen = true;
                diagnostics.push(invalid_runner_output(
                    &source,
                    "cargo-json promised newline-delimited JSON but emitted an invalid line",
                    line,
                ));
            }
            Err(_) => {}
        }
    }
    diagnostics
}

pub(in crate::diagnostics) fn parse_rustfmt(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_rustfmt_diff_line(source, line.trim()))
        .collect()
}

fn parse_rustfmt_diff_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    let location = line.strip_prefix("Diff in ")?.trim().trim_end_matches(':');
    let (file, line_number) = location.rsplit_once(':')?;
    let line_number = line_number.parse::<u64>().ok().and_then(to_u32);
    Some(DxDiagnostic {
        id: format!("{source}:format"),
        source: source.to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.to_string()),
        line: line_number,
        column: None,
        message: format!("{file} is not rustfmt-formatted"),
        next_action: "Run rustfmt on the reported Rust file, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn parse_cargo_json_value(
    source: &str,
    value: &Value,
) -> Result<Option<DxDiagnostic>, &'static str> {
    if value.get("reason").and_then(Value::as_str) != Some("compiler-message") {
        return Ok(None);
    }

    let message = value
        .get("message")
        .ok_or("cargo-json compiler-message is missing message object")?;
    let message_text = message
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .ok_or("cargo-json compiler-message is missing message text")?
        .to_string();
    let level = message
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("warning");
    let code = message
        .get("code")
        .and_then(|code| code.get("code"))
        .and_then(Value::as_str)
        .unwrap_or(level);
    let primary_span = message
        .get("spans")
        .and_then(Value::as_array)
        .and_then(|spans| {
            spans
                .iter()
                .find(|span| {
                    span.get("is_primary")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .or_else(|| spans.first())
        });

    Ok(Some(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity: cargo_level_to_severity(level),
        file: primary_span
            .and_then(|span| span.get("file_name"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        line: primary_span
            .and_then(|span| span.get("line_start"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        column: primary_span
            .and_then(|span| span.get("column_start"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        message: message_text,
        next_action: "Fix the Rust diagnostic reported by Cargo, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    }))
}

fn cargo_level_to_severity(level: &str) -> DxSeverity {
    match level {
        "error" | "failure-note" => DxSeverity::Failure,
        "warning" => DxSeverity::Warning,
        _ => DxSeverity::Info,
    }
}
