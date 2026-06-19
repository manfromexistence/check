use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;

use serde_json::Value;

use crate::model::{
    LITEHOUSE_ENGINE, LITEHOUSE_SCHEMA_VERSION, LitehouseArtifactSummary, LitehouseCategory,
    LitehouseReport,
};
use crate::runner::{audit, category_status_for_score};

const IMPORT_MODE: &str = "lighthouse-json-import";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitehouseImportError {
    message: String,
}

impl LitehouseImportError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for LitehouseImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LitehouseImportError {}

pub fn import_lighthouse_result(
    value: &Value,
    target_id: &str,
) -> Result<LitehouseReport, LitehouseImportError> {
    let object = value
        .as_object()
        .ok_or_else(|| LitehouseImportError::new("Lighthouse import expected a JSON object"))?;
    reject_runtime_error(object)?;
    let requested_url = object
        .get("requestedUrl")
        .or_else(|| object.get("requested_url"))
        .and_then(Value::as_str)
        .ok_or_else(|| LitehouseImportError::new("Lighthouse import is missing requestedUrl"))?;
    let final_url = object
        .get("finalDisplayedUrl")
        .or_else(|| object.get("finalUrl"))
        .or_else(|| object.get("final_url"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let category_values = object
        .get("categories")
        .and_then(Value::as_object)
        .ok_or_else(|| LitehouseImportError::new("Lighthouse import is missing categories"))?;
    let audit_values = object
        .get("audits")
        .and_then(Value::as_object)
        .ok_or_else(|| LitehouseImportError::new("Lighthouse import is missing audits"))?;
    if category_values.is_empty() {
        return Err(LitehouseImportError::new(
            "Lighthouse import categories are empty",
        ));
    }
    if audit_values.is_empty() {
        return Err(LitehouseImportError::new(
            "Lighthouse import audits are empty",
        ));
    }

    let mut categories = Vec::new();
    let mut audits_by_id = BTreeMap::new();
    let audit_category_map = audit_category_map(category_values, audit_values)?;

    for (category_key, category_value) in category_values {
        let id = category_value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(category_key);
        let label = category_value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(id);
        let score = score_value(category_value.get("score"));
        categories.push(LitehouseCategory {
            id: id.to_string(),
            label: label.to_string(),
            score,
            max_score: 100,
            status: category_status_for_score(score).to_string(),
        });
    }

    for (audit_key, audit_value) in audit_values {
        let id = audit_value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(audit_key);
        let Some(category) = audit_category_map.get(id) else {
            continue;
        };
        let label = audit_value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(id);
        let score = score_value(audit_value.get("score"));
        let detail = audit_value
            .get("displayValue")
            .or_else(|| audit_value.get("description"))
            .and_then(Value::as_str)
            .unwrap_or("Imported Lighthouse audit did not include a display value.");
        let next_action = audit_value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("Review the imported Lighthouse audit and fix the underlying page issue.");

        audits_by_id.insert(
            id.to_string(),
            audit(category, id, label, score, detail, next_action),
        );
    }

    let audits = audits_by_id.into_values().collect::<Vec<_>>();
    if audits.is_empty() {
        return Err(LitehouseImportError::new(
            "Lighthouse import included no audits referenced by categories",
        ));
    }
    let score = categories
        .iter()
        .map(|category| category.score)
        .sum::<u16>();
    let max_score = categories
        .iter()
        .map(|category| category.max_score)
        .sum::<u16>();

    Ok(LitehouseReport {
        schema_version: LITEHOUSE_SCHEMA_VERSION.to_string(),
        engine: LITEHOUSE_ENGINE.to_string(),
        mode: IMPORT_MODE.to_string(),
        fallback_from: None,
        id: format!("{target_id}-lighthouse"),
        target_id: target_id.to_string(),
        url: requested_url.to_string(),
        final_url,
        score,
        max_score,
        artifact: LitehouseArtifactSummary {
            status: None,
            response_time_ms: None,
            html_bytes: None,
            body_truncated: false,
            security_header_count: 0,
            header_count: 0,
        },
        categories,
        audits,
    })
}

fn audit_category_map(
    category_values: &serde_json::Map<String, Value>,
    audit_values: &serde_json::Map<String, Value>,
) -> Result<HashMap<String, String>, LitehouseImportError> {
    let mut map = HashMap::new();
    for (category_key, category_value) in category_values {
        let category_id = category_value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(category_key);
        let Some(audit_refs) = category_value.get("auditRefs").and_then(Value::as_array) else {
            return Err(LitehouseImportError::new(format!(
                "Lighthouse import category {category_id} has no auditRefs"
            )));
        };
        if audit_refs.is_empty() {
            return Err(LitehouseImportError::new(format!(
                "Lighthouse import category {category_id} has empty auditRefs"
            )));
        }
        let mut contributed = false;
        for audit_ref in audit_refs {
            if let Some(audit_id) = audit_ref.get("id").and_then(Value::as_str) {
                if !audit_values.contains_key(audit_id) {
                    return Err(LitehouseImportError::new(format!(
                        "Lighthouse import category {category_id} references missing audit {audit_id}"
                    )));
                }
                map.entry(audit_id.to_string())
                    .or_insert_with(|| category_id.to_string());
                contributed = true;
            }
        }
        if !contributed {
            return Err(LitehouseImportError::new(format!(
                "Lighthouse import category {category_id} auditRefs include no audit ids"
            )));
        }
    }
    Ok(map)
}

fn reject_runtime_error(
    object: &serde_json::Map<String, Value>,
) -> Result<(), LitehouseImportError> {
    let Some(runtime_error) = object.get("runtimeError").filter(|value| !value.is_null()) else {
        return Ok(());
    };

    let code = runtime_error.get("code").and_then(Value::as_str);
    let message = runtime_error.get("message").and_then(Value::as_str);
    let detail = match (code, message) {
        (Some(code), Some(message)) => format!(": {code} - {message}"),
        (Some(code), None) => format!(": {code}"),
        (None, Some(message)) => format!(": {message}"),
        (None, None) => String::new(),
    };

    Err(LitehouseImportError::new(format!(
        "Lighthouse import includes runtimeError{detail}"
    )))
}

fn score_value(value: Option<&Value>) -> u16 {
    let Some(value) = value else {
        return 0;
    };
    if let Some(score) = value.as_f64() {
        return ((score.clamp(0.0, 1.0) * 100.0).round() as u16).min(100);
    }
    0
}
