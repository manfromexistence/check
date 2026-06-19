use serde_json::Value;

use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

use super::common::{combined_lossy, invalid_runner_output};

const MAX_WEB_AUDIT_DIAGNOSTICS: usize = 32;
const MAX_WEB_LIGHTHOUSE_AUDITS: usize = 32;
const MAX_WEB_AUDIT_TEXT_CHARS: usize = 320;

pub(in crate::diagnostics) fn parse_web_audit_json(
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<DxDiagnostic> {
    let output = combined_lossy(stdout, stderr);
    if output.trim().is_empty() {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser received empty web audit JSON output",
            &output,
        )];
    }
    let Ok(value) = serde_json::from_str::<Value>(&output) else {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser received invalid web audit JSON",
            &output,
        )];
    };
    let Some(object) = value.as_object() else {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser expected a JSON object",
            &output,
        )];
    };

    let url = object
        .get("url")
        .and_then(Value::as_str)
        .and_then(bounded_text);
    let has_diagnostics = object.contains_key("diagnostics");
    let has_lighthouse = object.contains_key("lighthouse");
    if !has_diagnostics && !has_lighthouse {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser expected diagnostics or lighthouse evidence",
            &output,
        )];
    }
    if object
        .get("status")
        .is_some_and(|status| !valid_status_value(status))
    {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser found invalid status",
            &output,
        )];
    }
    let mut parsed = Vec::new();
    let mut evidence_items = 0usize;
    if let Some(diagnostics) = object.get("diagnostics") {
        let Some(diagnostics) = diagnostics.as_array() else {
            return vec![invalid_runner_output(
                source,
                "web audit JSON parser expected diagnostics to be an array",
                &output,
            )];
        };
        let mut emitted = 0;
        for diagnostic in diagnostics {
            evidence_items = evidence_items.saturating_add(1);
            match diagnostic_from_value(source, url.as_deref(), diagnostic) {
                Ok(diagnostic) => push_display_capped_diagnostic(
                    &mut parsed,
                    diagnostic,
                    &mut emitted,
                    MAX_WEB_AUDIT_DIAGNOSTICS,
                ),
                Err(reason) => return vec![invalid_runner_output(source, reason, &output)],
            }
        }
    }

    if let Some(lighthouse) = object.get("lighthouse") {
        let Some(lighthouse_object) = lighthouse.as_object() else {
            parsed.push(invalid_runner_output(
                source,
                "web audit JSON parser expected lighthouse to be a JSON object",
                &output,
            ));
            return parsed;
        };
        let Some(audits) = lighthouse_object.get("audits") else {
            parsed.push(invalid_runner_output(
                source,
                "web audit JSON parser expected lighthouse.audits to be an array",
                &output,
            ));
            return parsed;
        };
        let Some(audits) = audits.as_array() else {
            parsed.push(invalid_runner_output(
                source,
                "web audit JSON parser expected lighthouse.audits to be an array",
                &output,
            ));
            return parsed;
        };
        let mut emitted = 0;
        let mut non_ready_audits = 0usize;
        for audit in audits {
            evidence_items = evidence_items.saturating_add(1);
            match lighthouse_audit_from_value(source, url.as_deref(), audit) {
                Ok(Some(diagnostic)) => {
                    non_ready_audits = non_ready_audits.saturating_add(1);
                    push_display_capped_diagnostic(
                        &mut parsed,
                        diagnostic,
                        &mut emitted,
                        MAX_WEB_LIGHTHOUSE_AUDITS,
                    );
                }
                Ok(None) => {}
                Err(reason) => return vec![invalid_runner_output(source, reason, &output)],
            }
        }
        if non_ready_audits == 0 && lighthouse_report_score_is_failing(lighthouse_object) {
            return vec![invalid_runner_output(
                source,
                "web audit JSON parser found failing Lighthouse score without non-ready audit evidence",
                &output,
            )];
        }
    }

    if parsed.is_empty() && evidence_items == 0 {
        return vec![invalid_runner_output(
            source,
            "web audit JSON parser received empty evidence",
            &output,
        )];
    }

    parsed
}

fn push_display_capped_diagnostic(
    parsed: &mut Vec<DxDiagnostic>,
    diagnostic: DxDiagnostic,
    emitted: &mut usize,
    cap: usize,
) {
    if *emitted < cap || diagnostic.severity != DxSeverity::Info {
        parsed.push(diagnostic);
    }
    *emitted = emitted.saturating_add(1);
}

fn diagnostic_from_value(
    source: &str,
    url: Option<&str>,
    value: &Value,
) -> Result<DxDiagnostic, &'static str> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .and_then(bounded_text)
        .ok_or("web audit JSON parser found diagnostics item without id")?;
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .and_then(bounded_text)
        .ok_or("web audit JSON parser found diagnostics item without message")?;
    let next_action = bounded_text(
        value
            .get("next_action")
            .and_then(Value::as_str)
            .unwrap_or("Fix the web audit diagnostic, then rerun dx check."),
    )
    .ok_or("web audit JSON parser found diagnostics item without next_action")?;
    let severity = diagnostic_severity(value.get("severity"))?;

    Ok(DxDiagnostic {
        id: format!("{source}:{id}"),
        source: source.to_string(),
        severity,
        file: url.map(ToOwned::to_owned),
        line: None,
        column: None,
        message,
        next_action,
        measurement: DxMeasurementKind::Measured,
    })
}

fn lighthouse_audit_from_value(
    source: &str,
    url: Option<&str>,
    value: &Value,
) -> Result<Option<DxDiagnostic>, &'static str> {
    let status = lighthouse_status(value.get("status"))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .and_then(bounded_text)
        .ok_or("web audit JSON parser found lighthouse audit without id")?;
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .and_then(bounded_text)
        .ok_or("web audit JSON parser found lighthouse audit without label")?;
    let detail = value
        .get("detail")
        .and_then(Value::as_str)
        .and_then(bounded_text)
        .ok_or("web audit JSON parser found lighthouse audit without detail")?;
    let next_action = bounded_text(
        value
            .get("next_action")
            .and_then(Value::as_str)
            .unwrap_or("Fix the web audit finding, then rerun dx check."),
    )
    .ok_or("web audit JSON parser found lighthouse audit without next_action")?;
    if status == LighthouseAuditStatus::Ready {
        let (score, max_score) = required_lighthouse_score_pair(value)?;
        if score != max_score {
            return Err("web audit JSON parser found ready lighthouse audit with failing score");
        }
        return Ok(None);
    }
    if let Some((score, max_score)) = optional_lighthouse_score_pair(value)?
        && score == max_score
    {
        return Err("web audit JSON parser found non-ready lighthouse audit with passing score");
    }
    let severity = match status {
        LighthouseAuditStatus::Ready => unreachable!(),
        LighthouseAuditStatus::Warning => DxSeverity::Warning,
        LighthouseAuditStatus::Blocked => DxSeverity::Failure,
    };

    Ok(Some(DxDiagnostic {
        id: format!("{source}:{id}"),
        source: source.to_string(),
        severity,
        file: url.map(ToOwned::to_owned),
        line: None,
        column: None,
        message: bounded_text(&format!("{label}: {detail}"))
            .ok_or("web audit JSON parser found lighthouse audit without detail")?,
        next_action,
        measurement: DxMeasurementKind::Measured,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LighthouseAuditStatus {
    Ready,
    Warning,
    Blocked,
}

fn diagnostic_severity(value: Option<&Value>) -> Result<DxSeverity, &'static str> {
    let value = value.ok_or("web audit JSON parser found diagnostics item without severity")?;
    let value = value
        .as_str()
        .ok_or("web audit JSON parser found diagnostics item with invalid severity")?;
    severity_from_str(value)
        .ok_or("web audit JSON parser found diagnostics item with unknown severity")
}

fn severity_from_str(value: &str) -> Option<DxSeverity> {
    match value {
        "failure" | "error" => Some(DxSeverity::Failure),
        "warning" | "warn" => Some(DxSeverity::Warning),
        "info" | "information" => Some(DxSeverity::Info),
        _ => None,
    }
}

fn lighthouse_status(value: Option<&Value>) -> Result<LighthouseAuditStatus, &'static str> {
    let value = value.ok_or("web audit JSON parser found lighthouse audit without status")?;
    let value = value
        .as_str()
        .ok_or("web audit JSON parser found lighthouse audit with invalid status")?;
    match value {
        "ready" => Ok(LighthouseAuditStatus::Ready),
        "warning" | "warn" => Ok(LighthouseAuditStatus::Warning),
        "blocked" | "failure" | "failed" => Ok(LighthouseAuditStatus::Blocked),
        _ => Err("web audit JSON parser found lighthouse audit with unknown status"),
    }
}

fn lighthouse_score(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

fn required_lighthouse_score_pair(value: &Value) -> Result<(u64, u64), &'static str> {
    let score = lighthouse_score(value.get("score"));
    let max_score = lighthouse_score(value.get("max_score"));
    match (score, max_score) {
        (Some(score), Some(max_score)) => Ok((score, max_score)),
        (None, _) => Err("web audit JSON parser found lighthouse audit without valid score"),
        (_, None) => Err("web audit JSON parser found lighthouse audit without valid max_score"),
    }
}

fn optional_lighthouse_score_pair(value: &Value) -> Result<Option<(u64, u64)>, &'static str> {
    if value.get("score").is_none() && value.get("max_score").is_none() {
        return Ok(None);
    }
    required_lighthouse_score_pair(value).map(Some)
}

fn lighthouse_report_score_is_failing(object: &serde_json::Map<String, Value>) -> bool {
    let Some(score) = lighthouse_score(object.get("score")) else {
        return false;
    };
    let Some(max_score) = lighthouse_score(object.get("max_score")) else {
        return false;
    };
    score < max_score
}

fn valid_status_value(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|status| (100..=599).contains(&status))
}

fn bounded_text(value: &str) -> Option<String> {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_WEB_AUDIT_TEXT_CHARS {
        return Some(text);
    }
    let mut bounded = text
        .chars()
        .take(MAX_WEB_AUDIT_TEXT_CHARS.saturating_sub(3))
        .collect::<String>();
    bounded.push_str("...");
    Some(bounded)
}
