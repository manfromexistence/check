use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxCheckOutputFormat {
    Terminal,
    Json,
    Llm,
    Machine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DxToolTarget {
    Lint,
    Format,
    Typecheck,
    Test,
    Audit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxSeverity {
    Info,
    Warning,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxMeasurementKind {
    Measured,
    Imported,
    Estimated,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxDiagnostic {
    pub id: String,
    pub source: String,
    pub severity: DxSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
    pub next_action: String,
    pub measurement: DxMeasurementKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxFinding {
    pub id: String,
    pub category: String,
    pub severity: DxSeverity,
    pub message: String,
    pub next_action: String,
    pub measurement: DxMeasurementKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weight: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

fn is_zero(value: &u16) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxCheckEngineOptions {
    pub allow_writes: bool,
    pub strict_rule_packs: bool,
    pub output_format: DxCheckOutputFormat,
    pub run_targets: Vec<DxToolTarget>,
}

impl Default for DxCheckEngineOptions {
    fn default() -> Self {
        Self {
            allow_writes: true,
            strict_rule_packs: false,
            output_format: DxCheckOutputFormat::Terminal,
            run_targets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxRulePackStatus {
    #[default]
    BuiltIn,
    MachineFresh,
    MachineGenerated,
    SourceOnly,
    Invalid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxRulePackSummary {
    pub id: String,
    pub version: String,
    pub status: DxRulePackStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_status: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub rule_count: usize,
}

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxScoreStatus {
    #[default]
    Ready,
    Warning,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxScoreSummary {
    pub schema_version: String,
    pub profile: String,
    pub score: u16,
    pub max_score: u16,
    pub status: DxScoreStatus,
    pub finding_weight_total: u16,
    pub failure_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buckets: Vec<DxScoreBucketSummary>,
}

impl Default for DxScoreSummary {
    fn default() -> Self {
        Self {
            schema_version: "dx.check.engine_score.v1".to_string(),
            profile: "dx-check-engine.rules.v1".to_string(),
            score: 500,
            max_score: 500,
            status: DxScoreStatus::Ready,
            finding_weight_total: 0,
            failure_count: 0,
            warning_count: 0,
            info_count: 0,
            buckets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxRuleCategoryDefinition {
    pub id: String,
    pub label: String,
    pub weight: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxScoreBucketSummary {
    pub id: String,
    pub label: String,
    pub score: u16,
    pub max_score: u16,
    pub status: DxScoreStatus,
    pub finding_weight_total: u16,
    pub failure_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxRuleDefinition {
    pub id: String,
    pub category: String,
    pub severity: DxSeverity,
    pub weight: u16,
    pub metric: String,
    pub operator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxToolPlan {
    pub id: String,
    pub target: DxToolTarget,
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub detected_from: Vec<String>,
    pub parser: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DxWebLighthouseMode {
    #[default]
    Native,
    Official,
    Auto,
}

impl DxWebLighthouseMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Official => "official",
            Self::Auto => "auto",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "native" => Some(Self::Native),
            "official" => Some(Self::Official),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxWebAuditTarget {
    pub id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_html_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighthouse_mode: Option<DxWebLighthouseMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxWebAuditResult {
    pub id: String,
    pub target_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_bytes: Option<u64>,
    pub title_present: bool,
    pub description_present: bool,
    pub canonical_present: bool,
    pub viewport_present: bool,
    pub security_header_count: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DxToolRunStatus {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxToolProcessOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxToolRunResult {
    pub plan: DxToolPlan,
    pub status: DxToolRunStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub diagnostics: Vec<DxDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxTestInventory {
    pub rust_tests: usize,
    pub js_tests: usize,
    pub python_tests: usize,
    pub go_tests: usize,
    #[serde(default)]
    pub c_tests: usize,
    #[serde(default)]
    pub cpp_tests: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxCheckEngineReport {
    #[serde(default)]
    pub score: DxScoreSummary,
    pub rule_packs: Vec<DxRulePackSummary>,
    pub findings: Vec<DxFinding>,
    pub diagnostics: Vec<DxDiagnostic>,
    pub adapter_plans: Vec<DxToolPlan>,
    #[serde(default)]
    pub web_audit_targets: Vec<DxWebAuditTarget>,
    #[serde(default)]
    pub web_audit_results: Vec<DxWebAuditResult>,
    pub test_inventory: DxTestInventory,
    pub checked_paths: Vec<String>,
}
