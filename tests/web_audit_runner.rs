use dx_check_engine::web_audit_runner::{
    DxWebAuditHeader, DxWebAuditObservedResponse, DxWebAuditRunnerRequest, DxWebLighthouseCommand,
    DxWebLighthouseMode, build_web_audit_output, build_web_audit_output_with_lighthouse,
    inspect_html_document, run_web_audit,
};
use std::path::Path;

fn native_request(id: &str, url: &str) -> DxWebAuditRunnerRequest {
    let mut request = DxWebAuditRunnerRequest::new(id.to_string(), url.to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Native;
    request
}

#[test]
fn web_audit_runner_request_defaults_to_official_lighthouse() {
    let request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());

    assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
}

#[test]
fn web_audit_runner_rejects_noncanonical_lighthouse_command_args() {
    for args in [
        Vec::<String>::new(),
        vec!["lighthouse".to_string(), "js".to_string()],
        vec![
            "js".to_string(),
            "lighthouse".to_string(),
            "--contract".to_string(),
        ],
    ] {
        let mut request =
            DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
        request.lighthouse_command = Some(DxWebLighthouseCommand {
            executable: "G:\\Dx\\bin\\dx.exe".to_string(),
            args,
            cwd: None,
        });

        let error = run_web_audit(&request)
            .expect_err("noncanonical Lighthouse command args must fail before network access");

        assert!(error.contains("must be exactly `js lighthouse`"), "{error}");
    }
}

#[test]
fn web_audit_runner_rejects_ambiguous_lighthouse_sources() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_binary = Some("lighthouse".to_string());
    request.lighthouse_repo = Some(std::path::PathBuf::from("third_party/google-lighthouse"));

    let error = run_web_audit(&request)
        .expect_err("ambiguous Lighthouse sources must fail before network access");

    assert!(
        error.contains("choose only one Lighthouse source"),
        "{error}"
    );
}

#[test]
fn web_audit_runner_rejects_native_mode_with_lighthouse_source() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Native;
    request.lighthouse_binary = Some("lighthouse".to_string());

    let error = run_web_audit(&request)
        .expect_err("native Lighthouse mode with official source must fail before network access");

    assert!(error.contains("official Lighthouse source"), "{error}");
}

#[test]
fn detects_html_metadata_for_web_audit_results() {
    let metadata = inspect_html_document(
        r#"
<!doctype html>
<html>
  <head>
    <title>DX</title>
    <meta name="description" content="Developer experience">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="canonical" href="https://example.com/">
  </head>
</html>
"#,
    );

    assert!(metadata.title_present);
    assert!(metadata.description_present);
    assert!(metadata.viewport_present);
    assert!(metadata.canonical_present);
}

#[test]
fn builds_runner_output_with_status_budget_and_header_diagnostics() {
    let mut request = native_request("home", "https://example.com/");
    request.required_status = Some(200);
    request.max_html_bytes = Some(100);

    let output = build_web_audit_output(
        &request,
        DxWebAuditObservedResponse {
            status: 404,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 45,
            html_bytes: 250,
            headers: vec![DxWebAuditHeader {
                name: "content-type".to_string(),
                value: "text/html".to_string(),
            }],
            body: b"<html><head><title></title></head></html>".to_vec(),
            body_truncated: false,
        },
    );

    assert_eq!(output.target_id, "home");
    assert_eq!(output.status, Some(404));
    assert_eq!(output.html_bytes, Some(250));
    assert!(!output.title_present);
    assert_eq!(output.security_header_count, 0);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-http-status" && diagnostic.severity == "failure"
    }));
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "web-html-budget")
    );
    assert!(output.has_failure());
}

#[test]
fn builds_lighthouse_report_for_accessible_secure_html() {
    let mut request = native_request("home", "https://example.com/");
    request.required_status = Some(200);
    request.max_html_bytes = Some(64_000);

    let output = build_web_audit_output(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 180,
            html_bytes: 18_000,
            headers: vec![
                DxWebAuditHeader {
                    name: "content-security-policy".to_string(),
                    value: "default-src 'self'".to_string(),
                },
                DxWebAuditHeader {
                    name: "strict-transport-security".to_string(),
                    value: "max-age=31536000".to_string(),
                },
                DxWebAuditHeader {
                    name: "x-content-type-options".to_string(),
                    value: "nosniff".to_string(),
                },
                DxWebAuditHeader {
                    name: "referrer-policy".to_string(),
                    value: "strict-origin-when-cross-origin".to_string(),
                },
            ],
            body: br#"<!doctype html>
<html lang="en">
  <head>
    <title>DX Check</title>
    <meta name="description" content="Production project checks">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="canonical" href="https://example.com/">
  </head>
  <body>
    <main>
      <h1>DX Check</h1>
      <img src="/logo.png" alt="DX">
      <button aria-label="Run check"></button>
      <form><label for="email">Email</label><input id="email" type="email"></form>
    </main>
  </body>
</html>"#
                .to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.as_ref().expect("lighthouse report");

    assert_eq!(report.schema_version, "dx.check.web_lighthouse");
    assert_eq!(report.score, 400);
    assert_eq!(report.max_score, 400);
    assert!(
        report
            .categories
            .iter()
            .all(|category| category.score == 100)
    );
    assert!(
        report
            .audits
            .iter()
            .any(|audit| { audit.id == "accessibility-image-alt" && audit.status == "ready" })
    );
}

#[test]
fn lighthouse_report_flags_accessibility_and_best_practice_gaps() {
    let request = native_request("home", "http://example.com/");

    let output = build_web_audit_output(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "http://example.com/".to_string(),
            response_time_ms: 1_450,
            html_bytes: 900_000,
            headers: vec![DxWebAuditHeader {
                name: "content-type".to_string(),
                value: "text/html".to_string(),
            }],
            body: br#"<!doctype html>
<html>
  <head><title></title></head>
  <body>
    <img src="hero.png">
    <button></button>
    <form><input type="email"></form>
    <script src="http://cdn.example.com/app.js"></script>
  </body>
</html>"#
                .to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.as_ref().expect("lighthouse report");
    let accessibility = report
        .categories
        .iter()
        .find(|category| category.id == "accessibility")
        .expect("accessibility category");
    let best_practices = report
        .categories
        .iter()
        .find(|category| category.id == "best-practices")
        .expect("best-practices category");

    assert!(accessibility.score < 100);
    assert_eq!(accessibility.status, "warning");
    assert!(best_practices.score < 100);
    assert!(
        report
            .audits
            .iter()
            .any(|audit| { audit.id == "accessibility-image-alt" && audit.status == "warning" })
    );
    assert!(
        report
            .audits
            .iter()
            .any(|audit| { audit.id == "best-practices-https" && audit.status == "warning" })
    );
}

#[test]
fn web_audit_runner_output_serializes_litehouse_contract() {
    let request = native_request("home", "https://example.com/");

    let output = build_web_audit_output(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: vec![DxWebAuditHeader {
                name: "x-content-type-options".to_string(),
                value: "nosniff".to_string(),
            }],
            body: br#"<!doctype html><html lang="en"><head><title>DX</title><meta name="description" content="DX"><meta name="viewport" content="width=device-width"><link rel="canonical" href="https://example.com/"></head><body><main><h1>DX</h1></main></body></html>"#.to_vec(),
            body_truncated: false,
        },
    );

    let value = serde_json::to_value(&output).expect("serialized web audit output");
    let lighthouse = value
        .get("lighthouse")
        .expect("lighthouse report")
        .as_object()
        .expect("lighthouse object");

    assert_eq!(
        lighthouse.get("engine").and_then(|value| value.as_str()),
        Some("dx-litehouse")
    );
    assert_eq!(
        lighthouse.get("mode").and_then(|value| value.as_str()),
        Some("native-http-html")
    );
    assert!(lighthouse.get("artifact").is_some());
    assert!(
        lighthouse
            .get("categories")
            .and_then(|value| value.as_array())
            .is_some()
    );
    assert!(
        lighthouse
            .get("audits")
            .and_then(|value| value.as_array())
            .is_some()
    );
}

#[test]
fn web_audit_runner_imports_official_lighthouse_json() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Official;
    request.lighthouse_json = Some(official_lighthouse_json("https://example.com/"));

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("official lighthouse report");

    assert_eq!(report.engine, "dx-litehouse");
    assert_eq!(report.mode, "lighthouse-json-import");
    assert_eq!(report.score, 91);
    assert_eq!(report.max_score, 100);
    assert!(output.diagnostics.iter().all(|diagnostic| {
        diagnostic.id != "web-lighthouse-official-unavailable"
            && diagnostic.id != "web-lighthouse-import-failed"
    }));
}

#[test]
fn web_audit_runner_rejects_empty_official_lighthouse_json_shape() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Official;
    request.lighthouse_json = Some(
        serde_json::json!({
            "lighthouseVersion": "13.3.0",
            "requestedUrl": "https://example.com/",
            "categories": {},
            "audits": {}
        })
        .to_string(),
    );

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output
        .lighthouse
        .expect("invalid official Lighthouse JSON failure report");

    assert_eq!(report.mode, "official-lighthouse-unavailable");
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable"
            && diagnostic.severity == "failure"
            && diagnostic.message.contains("categories")
    }));
}

#[test]
fn web_audit_runner_rejects_official_lighthouse_json_for_different_url() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Official;
    request.lighthouse_json = Some(official_lighthouse_json("https://other.example/"));

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output
        .lighthouse
        .expect("explicit official mismatch failure report");

    assert_eq!(report.mode, "official-lighthouse-unavailable");
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable"
            && diagnostic.severity == "failure"
            && diagnostic.message.contains("does not match")
    }));
}

#[test]
fn web_audit_runner_auto_mode_rejects_mismatched_official_lighthouse_json_without_native_fallback()
{
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Auto;
    request.lighthouse_json = Some(official_lighthouse_json("https://other.example/"));

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output
        .lighthouse
        .expect("invalid official evidence failure report");

    assert_eq!(report.mode, "official-lighthouse-unavailable");
    assert_eq!(report.fallback_from, None);
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable"
            && diagnostic.severity == "failure"
            && diagnostic.message.contains("does not match")
    }));
}

#[test]
fn web_audit_runner_falls_back_when_official_lighthouse_is_unavailable_in_auto_mode() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Auto;
    request.lighthouse_binary = Some("definitely-missing-lighthouse-binary".to_string());

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("native fallback report");

    assert_eq!(report.mode, "native-http-html");
    assert_eq!(report.fallback_from.as_deref(), Some("official-lighthouse"));
    let value = serde_json::to_value(&report).expect("serialized fallback report");
    assert_eq!(
        value.get("fallback_from").and_then(|value| value.as_str()),
        Some("official-lighthouse")
    );
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable" && diagnostic.severity == "warning"
    }));
}

#[test]
fn web_audit_runner_native_lighthouse_reports_do_not_emit_fallback_marker() {
    let request = native_request("home", "https://example.com/");

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("native report");

    assert_eq!(report.mode, "native-http-html");
    assert_eq!(report.fallback_from, None);
    let value = serde_json::to_value(&report).expect("serialized native report");
    assert!(value.get("fallback_from").is_none());
}

#[test]
fn web_audit_runner_does_not_native_fallback_when_official_lighthouse_is_required() {
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Official;
    request.lighthouse_binary = Some("definitely-missing-lighthouse-binary".to_string());

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("explicit official failure report");

    assert_eq!(report.mode, "official-lighthouse-unavailable");
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable" && diagnostic.severity == "failure"
    }));
}

#[test]
fn web_audit_runner_treats_nonzero_official_lighthouse_status_as_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let lighthouse_binary = write_nonzero_lighthouse_command(temp.path());
    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Official;
    request.lighthouse_binary = Some(lighthouse_binary.display().to_string());

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("nonzero official failure report");

    assert_eq!(report.mode, "official-lighthouse-unavailable");
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable"
            && diagnostic.severity == "failure"
            && diagnostic.message.contains("exited with status")
    }));
}

#[test]
fn web_audit_runner_reports_unbootstrapped_official_lighthouse_repo() {
    let temp = tempfile::tempdir().expect("tempdir");
    let lighthouse_repo = temp.path().join("google-lighthouse");
    std::fs::create_dir_all(lighthouse_repo.join("cli")).expect("created cli dir");
    std::fs::write(lighthouse_repo.join("cli").join("index.js"), "").expect("created cli");

    let mut request =
        DxWebAuditRunnerRequest::new("home".to_string(), "https://example.com/".to_string());
    request.lighthouse_mode = DxWebLighthouseMode::Auto;
    request.lighthouse_repo = Some(lighthouse_repo);

    let output = build_web_audit_output_with_lighthouse(
        &request,
        DxWebAuditObservedResponse {
            status: 200,
            final_url: "https://example.com/".to_string(),
            response_time_ms: 120,
            html_bytes: 512,
            headers: Vec::new(),
            body: b"<html><head><title>DX</title></head><body></body></html>".to_vec(),
            body_truncated: false,
        },
    );

    let report = output.lighthouse.expect("native fallback report");

    assert_eq!(report.mode, "native-http-html");
    assert!(output.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-official-unavailable"
            && diagnostic.severity == "warning"
            && diagnostic.message.contains("not bootstrapped")
    }));
}

fn official_lighthouse_json(requested_url: &str) -> String {
    serde_json::json!({
        "lighthouseVersion": "13.3.0",
        "requestedUrl": requested_url,
        "finalDisplayedUrl": requested_url,
        "categories": {
            "performance": {
                "id": "performance",
                "title": "Performance",
                "score": 0.91,
                "auditRefs": [{ "id": "first-contentful-paint", "weight": 10 }]
            }
        },
        "audits": {
            "first-contentful-paint": {
                "id": "first-contentful-paint",
                "title": "First Contentful Paint",
                "score": 0.77,
                "displayValue": "1.4 s",
                "description": "Marks the first text or image paint."
            }
        }
    })
    .to_string()
}

#[cfg(windows)]
fn write_nonzero_lighthouse_command(root: &Path) -> std::path::PathBuf {
    let path = root.join("fake-lighthouse.cmd");
    std::fs::write(
        &path,
        format!(
            "@echo off\r\necho {}\r\nexit /b 7\r\n",
            official_lighthouse_json("https://example.com/")
        ),
    )
    .expect("write fake lighthouse command");
    path
}

#[cfg(not(windows))]
fn write_nonzero_lighthouse_command(root: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join("fake-lighthouse");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\nexit 7\n",
            official_lighthouse_json("https://example.com/")
        ),
    )
    .expect("write fake lighthouse command");
    let mut permissions = std::fs::metadata(&path)
        .expect("fake lighthouse metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("make fake lighthouse executable");
    path
}
