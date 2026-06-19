use std::{collections::BTreeSet, path::Path};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serializer::{DxDocument, DxLlmValue, DxSection, llm_to_document};

use crate::rule_pack::{has_rule_pack_marker, pack_id_from_document, pack_version_from_document};

mod signature;
#[cfg(test)]
mod tests;

pub use signature::rule_pack_signature_payload;
use signature::verify_signature;

const RULE_PACK_LOCK_SCHEMA: &str = "dx.check.rule_pack_lock.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulePackLock {
    pub schema: String,
    pub strict: bool,
    pub registry: String,
    pub entries: Vec<RulePackLockEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulePackLockEntry {
    pub id: String,
    pub version: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
    pub hash_blake3: String,
    pub signed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key_ed25519: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_ed25519: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulePackTrustDecision {
    Accepted {
        signature_status: String,
    },
    Rejected {
        reason: String,
        signature_status: Option<String>,
    },
}

pub fn rule_pack_lock_from_document(document: &DxDocument, source: &Path) -> Result<RulePackLock> {
    let schema = document
        .get_path("rule_pack_lock.schema")
        .and_then(value_string)
        .unwrap_or_else(|| RULE_PACK_LOCK_SCHEMA.to_string());
    if schema != RULE_PACK_LOCK_SCHEMA {
        return Err(anyhow!(
            "{} has unsupported rule-pack lock schema `{schema}`",
            source.display()
        ));
    }

    let strict = match document.get_path("rule_pack_lock.strict") {
        Some(value) => required_bool(value, source, "rule_pack_lock.strict")?,
        None => false,
    };
    let registry = document
        .get_path("rule_pack_lock.registry")
        .and_then(value_string)
        .unwrap_or_else(|| "forge-r2".to_string());
    let section = document
        .section_by_name("locks")
        .ok_or_else(|| anyhow!("{} is missing a locks section", source.display()))?;

    let id_index = required_column(section, source, "id")?;
    let version_index = required_column(section, source, "version")?;
    let source_index = required_column(section, source, "source")?;
    let hash_index = required_column(section, source, "hash_blake3")?;
    let cache_index = section.column_index("cache");
    let signed_index = section.column_index("signed");
    let provenance_index = section.column_index("provenance");
    let signer_index = section.column_index("signer");
    let public_key_index = section.column_index("public_key_ed25519");
    let signature_index = section.column_index("signature_ed25519");

    let mut entries = Vec::new();
    let mut identities = BTreeSet::new();
    let mut cache_paths = BTreeSet::new();
    for (index, row) in section.rows.iter().enumerate() {
        let row_label = format!("row {}", index + 1);
        let id = required_cell(row.get(id_index), source, &row_label, "id")?;
        let version = required_cell(row.get(version_index), source, &row_label, "version")?;
        let source_url = required_cell(row.get(source_index), source, &row_label, "source")?;
        let hash_blake3 = required_cell(row.get(hash_index), source, &row_label, "hash_blake3")?;
        if hash_blake3.len() != 64 || !hash_blake3.chars().all(|char| char.is_ascii_hexdigit()) {
            return Err(anyhow!(
                "{} locks {row_label} has an invalid BLAKE3 hash",
                source.display()
            ));
        }
        if !identities.insert((id.clone(), version.clone())) {
            return Err(anyhow!(
                "{} locks {row_label} duplicates rule pack `{id}` version `{version}`",
                source.display()
            ));
        }
        let cache_path = optional_cell(row, cache_index);
        if let Some(cache_path) = cache_path.as_deref() {
            let cache_identity = cache_path_identity(cache_path);
            if !cache_paths.insert(cache_identity.clone()) {
                return Err(anyhow!(
                    "{} locks {row_label} duplicates cache path `{cache_identity}`",
                    source.display()
                ));
            }
        }
        let signed =
            optional_bool_cell(row, signed_index, source, &row_label, "signed")?.unwrap_or(false);
        entries.push(RulePackLockEntry {
            id,
            version,
            source: source_url,
            cache_path,
            hash_blake3,
            signed,
            provenance: optional_cell(row, provenance_index),
            signer: optional_cell(row, signer_index),
            public_key_ed25519: optional_cell(row, public_key_index),
            signature_ed25519: optional_cell(row, signature_index),
        });
    }

    Ok(RulePackLock {
        schema,
        strict,
        registry,
        entries,
    })
}

fn cache_path_identity(cache_path: &str) -> String {
    let parts = Path::new(cache_path.trim())
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            std::path::Component::CurDir => None,
            std::path::Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_string())
            }
            std::path::Component::RootDir => Some("/".to_string()),
            std::path::Component::ParentDir => Some("..".to_string()),
        })
        .collect::<Vec<_>>();

    let identity = if parts.is_empty() {
        cache_path.trim().to_string()
    } else {
        parts.join("/")
    };

    identity.to_ascii_lowercase()
}

pub fn verify_cached_pack(
    path: &Path,
    lock: &RulePackLockEntry,
    strict: bool,
) -> RulePackTrustDecision {
    let Ok(bytes) = std::fs::read(path) else {
        return rejected(
            format!("cached pack {} is unreadable", path.display()),
            None,
        );
    };
    let actual = blake3::hash(&bytes).to_hex().to_string();
    if actual != lock.hash_blake3 {
        return rejected("cached pack hash does not match lockfile", None);
    }
    if strict && !lock.signed {
        return rejected(
            "strict mode requires a signed rule pack",
            Some("unsigned".to_string()),
        );
    }
    let signature_status = match verify_signature(lock, strict) {
        Ok(status) => status,
        Err((reason, status)) => return rejected(reason, status),
    };

    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            return rejected(
                "cached pack is not valid UTF-8 serializer source",
                Some(signature_status),
            );
        }
    };
    let document = match llm_to_document(&source) {
        Ok(document) => document,
        Err(error) => {
            return rejected(
                format!("cached pack is malformed serializer .sr: {error}"),
                Some(signature_status),
            );
        }
    };
    if !has_rule_pack_marker(&document) {
        return rejected(
            "cached pack is not a DX Check rule pack",
            Some(signature_status),
        );
    }
    let actual_id = pack_id_from_document(&document, path);
    let actual_version = pack_version_from_document(&document);
    if actual_id != lock.id || actual_version != lock.version {
        return rejected(
            format!(
                "cached pack id/version `{actual_id}`/`{actual_version}` does not match lockfile `{}`/`{}`",
                lock.id, lock.version
            ),
            Some(signature_status),
        );
    }
    RulePackTrustDecision::Accepted { signature_status }
}

fn rejected(reason: impl Into<String>, signature_status: Option<String>) -> RulePackTrustDecision {
    RulePackTrustDecision::Rejected {
        reason: reason.into(),
        signature_status,
    }
}

fn required_column(section: &DxSection, source: &Path, column: &str) -> Result<usize> {
    section
        .column_index(column)
        .ok_or_else(|| anyhow!("{} locks section is missing `{column}`", source.display()))
}

fn required_cell(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    column: &str,
) -> Result<String> {
    value
        .and_then(value_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow!(
                "{} locks {row_label} is missing `{column}`",
                source.display()
            )
        })
}

fn optional_cell(row: &[DxLlmValue], index: Option<usize>) -> Option<String> {
    index
        .and_then(|index| row.get(index))
        .and_then(value_string)
        .filter(|value| !value.trim().is_empty())
}

fn optional_bool_cell(
    row: &[DxLlmValue],
    index: Option<usize>,
    source: &Path,
    row_label: &str,
    column: &str,
) -> Result<Option<bool>> {
    let Some(value) = index.and_then(|index| row.get(index)) else {
        return Ok(None);
    };
    if cell_is_empty(value) {
        return Ok(None);
    }
    required_bool(value, source, &format!("locks {row_label} `{column}`")).map(Some)
}

fn required_bool(value: &DxLlmValue, source: &Path, label: &str) -> Result<bool> {
    value_bool(value).ok_or_else(|| {
        anyhow!(
            "{} {label} must be a boolean value (`true` or `false`)",
            source.display()
        )
    })
}

fn cell_is_empty(value: &DxLlmValue) -> bool {
    value_string(value).is_some_and(|value| value.trim().is_empty())
}

fn value_string(value: &DxLlmValue) -> Option<String> {
    match value {
        DxLlmValue::Str(value) | DxLlmValue::Ref(value) => Some(value.clone()),
        DxLlmValue::Num(_) | DxLlmValue::Bool(_) => Some(value.to_string()),
        DxLlmValue::Null | DxLlmValue::Arr(_) | DxLlmValue::Obj(_) => None,
    }
}

fn value_bool(value: &DxLlmValue) -> Option<bool> {
    match value {
        DxLlmValue::Bool(value) => Some(*value),
        DxLlmValue::Str(value) => match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}
