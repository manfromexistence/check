use dx_litehouse::{LitehouseHeader, LitehousePageArtifact, LitehouseReport, LitehouseRunner};
use serde_json::Value;

use crate::web_audit_runner::{
    DxWebAuditHtmlMetadata, DxWebAuditObservedResponse, DxWebAuditRunnerRequest,
};

pub type DxWebLighthouseReport = LitehouseReport;
pub type DxWebLighthouseCategory = dx_litehouse::LitehouseCategory;
pub type DxWebLighthouseAudit = dx_litehouse::LitehouseAudit;

pub fn build_web_lighthouse_report(
    request: &DxWebAuditRunnerRequest,
    observed: &DxWebAuditObservedResponse,
    _metadata: &DxWebAuditHtmlMetadata,
    security_header_count: u16,
    html: &str,
) -> DxWebLighthouseReport {
    LitehouseRunner.audit(&LitehousePageArtifact {
        target_id: request.id.clone(),
        url: request.url.clone(),
        final_url: Some(observed.final_url.clone()),
        status: Some(observed.status),
        response_time_ms: Some(observed.response_time_ms),
        html_bytes: Some(observed.html_bytes),
        body_truncated: observed.body_truncated,
        required_status: request.required_status,
        max_html_bytes: request.max_html_bytes,
        security_header_count,
        headers: observed
            .headers
            .iter()
            .map(|header| LitehouseHeader {
                name: header.name.clone(),
                value: header.value.clone(),
            })
            .collect(),
        html: html.to_string(),
    })
}

pub fn build_failed_web_lighthouse_report(
    request: &DxWebAuditRunnerRequest,
    audit_id: &str,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxWebLighthouseReport {
    LitehouseRunner.failed_report(&request.id, &request.url, audit_id, message, next_action)
}

pub fn import_official_lighthouse_report(
    value: &Value,
    target_id: &str,
) -> Result<DxWebLighthouseReport, String> {
    dx_litehouse::import_lighthouse_result(value, target_id).map_err(|error| error.to_string())
}
