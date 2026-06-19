use std::path::PathBuf;

use crate::model::{DxDiagnostic, DxRuleCategoryDefinition, DxRuleDefinition, DxRulePackStatus};
use crate::registry::RulePackLockEntry;

#[derive(Debug, Clone, Default)]
pub struct LoadedRulePackSet {
    pub summaries: Vec<crate::model::DxRulePackSummary>,
    pub categories: Vec<DxRuleCategoryDefinition>,
    pub rules: Vec<DxRuleDefinition>,
    pub diagnostics: Vec<DxDiagnostic>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RulePackLoadOptions {
    pub allow_writes: bool,
    pub strict_rule_packs: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RulePackSource {
    pub(super) path: PathBuf,
    pub(super) trust: RulePackSourceTrust,
}

#[derive(Debug, Clone, Default)]
pub(super) struct LockedRulePackSources {
    pub(super) sources: Vec<RulePackSource>,
    pub(super) strict_local_sources: bool,
    pub(super) lock_present: bool,
}

#[derive(Debug, Clone)]
pub(super) enum RulePackSourceTrust {
    Local,
    Locked {
        entry: Box<RulePackLockEntry>,
        signature_status: String,
    },
}

pub(super) struct RulePackSummaryInput {
    pub(super) id: String,
    pub(super) version: String,
    pub(super) status: DxRulePackStatus,
    pub(super) machine_path: Option<String>,
    pub(super) provenance: Option<String>,
    pub(super) lock_status: String,
    pub(super) rule_count: usize,
}
