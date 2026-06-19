use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub use crate::model::DxWebLighthouseMode;
use crate::web_lighthouse::{
    DxWebLighthouseReport, build_failed_web_lighthouse_report, build_web_lighthouse_report,
    import_official_lighthouse_report,
};

const DEFAULT_TIMEOUT_SECONDS: u64 = 8;
const DEFAULT_LIGHTHOUSE_TIMEOUT_SECONDS: u64 = 120;
const DEFAULT_CAPTURE_BYTES: u64 = 512 * 1024;
const MAX_CAPTURE_BYTES: u64 = 2 * 1024 * 1024;
const OFFICIAL_LIGHTHOUSE_OUTPUT_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const OFFICIAL_LIGHTHOUSE_UNAVAILABLE_MODE: &str = "official-lighthouse-unavailable";
const OFFICIAL_LIGHTHOUSE_FALLBACK_SOURCE: &str = "official-lighthouse";
const DX_JS_LIGHTHOUSE_COMMAND_ARGS: &[&str] = &["js", "lighthouse"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxWebAuditRunnerRequest {
    pub id: String,
    pub url: String,
    pub required_status: Option<u16>,
    pub max_html_bytes: Option<u64>,
    pub timeout_seconds: u64,
    pub lighthouse_timeout_seconds: u64,
    pub lighthouse_mode: DxWebLighthouseMode,
    pub lighthouse_json: Option<String>,
    pub lighthouse_binary: Option<String>,
    pub lighthouse_repo: Option<PathBuf>,
    pub lighthouse_command: Option<DxWebLighthouseCommand>,
}

impl DxWebAuditRunnerRequest {
    pub fn new(id: String, url: String) -> Self {
        Self {
            id,
            url,
            required_status: None,
            max_html_bytes: None,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            lighthouse_timeout_seconds: DEFAULT_LIGHTHOUSE_TIMEOUT_SECONDS,
            lighthouse_mode: DxWebLighthouseMode::Official,
            lighthouse_json: None,
            lighthouse_binary: None,
            lighthouse_repo: None,
            lighthouse_command: None,
        }
    }

    fn capture_limit(&self) -> u64 {
        self.max_html_bytes
            .unwrap_or(DEFAULT_CAPTURE_BYTES)
            .saturating_add(1)
            .clamp(1, MAX_CAPTURE_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxWebLighthouseCommand {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxWebAuditRunnerOutput {
    pub schema_version: String,
    pub id: String,
    pub target_id: String,
    pub url: String,
    pub status: Option<u16>,
    pub final_url: Option<String>,
    pub response_time_ms: Option<u128>,
    pub html_bytes: Option<u64>,
    pub title_present: bool,
    pub description_present: bool,
    pub canonical_present: bool,
    pub viewport_present: bool,
    pub security_header_count: u16,
    pub headers: Vec<DxWebAuditHeader>,
    pub diagnostics: Vec<DxWebAuditRunnerDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lighthouse: Option<DxWebLighthouseReport>,
}

impl DxWebAuditRunnerOutput {
    pub fn has_failure(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == "failure")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxWebAuditHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxWebAuditRunnerDiagnostic {
    pub id: String,
    pub severity: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxWebAuditObservedResponse {
    pub status: u16,
    pub final_url: String,
    pub response_time_ms: u128,
    pub html_bytes: u64,
    pub headers: Vec<DxWebAuditHeader>,
    pub body: Vec<u8>,
    pub body_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxWebAuditHtmlMetadata {
    pub title_present: bool,
    pub description_present: bool,
    pub canonical_present: bool,
    pub viewport_present: bool,
}

pub fn run_web_audit(request: &DxWebAuditRunnerRequest) -> Result<DxWebAuditRunnerOutput, String> {
    validate_web_audit_runner_request(request)?;

    let started = Instant::now();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(request.timeout_seconds.max(1)))
        .user_agent("dx-check-web-audit/0.1")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| format!("web audit HTTP client could not be built: {error}"))?;

    let mut response = match client.get(&request.url).send() {
        Ok(response) => response,
        Err(error) => {
            return Ok(failed_output(
                request,
                Some(started.elapsed().as_millis()),
                "web-request-failed",
                format!("Web audit request failed: {error}"),
                "Start the target app, confirm the URL is reachable, then rerun dx check.",
            ));
        }
    };

    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let headers = response_headers(response.headers());
    let content_length = response.content_length();
    let mut body = Vec::new();
    let read_result = {
        let mut limited = response.by_ref().take(request.capture_limit());
        limited.read_to_end(&mut body)
    };
    if let Err(error) = read_result {
        return Ok(failed_output(
            request,
            Some(started.elapsed().as_millis()),
            "web-response-read-failed",
            format!("Web audit response body could not be read: {error}"),
            "Check the target app response stream, then rerun dx check.",
        ));
    }
    let body_truncated = body.len() as u64 >= request.capture_limit();
    let html_bytes = content_length.unwrap_or(body.len() as u64);

    Ok(build_web_audit_output_with_lighthouse(
        request,
        DxWebAuditObservedResponse {
            status,
            final_url,
            response_time_ms: started.elapsed().as_millis(),
            html_bytes,
            headers,
            body,
            body_truncated,
        },
    ))
}

pub fn build_web_audit_output(
    request: &DxWebAuditRunnerRequest,
    observed: DxWebAuditObservedResponse,
) -> DxWebAuditRunnerOutput {
    let mut native_request = request.clone();
    native_request.lighthouse_mode = DxWebLighthouseMode::Native;
    native_request.lighthouse_json = None;
    build_web_audit_output_with_lighthouse(&native_request, observed)
}

pub fn build_web_audit_output_with_lighthouse(
    request: &DxWebAuditRunnerRequest,
    observed: DxWebAuditObservedResponse,
) -> DxWebAuditRunnerOutput {
    let html = String::from_utf8_lossy(&observed.body);
    let metadata = inspect_html_document(&html);
    let security_header_count = security_header_count(&observed.headers);
    let mut diagnostics = Vec::new();

    if let Some(required_status) = request.required_status
        && observed.status != required_status
    {
        diagnostics.push(diagnostic(
            "web-http-status",
            "failure",
            format!(
                "Web audit target `{}` returned HTTP {}, expected {}",
                request.id, observed.status, required_status
            ),
            "Fix the route or configured expected status, then rerun dx check.",
        ));
    }

    if let Some(max_html_bytes) = request.max_html_bytes
        && observed.html_bytes > max_html_bytes
    {
        diagnostics.push(diagnostic(
            "web-html-budget",
            "warning",
            format!(
                "Web audit target `{}` returned {} HTML bytes, over the {} byte budget",
                request.id, observed.html_bytes, max_html_bytes
            ),
            "Reduce initial HTML size or raise the configured budget with evidence.",
        ));
    }

    if observed.body_truncated {
        diagnostics.push(diagnostic(
            "web-body-capture-truncated",
            "warning",
            format!(
                "Web audit target `{}` exceeded the bounded response capture limit",
                request.id
            ),
            "Use a smaller HTML response or review the page with a dedicated Lighthouse artifact.",
        ));
    }

    diagnostics.extend(metadata_diagnostics(&request.id, &metadata));
    diagnostics.extend(security_header_diagnostics(
        &request.id,
        &observed.headers,
        security_header_count,
    ));

    let lighthouse = lighthouse_report(request, &observed, &metadata, security_header_count, &html)
        .unwrap_or_else(|diagnostic| {
            let native_fallback = request.lighthouse_mode == DxWebLighthouseMode::Auto
                && diagnostic.severity == "warning";
            let mut report = if native_fallback {
                build_web_lighthouse_report(
                    request,
                    &observed,
                    &metadata,
                    security_header_count,
                    &html,
                )
            } else {
                build_official_lighthouse_failure_report(request, &diagnostic)
            };
            if native_fallback {
                report.fallback_from = Some(OFFICIAL_LIGHTHOUSE_FALLBACK_SOURCE.to_string());
            }
            diagnostics.push(diagnostic);
            report
        });

    DxWebAuditRunnerOutput {
        schema_version: "dx.check.web_audit.v1".to_string(),
        id: format!("{}-run", request.id),
        target_id: request.id.clone(),
        url: request.url.clone(),
        status: Some(observed.status),
        final_url: Some(observed.final_url),
        response_time_ms: Some(observed.response_time_ms),
        html_bytes: Some(observed.html_bytes),
        title_present: metadata.title_present,
        description_present: metadata.description_present,
        canonical_present: metadata.canonical_present,
        viewport_present: metadata.viewport_present,
        security_header_count,
        headers: observed.headers,
        diagnostics,
        lighthouse: Some(lighthouse),
    }
}

fn lighthouse_report(
    request: &DxWebAuditRunnerRequest,
    observed: &DxWebAuditObservedResponse,
    metadata: &DxWebAuditHtmlMetadata,
    security_header_count: u16,
    html: &str,
) -> Result<DxWebLighthouseReport, DxWebAuditRunnerDiagnostic> {
    match request.lighthouse_mode {
        DxWebLighthouseMode::Native => Ok(build_web_lighthouse_report(
            request,
            observed,
            metadata,
            security_header_count,
            html,
        )),
        DxWebLighthouseMode::Official => official_lighthouse_report(request).map_err(|error| {
            diagnostic(
                "web-lighthouse-official-unavailable",
                "failure",
                format!(
                    "Official Lighthouse could not produce a report: {}",
                    error.message()
                ),
                "Install the official Lighthouse toolchain or switch this web audit target to lighthouse=auto or lighthouse=native.",
            )
        }),
        DxWebLighthouseMode::Auto => official_lighthouse_report(request).map_err(|error| {
            match error {
                OfficialLighthouseError::Unavailable(message) => diagnostic(
                    "web-lighthouse-official-unavailable",
                    "warning",
                    format!(
                        "Official Lighthouse was unavailable, so DX used native Litehouse fallback: {message}"
                    ),
                    "Install the official Lighthouse toolchain for browser-based scores, or keep lighthouse=native for deterministic HTTP checks.",
                ),
                OfficialLighthouseError::InvalidEvidence(message) => diagnostic(
                    "web-lighthouse-official-unavailable",
                    "failure",
                    format!("Official Lighthouse evidence could not be accepted: {message}"),
                    "Fix the supplied Lighthouse JSON or official Lighthouse command output before accepting web metric evidence.",
                ),
            }
        }),
    }
}

fn official_lighthouse_report(
    request: &DxWebAuditRunnerRequest,
) -> Result<DxWebLighthouseReport, OfficialLighthouseError> {
    if let Some(json) = &request.lighthouse_json {
        let value = serde_json::from_str::<serde_json::Value>(json).map_err(|error| {
            OfficialLighthouseError::InvalidEvidence(format!(
                "official Lighthouse JSON could not be parsed: {error}"
            ))
        })?;
        return import_official_lighthouse_report_for_request(request, &value);
    }

    let output =
        run_official_lighthouse_command(request).map_err(OfficialLighthouseError::Unavailable)?;
    import_official_lighthouse_command_output(request, output)
}

fn import_official_lighthouse_command_output(
    request: &DxWebAuditRunnerRequest,
    output: OfficialLighthouseOutput,
) -> Result<DxWebLighthouseReport, OfficialLighthouseError> {
    if output.stdout_truncated {
        return Err(OfficialLighthouseError::InvalidEvidence(
            official_lighthouse_capture_limit_message("stdout"),
        ));
    }
    if output.stderr_truncated {
        return Err(OfficialLighthouseError::InvalidEvidence(
            official_lighthouse_capture_limit_message("stderr"),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = serde_json::from_str::<serde_json::Value>(&stdout).map_err(|error| {
        let stderr = captured_output_preview(&output.stderr, output.stderr_truncated, 320);
        OfficialLighthouseError::InvalidEvidence(format!(
            "official Lighthouse did not emit JSON on stdout: {error}; stderr: {stderr}"
        ))
    })?;
    import_official_lighthouse_report_for_request(request, &value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OfficialLighthouseError {
    Unavailable(String),
    InvalidEvidence(String),
}

impl OfficialLighthouseError {
    fn message(&self) -> &str {
        match self {
            Self::Unavailable(message) | Self::InvalidEvidence(message) => message,
        }
    }
}

fn import_official_lighthouse_report_for_request(
    request: &DxWebAuditRunnerRequest,
    value: &serde_json::Value,
) -> Result<DxWebLighthouseReport, OfficialLighthouseError> {
    let report = import_official_lighthouse_report(value, &request.id).map_err(|error| {
        OfficialLighthouseError::InvalidEvidence(format!(
            "official Lighthouse JSON could not be imported: {error}"
        ))
    })?;
    if !urls_match_requested_target(&request.url, &report.url) {
        return Err(OfficialLighthouseError::InvalidEvidence(format!(
            "official Lighthouse JSON URL `{}` does not match requested web audit URL `{}`",
            report.url, request.url
        )));
    }

    Ok(report)
}

fn urls_match_requested_target(expected: &str, actual: &str) -> bool {
    let expected = expected.trim();
    let actual = actual.trim();
    if expected == actual {
        return true;
    }

    let Ok(expected_url) = reqwest::Url::parse(expected) else {
        return false;
    };
    let Ok(actual_url) = reqwest::Url::parse(actual) else {
        return false;
    };

    expected_url
        .scheme()
        .eq_ignore_ascii_case(actual_url.scheme())
        && expected_url
            .host_str()
            .zip(actual_url.host_str())
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && expected_url.port_or_known_default() == actual_url.port_or_known_default()
        && expected_url.path() == actual_url.path()
        && expected_url.query() == actual_url.query()
}

fn build_official_lighthouse_failure_report(
    request: &DxWebAuditRunnerRequest,
    diagnostic: &DxWebAuditRunnerDiagnostic,
) -> DxWebLighthouseReport {
    let mut report = build_failed_web_lighthouse_report(
        request,
        &diagnostic.id,
        diagnostic.message.clone(),
        diagnostic.next_action.clone(),
    );
    report.mode = OFFICIAL_LIGHTHOUSE_UNAVAILABLE_MODE.to_string();
    report
}

fn official_lighthouse_command(request: &DxWebAuditRunnerRequest) -> Command {
    let mut command = if let Some(lighthouse_command) = &request.lighthouse_command {
        let mut command = Command::new(&lighthouse_command.executable);
        command.args(&lighthouse_command.args);
        if let Some(cwd) = &lighthouse_command.cwd {
            command.current_dir(cwd);
        }
        command
    } else if let Some(binary) = &request.lighthouse_binary {
        Command::new(binary)
    } else if let Some(repo) = &request.lighthouse_repo {
        let mut command = Command::new("node");
        command.arg(repo.join("cli").join("index.js"));
        command
    } else {
        Command::new("lighthouse")
    };
    command.args([
        request.url.as_str(),
        "--quiet",
        "--no-enable-error-reporting",
        "--chrome-flags=--headless",
        "--output=json",
        "--output-path=stdout",
        "--only-categories=performance,accessibility,best-practices,seo",
    ]);
    command
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OfficialLighthouseOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CapturedPipe {
    bytes: Vec<u8>,
    truncated: bool,
}

fn run_official_lighthouse_command(
    request: &DxWebAuditRunnerRequest,
) -> Result<OfficialLighthouseOutput, String> {
    if let Some(repo) = &request.lighthouse_repo {
        validate_official_lighthouse_repo(repo)?;
    }

    let timeout = Duration::from_secs(request.lighthouse_timeout_seconds.max(1));
    let mut command = official_lighthouse_command(request);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("official Lighthouse command could not start: {error}"))?;

    let stdout = child.stdout.take().map(read_child_pipe);
    let stderr = child.stderr.take().map(read_child_pipe);
    let started = Instant::now();

    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            format!("official Lighthouse command status could not be read: {error}")
        })? {
            let stdout = collect_child_pipe(stdout);
            let stderr = collect_child_pipe(stderr);
            if !status.success() {
                let stderr = captured_output_preview(&stderr.bytes, stderr.truncated, 320);
                return Err(format!(
                    "official Lighthouse exited with status {status}; stderr: {stderr}"
                ));
            }
            return Ok(OfficialLighthouseOutput {
                stdout: stdout.bytes,
                stderr: stderr.bytes,
                stdout_truncated: stdout.truncated,
                stderr_truncated: stderr.truncated,
            });
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = collect_child_pipe(stdout);
            let stderr = collect_child_pipe(stderr);
            let stderr = captured_output_preview(&stderr.bytes, stderr.truncated, 320);
            return Err(format!(
                "official Lighthouse timed out after {} seconds; stderr: {stderr}",
                request.lighthouse_timeout_seconds.max(1)
            ));
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn validate_official_lighthouse_repo(repo: &Path) -> Result<(), String> {
    let cli = repo.join("cli").join("index.js");
    if !cli.is_file() {
        return Err(format!(
            "official Lighthouse repo is missing CLI entrypoint at {}",
            cli.display()
        ));
    }

    if !repo.join("node_modules").is_dir() {
        return Err(format!(
            "official Lighthouse repo at {} is not bootstrapped; run yarn in that repo or pass --lighthouse-bin",
            repo.display()
        ));
    }

    Ok(())
}

fn read_child_pipe<R>(pipe: R) -> JoinHandle<CapturedPipe>
where
    R: Read + Send + 'static,
{
    read_child_pipe_limited(pipe, OFFICIAL_LIGHTHOUSE_OUTPUT_CAPTURE_BYTES)
}

fn read_child_pipe_limited<R>(mut pipe: R, limit: usize) -> JoinHandle<CapturedPipe>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = pipe
            .by_ref()
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut output);
        let truncated = output.len() > limit;
        if truncated {
            output.truncate(limit);
        }
        CapturedPipe {
            bytes: output,
            truncated,
        }
    })
}

fn collect_child_pipe(handle: Option<JoinHandle<CapturedPipe>>) -> CapturedPipe {
    handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

fn official_lighthouse_capture_limit_message(stream_name: &str) -> String {
    format!(
        "official Lighthouse {stream_name} exceeded the {} byte capture limit; output was rejected to avoid accepting partial Lighthouse evidence",
        OFFICIAL_LIGHTHOUSE_OUTPUT_CAPTURE_BYTES
    )
}

fn captured_output_preview(bytes: &[u8], truncated: bool, max_chars: usize) -> String {
    let mut preview = truncate(&String::from_utf8_lossy(bytes), max_chars);
    if truncated {
        if !preview.is_empty() {
            preview.push_str(" [truncated at capture limit]");
        } else {
            preview.push_str("[truncated at capture limit]");
        }
    }
    preview
}

pub fn inspect_html_document(html: &str) -> DxWebAuditHtmlMetadata {
    let metadata = dx_litehouse::inspect_html_metadata(html);
    DxWebAuditHtmlMetadata {
        title_present: metadata.title_present,
        description_present: metadata.description_present,
        canonical_present: metadata.canonical_present,
        viewport_present: metadata.viewport_present,
    }
}

fn failed_output(
    request: &DxWebAuditRunnerRequest,
    response_time_ms: Option<u128>,
    id: &str,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxWebAuditRunnerOutput {
    let message = message.into();
    let next_action = next_action.into();
    DxWebAuditRunnerOutput {
        schema_version: "dx.check.web_audit.v1".to_string(),
        id: format!("{}-run", request.id),
        target_id: request.id.clone(),
        url: request.url.clone(),
        status: None,
        final_url: None,
        response_time_ms,
        html_bytes: None,
        title_present: false,
        description_present: false,
        canonical_present: false,
        viewport_present: false,
        security_header_count: 0,
        headers: Vec::new(),
        diagnostics: vec![diagnostic(
            id,
            "failure",
            message.clone(),
            next_action.clone(),
        )],
        lighthouse: Some(build_failed_web_lighthouse_report(
            request,
            id,
            message,
            next_action,
        )),
    }
}

pub fn validate_web_audit_runner_request(request: &DxWebAuditRunnerRequest) -> Result<(), String> {
    if request.id.trim().is_empty() {
        return Err("web audit target id is required".to_string());
    }
    if !(request.url.starts_with("http://") || request.url.starts_with("https://")) {
        return Err("web audit URL must start with http:// or https://".to_string());
    }
    if request.url.chars().any(char::is_control) {
        return Err("web audit URL cannot contain control characters".to_string());
    }
    validate_lighthouse_sources(request)?;
    Ok(())
}

fn validate_lighthouse_sources(request: &DxWebAuditRunnerRequest) -> Result<(), String> {
    let source_count = [
        request.lighthouse_json.is_some(),
        request.lighthouse_binary.is_some(),
        request.lighthouse_repo.is_some(),
        request.lighthouse_command.is_some(),
    ]
    .into_iter()
    .filter(|source| *source)
    .count();

    if source_count > 1 {
        return Err(
            "choose only one Lighthouse source: --lighthouse-json, --lighthouse-bin, --lighthouse-repo, or --lighthouse-command"
                .to_string(),
        );
    }
    if source_count > 0 && request.lighthouse_mode == DxWebLighthouseMode::Native {
        return Err(
            "native Lighthouse mode cannot use an official Lighthouse source flag".to_string(),
        );
    }

    if let Some(command) = &request.lighthouse_command
        && !lighthouse_command_args_are_canonical(&command.args)
    {
        return Err(format!(
            "DX JS Lighthouse command args must be exactly `js lighthouse`, got `{}`",
            format_lighthouse_command_args(&command.args)
        ));
    }

    Ok(())
}

fn lighthouse_command_args_are_canonical(args: &[String]) -> bool {
    args.len() == DX_JS_LIGHTHOUSE_COMMAND_ARGS.len()
        && args
            .iter()
            .map(String::as_str)
            .eq(DX_JS_LIGHTHOUSE_COMMAND_ARGS.iter().copied())
}

fn format_lighthouse_command_args(args: &[String]) -> String {
    if args.is_empty() {
        "<empty>".to_string()
    } else {
        args.join(" ")
    }
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> Vec<DxWebAuditHeader> {
    headers
        .iter()
        .take(64)
        .filter_map(|(name, value)| {
            let value = value.to_str().ok()?;
            Some(DxWebAuditHeader {
                name: name.as_str().to_ascii_lowercase(),
                value: truncate(value, 240),
            })
        })
        .collect()
}

fn metadata_diagnostics(
    target_id: &str,
    metadata: &DxWebAuditHtmlMetadata,
) -> Vec<DxWebAuditRunnerDiagnostic> {
    let mut diagnostics = Vec::new();
    if !metadata.title_present {
        diagnostics.push(diagnostic(
            "web-title-missing",
            "warning",
            format!("Web audit target `{target_id}` is missing a non-empty <title>"),
            "Add a descriptive title to the initial HTML document.",
        ));
    }
    if !metadata.description_present {
        diagnostics.push(diagnostic(
            "web-description-missing",
            "warning",
            format!("Web audit target `{target_id}` is missing a meta description"),
            "Add a concise meta description to the initial HTML document.",
        ));
    }
    if !metadata.viewport_present {
        diagnostics.push(diagnostic(
            "web-viewport-missing",
            "warning",
            format!("Web audit target `{target_id}` is missing a viewport meta tag"),
            "Add a viewport meta tag so mobile audits can reason about layout.",
        ));
    }
    diagnostics
}

fn security_header_diagnostics(
    target_id: &str,
    headers: &[DxWebAuditHeader],
    security_header_count: u16,
) -> Vec<DxWebAuditRunnerDiagnostic> {
    if security_header_count >= 2 {
        return Vec::new();
    }

    let names = headers
        .iter()
        .map(|header| header.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    let missing = [
        "content-security-policy",
        "x-content-type-options",
        "referrer-policy",
    ]
    .into_iter()
    .filter(|name| !names.contains(name))
    .collect::<Vec<_>>()
    .join(", ");

    vec![diagnostic(
        "web-security-headers-low",
        "warning",
        format!(
            "Web audit target `{target_id}` has only {security_header_count} common security headers"
        ),
        format!("Review missing security headers: {missing}."),
    )]
}

fn security_header_count(headers: &[DxWebAuditHeader]) -> u16 {
    headers
        .iter()
        .filter(|header| {
            matches!(
                header.name.as_str(),
                "content-security-policy"
                    | "strict-transport-security"
                    | "x-content-type-options"
                    | "referrer-policy"
                    | "permissions-policy"
                    | "cross-origin-opener-policy"
                    | "cross-origin-resource-policy"
            )
        })
        .count()
        .min(u16::MAX as usize) as u16
}

fn diagnostic(
    id: &str,
    severity: &str,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxWebAuditRunnerDiagnostic {
    DxWebAuditRunnerDiagnostic {
        id: id.to_string(),
        severity: severity.to_string(),
        message: message.into(),
        next_action: next_action.into(),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        DxWebAuditRunnerRequest, OfficialLighthouseError, OfficialLighthouseOutput,
        import_official_lighthouse_command_output, read_child_pipe_limited,
    };

    #[test]
    fn child_pipe_capture_reports_truncation_past_limit() {
        let handle = read_child_pipe_limited(Cursor::new(b"abcdef".to_vec()), 3);
        let captured = handle.join().expect("capture thread joins");

        assert_eq!(captured.bytes, b"abc");
        assert!(captured.truncated);
    }

    #[test]
    fn child_pipe_capture_keeps_exact_limit_without_truncation() {
        let handle = read_child_pipe_limited(Cursor::new(b"abc".to_vec()), 3);
        let captured = handle.join().expect("capture thread joins");

        assert_eq!(captured.bytes, b"abc");
        assert!(!captured.truncated);
    }

    #[test]
    fn official_lighthouse_output_rejects_truncated_stdout() {
        let request = DxWebAuditRunnerRequest::new(
            "landing-page".to_string(),
            "https://example.com".to_string(),
        );

        let error = import_official_lighthouse_command_output(
            &request,
            OfficialLighthouseOutput {
                stdout: b"{".to_vec(),
                stderr: Vec::new(),
                stdout_truncated: true,
                stderr_truncated: false,
            },
        )
        .expect_err("truncated stdout must be rejected");

        assert_invalid_evidence_contains(error, "stdout exceeded");
    }

    #[test]
    fn official_lighthouse_output_rejects_truncated_stderr() {
        let request = DxWebAuditRunnerRequest::new(
            "landing-page".to_string(),
            "https://example.com".to_string(),
        );

        let error = import_official_lighthouse_command_output(
            &request,
            OfficialLighthouseOutput {
                stdout: b"{}".to_vec(),
                stderr: b"diagnostic".to_vec(),
                stdout_truncated: false,
                stderr_truncated: true,
            },
        )
        .expect_err("truncated stderr must be rejected");

        assert_invalid_evidence_contains(error, "stderr exceeded");
    }

    fn assert_invalid_evidence_contains(error: OfficialLighthouseError, expected: &str) {
        let OfficialLighthouseError::InvalidEvidence(message) = error else {
            panic!("expected invalid evidence error");
        };
        assert!(message.contains(expected), "{message}");
    }
}
