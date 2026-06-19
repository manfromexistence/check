use serde::{Deserialize, Serialize};

pub const LITEHOUSE_SCHEMA_VERSION: &str = "dx.check.web_lighthouse";
pub const LITEHOUSE_ENGINE: &str = "dx-litehouse";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitehouseReport {
    pub schema_version: String,
    pub engine: String,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<String>,
    pub id: String,
    pub target_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_url: Option<String>,
    pub score: u16,
    pub max_score: u16,
    pub artifact: LitehouseArtifactSummary,
    pub categories: Vec<LitehouseCategory>,
    pub audits: Vec<LitehouseAudit>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitehouseArtifactSummary {
    pub status: Option<u16>,
    pub response_time_ms: Option<u128>,
    pub html_bytes: Option<u64>,
    pub body_truncated: bool,
    pub security_header_count: u16,
    pub header_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitehouseCategory {
    pub id: String,
    pub label: String,
    pub score: u16,
    pub max_score: u16,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitehouseAudit {
    pub id: String,
    pub category: String,
    pub label: String,
    pub score: u16,
    pub max_score: u16,
    pub status: String,
    pub detail: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitehouseHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitehousePageArtifact {
    pub target_id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub status: Option<u16>,
    pub response_time_ms: Option<u128>,
    pub html_bytes: Option<u64>,
    pub body_truncated: bool,
    pub required_status: Option<u16>,
    pub max_html_bytes: Option<u64>,
    pub security_header_count: u16,
    pub headers: Vec<LitehouseHeader>,
    pub html: String,
}

impl LitehousePageArtifact {
    pub fn summary(&self) -> LitehouseArtifactSummary {
        LitehouseArtifactSummary {
            status: self.status,
            response_time_ms: self.response_time_ms,
            html_bytes: self.html_bytes,
            body_truncated: self.body_truncated,
            security_header_count: self.security_header_count,
            header_count: self.headers.len(),
        }
    }
}
