use std::collections::BTreeSet;

use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

pub(super) fn parse_clang_format(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_clang_gcc_line(source, line.trim(), Some(DxSeverity::Failure)))
        .map(|mut diagnostic| {
            diagnostic.id = format!(
                "{source}:{}",
                diagnostic_code(&diagnostic.message, "format")
            );
            diagnostic.next_action =
                "Run clang-format on the reported C/C++ file, then rerun dx check.".to_string();
            diagnostic
        })
        .collect()
}

pub(super) fn parse_clang_tidy(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    parse_cxx_compiler(source, stdout, stderr)
        .into_iter()
        .map(|mut diagnostic| {
            diagnostic.next_action =
                "Fix the clang-tidy diagnostic, then rerun dx check.".to_string();
            diagnostic
        })
        .collect()
}

pub(super) fn parse_clangd(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| parse_clangd_line(source, line.trim()))
        .collect()
}

pub(super) fn parse_cxx_compiler(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            parse_msvc_line(source, line).or_else(|| parse_clang_gcc_line(source, line, None))
        })
        .collect()
}

fn parse_clangd_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    let severity_marker = line.chars().next()?;
    let severity = match severity_marker {
        'E' | 'F' => DxSeverity::Failure,
        'W' => DxSeverity::Warning,
        _ => return None,
    };
    let (_, rest) = line.split_once(']')?;
    let rest = rest.trim();
    let (code, message) = match rest.strip_prefix('[') {
        Some(rest) => {
            let (code, message) = rest.split_once(']')?;
            (code.trim(), message.trim())
        }
        None => (level_code_for_clangd(severity), rest),
    };
    let (location, message) = clangd_location_and_message(message);

    Some(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity,
        file: location.as_ref().map(|location| location.file.clone()),
        line: location.as_ref().map(|location| location.line),
        column: location.as_ref().and_then(|location| location.column),
        message: message.to_string(),
        next_action: "Fix the clangd check diagnostic, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn clangd_location_and_message(message: &str) -> (Option<Location>, &str) {
    let Some((prefix, rest)) = message.split_once(": ") else {
        return (None, message);
    };
    match parse_colon_location(prefix) {
        Some(location) => (Some(location), rest.trim()),
        None => (None, message),
    }
}

fn level_code_for_clangd(severity: DxSeverity) -> &'static str {
    match severity {
        DxSeverity::Failure => "error",
        DxSeverity::Warning => "warning",
        DxSeverity::Info => "info",
    }
}

pub(super) fn parse_cppcheck_xml(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    let output = combined_lossy(stdout, stderr);
    let mut diagnostics = Vec::new();
    let mut current = None::<CppcheckError>;

    for line in output.lines().map(str::trim) {
        if line.starts_with("<error ") {
            current = Some(CppcheckError {
                id: xml_attr(line, "id").unwrap_or_else(|| "cppcheck".to_string()),
                severity: xml_attr(line, "severity").unwrap_or_else(|| "warning".to_string()),
                message: xml_attr(line, "msg").unwrap_or_else(|| "cppcheck diagnostic".to_string()),
            });
            if line.contains("<location ")
                && let Some(error) = current.as_ref()
            {
                diagnostics.push(cppcheck_diagnostic(source, error, line));
            }
            if line.ends_with("/>") || line.contains("</error>") {
                current = None;
            }
            continue;
        }

        if line.starts_with("<location ") {
            if let Some(error) = current.as_ref() {
                diagnostics.push(cppcheck_diagnostic(source, error, line));
            }
            current = None;
            continue;
        }

        if line.starts_with("</error")
            && let Some(error) = current.take()
        {
            diagnostics.push(cppcheck_diagnostic(source, &error, ""));
        }
    }

    diagnostics
}

pub(super) fn parse_ctest(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    let mut seen = BTreeSet::new();
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(ctest_failed_test_name)
        .filter(|name| seen.insert(name.clone()))
        .map(|name| DxDiagnostic {
            id: format!("{source}:test-failed"),
            source: source.to_string(),
            severity: DxSeverity::Failure,
            file: None,
            line: None,
            column: None,
            message: format!("CTest test `{name}` failed"),
            next_action: "Fix the failing C/C++ test, then rerun dx check.".to_string(),
            measurement: DxMeasurementKind::Measured,
        })
        .collect()
}

fn parse_clang_gcc_line(
    source: &str,
    line: &str,
    forced_severity: Option<DxSeverity>,
) -> Option<DxDiagnostic> {
    let (prefix, level, message) = clang_gcc_parts(line)?;
    let location = parse_colon_location(prefix)?;
    let severity = forced_severity.unwrap_or(match level {
        "warning" => DxSeverity::Warning,
        "note" => DxSeverity::Info,
        _ => DxSeverity::Failure,
    });
    let code = diagnostic_code(message, level_code(level));

    Some(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity,
        file: Some(location.file),
        line: Some(location.line),
        column: location.column,
        message: message.to_string(),
        next_action: "Fix the C/C++ compiler diagnostic, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn clang_gcc_parts(line: &str) -> Option<(&str, &str, &str)> {
    [
        (": fatal error:", "fatal error"),
        (": error:", "error"),
        (": warning:", "warning"),
        (": note:", "note"),
    ]
    .iter()
    .find_map(|(marker, level)| {
        let (prefix, message) = line.split_once(marker)?;
        Some((prefix.trim(), *level, message.trim()))
    })
}

fn parse_colon_location(prefix: &str) -> Option<Location> {
    let (file_and_line, maybe_column) = prefix.rsplit_once(':')?;
    let (file, line) = match file_and_line.rsplit_once(':') {
        Some((file, line)) if line.parse::<u64>().is_ok() => (file, line),
        _ => (file_and_line, maybe_column),
    };
    let line = line.parse::<u64>().ok().and_then(to_u32)?;
    let column = maybe_column.parse::<u64>().ok().and_then(to_u32);
    let file = file.trim();
    if file.is_empty() {
        return None;
    }
    Some(Location {
        file: file.to_string(),
        line,
        column,
    })
}

fn parse_msvc_line(source: &str, line: &str) -> Option<DxDiagnostic> {
    let (prefix, rest) = line.split_once("):")?;
    let (file, location) = prefix.rsplit_once('(')?;
    let file = file.trim();
    if file.is_empty() {
        return None;
    }
    let (line_number, column) = match location.split_once(',') {
        Some((line_number, column)) => (
            line_number.parse::<u64>().ok().and_then(to_u32)?,
            column.parse::<u64>().ok().and_then(to_u32),
        ),
        None => (location.parse::<u64>().ok().and_then(to_u32)?, None),
    };

    let rest = rest.trim();
    let (severity, code, message) = msvc_message_parts(rest)?;
    Some(DxDiagnostic {
        id: format!("{source}:{code}"),
        source: source.to_string(),
        severity,
        file: Some(file.to_string()),
        line: Some(line_number),
        column,
        message: message.to_string(),
        next_action: "Fix the C/C++ compiler diagnostic, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn msvc_message_parts(rest: &str) -> Option<(DxSeverity, &str, &str)> {
    let (severity, rest) = if let Some(rest) = rest.strip_prefix("warning ") {
        (DxSeverity::Warning, rest)
    } else if let Some(rest) = rest.strip_prefix("error ") {
        (DxSeverity::Failure, rest)
    } else if let Some(rest) = rest.strip_prefix("fatal error ") {
        (DxSeverity::Failure, rest)
    } else {
        return None;
    };
    let (code, message) = rest.split_once(':')?;
    Some((severity, code.trim(), message.trim()))
}

#[derive(Debug, Clone)]
struct Location {
    file: String,
    line: u32,
    column: Option<u32>,
}

#[derive(Debug, Clone)]
struct CppcheckError {
    id: String,
    severity: String,
    message: String,
}

fn cppcheck_diagnostic(source: &str, error: &CppcheckError, location_line: &str) -> DxDiagnostic {
    DxDiagnostic {
        id: format!("{source}:{}", error.id),
        source: source.to_string(),
        severity: cppcheck_severity(&error.severity),
        file: xml_attr(location_line, "file"),
        line: xml_attr(location_line, "line")
            .and_then(|line| line.parse::<u64>().ok())
            .and_then(to_u32),
        column: xml_attr(location_line, "column")
            .and_then(|column| column.parse::<u64>().ok())
            .and_then(to_u32),
        message: error.message.clone(),
        next_action: "Fix the cppcheck diagnostic, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn cppcheck_severity(severity: &str) -> DxSeverity {
    match severity {
        "error" => DxSeverity::Failure,
        "information" => DxSeverity::Info,
        _ => DxSeverity::Warning,
    }
}

fn xml_attr(line: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(decode_xml_attr(&rest[..end]))
}

fn decode_xml_attr(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn ctest_failed_test_name(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some((_, rest)) = line.split_once(" - ") {
        let (name, _) = rest.split_once(" (")?;
        let name = name.trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    if !(line.contains("***Failed") || line.contains("***Timeout")) {
        return None;
    }
    let (_, rest) = line.split_once(':')?;
    let name = rest.split_whitespace().next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn diagnostic_code(message: &str, fallback: &str) -> String {
    let trimmed = message.trim();
    if let Some(start) = trimmed.rfind('[')
        && trimmed.ends_with(']')
        && start + 1 < trimmed.len() - 1
    {
        return trimmed[start + 1..trimmed.len() - 1].to_string();
    }
    fallback.to_string()
}

fn level_code(level: &str) -> &'static str {
    match level {
        "fatal error" => "fatal-error",
        "" => "diagnostic",
        "warning" => "warning",
        "note" => "note",
        _ => "error",
    }
}

fn combined_lossy(stdout: &[u8], stderr: &[u8]) -> String {
    let mut output = String::from_utf8_lossy(stdout).into_owned();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(&String::from_utf8_lossy(stderr));
    output
}

fn to_u32(value: u64) -> Option<u32> {
    u32::try_from(value).ok()
}
