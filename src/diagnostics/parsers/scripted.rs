use serde_json::Value;

use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

use super::common::{combined_lossy, invalid_runner_output, to_u32};

pub(in crate::diagnostics) fn parse_unknown_parser(
    source: &str,
    parser: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    let output = combined_lossy(stdout, stderr);
    if output.trim().is_empty() {
        return Vec::new();
    }
    vec![invalid_runner_output(
        source,
        &format!("unknown parser `{parser}` cannot safely interpret this adapter output"),
        &output,
    )]
}

pub(in crate::diagnostics) fn parse_package_script(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_typescript_diagnostic_line(source, line.trim()))
        .collect()
}

pub(in crate::diagnostics) fn parse_pytest(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_pytest_line(source, line.trim()))
        .collect()
}

pub(in crate::diagnostics) fn parse_ruff_format(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("Would reformat:")
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(|file| DxDiagnostic {
                    id: format!("{source}:format"),
                    source: source.to_string(),
                    severity: DxSeverity::Failure,
                    file: Some(file.to_string()),
                    line: None,
                    column: None,
                    message: format!("{file} would be reformatted by Ruff"),
                    next_action: "Run the approved Ruff formatter, then rerun dx check."
                        .to_string(),
                    measurement: DxMeasurementKind::Measured,
                })
        })
        .collect()
}

pub(in crate::diagnostics) fn parse_black(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("would reformat ")
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(|file| DxDiagnostic {
                    id: format!("{source}:format"),
                    source: source.to_string(),
                    severity: DxSeverity::Failure,
                    file: Some(file.to_string()),
                    line: None,
                    column: None,
                    message: format!("{file} would be reformatted by Black"),
                    next_action: "Run the approved Black formatter, then rerun dx check."
                        .to_string(),
                    measurement: DxMeasurementKind::Measured,
                })
        })
        .collect()
}

pub(in crate::diagnostics) fn parse_gofmt_list(source: &str, stdout: &[u8]) -> Vec<DxDiagnostic> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|file| DxDiagnostic {
            id: format!("{source}:format"),
            source: source.to_string(),
            severity: DxSeverity::Failure,
            file: Some(file.to_string()),
            line: None,
            column: None,
            message: format!("{file} is not gofmt-formatted"),
            next_action: "Run gofmt on the reported file, then rerun dx check.".to_string(),
            measurement: DxMeasurementKind::Measured,
        })
        .collect()
}

pub(in crate::diagnostics) fn parse_go_locations(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_go_location_line(source, line.trim()))
        .collect()
}

pub(in crate::diagnostics) fn parse_ruff_json(source: &str, stdout: &[u8]) -> Vec<DxDiagnostic> {
    if stdout.iter().all(u8::is_ascii_whitespace) {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(stdout) else {
        return vec![invalid_runner_output(
            source,
            "ruff-json promised machine-readable JSON but emitted invalid output",
            &String::from_utf8_lossy(stdout),
        )];
    };
    let Some(items) = value.as_array() else {
        return vec![invalid_runner_output(
            source,
            "ruff-json promised machine-readable JSON; expected JSON array",
            &String::from_utf8_lossy(stdout),
        )];
    };

    let mut diagnostics = Vec::new();
    for item in items {
        match ruff_diagnostic(source, item) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(reason) => {
                return vec![invalid_runner_output(
                    source,
                    reason,
                    &String::from_utf8_lossy(stdout),
                )];
            }
        }
    }
    diagnostics
}

pub(in crate::diagnostics) fn parse_biome_json(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    let output = combined_lossy(stdout, stderr);
    if output.trim().is_empty() {
        return Vec::new();
    }

    let value = match extract_biome_report(&output) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return vec![invalid_runner_output(
                source,
                "biome-json promised machine-readable JSON but emitted invalid output",
                &output,
            )];
        }
        Err(reason) => return vec![invalid_runner_output(source, reason, &output)],
    };
    let Some(items) = value.get("diagnostics").and_then(Value::as_array) else {
        return vec![invalid_runner_output(
            source,
            "biome-json promised machine-readable JSON; expected diagnostics array",
            &output,
        )];
    };
    let command = value
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("check");

    let mut diagnostics = Vec::new();
    for item in items {
        match biome_diagnostic(source, command, item) {
            Ok(diagnostic) => diagnostics.push(diagnostic),
            Err(reason) => return vec![invalid_runner_output(source, reason, &output)],
        }
    }
    diagnostics
}

fn extract_biome_report(output: &str) -> Result<Option<Value>, &'static str> {
    for (index, _) in output
        .char_indices()
        .filter(|(_, character)| *character == '{')
    {
        let mut stream = serde_json::Deserializer::from_str(&output[index..]).into_iter::<Value>();
        let Some(Ok(value)) = stream.next() else {
            continue;
        };
        if value.get("diagnostics").is_some() {
            if value.get("diagnostics").and_then(Value::as_array).is_some() {
                return Ok(Some(value));
            }
            return Err("biome-json promised machine-readable JSON; expected diagnostics array");
        }
    }

    Ok(None)
}

fn ruff_diagnostic(source: &str, item: &Value) -> Result<DxDiagnostic, &'static str> {
    let code = item.get("code").and_then(Value::as_str).unwrap_or("ruff");
    let message = item
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .ok_or("ruff-json diagnostic item is missing required message")?
        .to_string();
    let location = item.get("location");
    Ok(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity: DxSeverity::Failure,
        file: item
            .get("filename")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        line: location
            .and_then(|location| location.get("row"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        column: location
            .and_then(|location| location.get("column"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        message,
        next_action: "Fix the Python diagnostic reported by Ruff, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn biome_diagnostic(
    source: &str,
    command: &str,
    item: &Value,
) -> Result<DxDiagnostic, &'static str> {
    let message = item
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .ok_or("biome-json diagnostic item is missing required message")?
        .to_string();
    let category = item
        .get("category")
        .and_then(Value::as_str)
        .filter(|category| !category.trim().is_empty())
        .unwrap_or("biome");
    let severity = item
        .get("severity")
        .and_then(Value::as_str)
        .map(biome_severity)
        .unwrap_or(DxSeverity::Failure);
    let location = item.get("location");
    let start = location.and_then(|location| location.get("start"));

    Ok(DxDiagnostic {
        id: format!("{source}:{category}"),
        source: source.to_string(),
        severity,
        file: location
            .and_then(|location| location.get("path"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        line: start
            .and_then(|start| start.get("line"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        column: start
            .and_then(|start| start.get("column"))
            .and_then(Value::as_u64)
            .and_then(to_u32),
        message,
        next_action: biome_next_action(command, category),
        measurement: DxMeasurementKind::Measured,
    })
}

fn biome_severity(value: &str) -> DxSeverity {
    match value {
        "hint" | "info" | "information" => DxSeverity::Info,
        "warn" | "warning" => DxSeverity::Warning,
        _ => DxSeverity::Failure,
    }
}

fn biome_next_action(command: &str, category: &str) -> String {
    if command == "format" || category == "format" {
        "Run the approved Biome formatter, then rerun dx check.".to_string()
    } else {
        "Fix the Biome diagnostic, then rerun dx check.".to_string()
    }
}

fn parse_typescript_diagnostic_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    let (file, rest) = line.split_once('(')?;
    let (location, rest) = rest.split_once("):")?;
    let (line_number, column) = location.split_once(',')?;
    let line_number = line_number.parse::<u64>().ok().and_then(to_u32)?;
    let column = column.parse::<u64>().ok().and_then(to_u32)?;
    let rest = rest.trim();
    let (level, rest) = rest.split_once(' ')?;
    if !matches!(level, "error" | "warning") {
        return None;
    }
    let (code, message) = rest.split_once(':')?;
    let code = code.trim();
    let message = message.trim();
    if code.is_empty() || message.is_empty() {
        return None;
    }

    Some(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity: if level == "error" {
            DxSeverity::Failure
        } else {
            DxSeverity::Warning
        },
        file: Some(file.to_string()),
        line: Some(line_number),
        column: Some(column),
        message: message.to_string(),
        next_action:
            "Fix the package script diagnostic reported by TypeScript, then rerun dx check."
                .to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn parse_pytest_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    if !line.contains(" FAILED") {
        return None;
    }
    let (file, rest) = line.split_once("::")?;
    if !(file.ends_with(".py") || file.contains(".py::")) {
        return None;
    }
    let test_name = rest.split_whitespace().next().unwrap_or("").trim();
    if test_name.is_empty() {
        return None;
    }

    Some(DxDiagnostic {
        id: format!("{source}:test-failed"),
        source: source.to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.to_string()),
        line: None,
        column: None,
        message: format!("pytest test `{test_name}` failed"),
        next_action: "Fix the failing pytest test, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn parse_go_location_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    if line.is_empty()
        || line.starts_with('#')
        || line == "FAIL"
        || line == "PASS"
        || line.starts_with("ok ")
        || line.starts_with("FAIL ")
    {
        return None;
    }

    let line = line.trim_start_matches(|character: char| character.is_whitespace());
    let (file, rest) = line.split_once(':')?;
    if !file.ends_with(".go") {
        return None;
    }
    let (line_number, rest) = rest.split_once(':')?;
    let line_number = line_number.parse::<u64>().ok().and_then(to_u32)?;
    let (column, message) = match rest.split_once(':') {
        Some((maybe_column, message)) => match maybe_column.parse::<u64>().ok().and_then(to_u32) {
            Some(column) => (Some(column), message.trim()),
            None => (None, rest.trim()),
        },
        None => (None, rest.trim()),
    };
    if message.is_empty() {
        return None;
    }

    Some(DxDiagnostic {
        id: format!("{source}:go-location"),
        source: source.to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.to_string()),
        line: Some(line_number),
        column,
        message: message.to_string(),
        next_action: "Fix the Go diagnostic reported by the adapter, then rerun dx check."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}
