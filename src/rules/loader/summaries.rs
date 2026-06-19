use std::fs;
use std::path::{Path, PathBuf};

use serializer::DxDocument;

use crate::model::{DxRuleDefinition, DxRulePackStatus, DxRulePackSummary};
use crate::registry::RulePackLockEntry;
use crate::rule_pack::{pack_id_from_document, pack_version_from_document};

use super::types::{RulePackSource, RulePackSourceTrust, RulePackSummaryInput};

impl RulePackSourceTrust {
    pub(super) fn registry_source(&self) -> String {
        match self {
            Self::Local => "project-local".to_string(),
            Self::Locked { entry, .. } => entry.source.clone(),
        }
    }

    pub(super) fn signed(&self) -> Option<bool> {
        match self {
            Self::Local => None,
            Self::Locked { entry, .. } => Some(entry.signed),
        }
    }

    pub(super) fn signer(&self) -> Option<String> {
        match self {
            Self::Local => None,
            Self::Locked { entry, .. } => entry.signer.clone(),
        }
    }

    pub(super) fn signature_status(&self) -> Option<String> {
        match self {
            Self::Local => None,
            Self::Locked {
                signature_status, ..
            } => Some(signature_status.clone()),
        }
    }

    pub(super) fn default_provenance(&self) -> Option<String> {
        match self {
            Self::Local => Some("local".to_string()),
            Self::Locked { entry, .. } => entry.provenance.clone(),
        }
    }

    pub(super) fn summary_provenance(&self, parsed_provenance: Option<String>) -> Option<String> {
        match self {
            Self::Local => parsed_provenance.or_else(|| self.default_provenance()),
            Self::Locked { entry, .. } => entry.provenance.clone().or(parsed_provenance),
        }
    }

    pub(super) fn invalid_lock_status(&self) -> String {
        match self {
            Self::Local => "local-invalid".to_string(),
            Self::Locked { .. } => "locked-invalid".to_string(),
        }
    }

    pub(super) fn lock_status(&self, status: &DxRulePackStatus) -> String {
        match self {
            Self::Local => rule_pack_lock_status(status).to_string(),
            Self::Locked { entry, .. } if entry.signed => "locked-signed".to_string(),
            Self::Locked { .. } => "locked-unsigned".to_string(),
        }
    }
}

pub(super) fn summary_for_source(
    source: &RulePackSource,
    input: RulePackSummaryInput,
) -> DxRulePackSummary {
    DxRulePackSummary {
        id: input.id,
        version: input.version,
        status: input.status,
        source_path: Some(source.path.display().to_string()),
        machine_path: input.machine_path,
        source_hash: source_hash(&source.path),
        registry_source: Some(source.trust.registry_source()),
        provenance: source.trust.summary_provenance(input.provenance),
        lock_status: Some(input.lock_status),
        signed: source.trust.signed(),
        signer: source.trust.signer(),
        signature_status: source.trust.signature_status(),
        rule_count: input.rule_count,
    }
}

pub(super) fn rejected_locked_summary(
    entry: &RulePackLockEntry,
    source_path: Option<PathBuf>,
    signature_status: Option<String>,
) -> DxRulePackSummary {
    DxRulePackSummary {
        id: entry.id.clone(),
        version: entry.version.clone(),
        status: DxRulePackStatus::Invalid,
        source_path: source_path.as_ref().map(|path| path.display().to_string()),
        machine_path: None,
        source_hash: source_path.as_ref().and_then(|path| source_hash(path)),
        registry_source: Some(entry.source.clone()),
        provenance: entry.provenance.clone(),
        lock_status: Some("rejected".to_string()),
        signed: Some(entry.signed),
        signer: entry.signer.clone(),
        signature_status,
        rule_count: 0,
    }
}

pub(super) fn unlisted_local_rule_pack_summary(
    source: &Path,
    document: &DxDocument,
) -> DxRulePackSummary {
    DxRulePackSummary {
        id: pack_id_from_document(document, source),
        version: pack_version_from_document(document),
        status: DxRulePackStatus::Invalid,
        source_path: Some(source.display().to_string()),
        machine_path: None,
        source_hash: source_hash(source),
        registry_source: Some("project-local".to_string()),
        provenance: Some("local".to_string()),
        lock_status: Some("unlisted-local".to_string()),
        signed: None,
        signer: None,
        signature_status: None,
        rule_count: 0,
    }
}

pub(super) fn source_hash(source: &Path) -> Option<String> {
    fs::read(source)
        .ok()
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
}

pub(super) fn pack_provenance(rules: &[DxRuleDefinition]) -> Option<String> {
    let mut provenance = rules
        .iter()
        .filter_map(|rule| rule.provenance.as_deref())
        .filter(|value| !value.trim().is_empty());
    let first = provenance.next()?.to_string();
    if provenance.all(|value| value == first) {
        Some(first)
    } else {
        Some("mixed".to_string())
    }
}

fn rule_pack_lock_status(status: &DxRulePackStatus) -> &'static str {
    match status {
        DxRulePackStatus::BuiltIn => "built-in",
        DxRulePackStatus::MachineFresh => "local-machine-fresh",
        DxRulePackStatus::MachineGenerated => "local-machine-generated",
        DxRulePackStatus::SourceOnly => "local-source-only",
        DxRulePackStatus::Invalid => "local-invalid",
    }
}
