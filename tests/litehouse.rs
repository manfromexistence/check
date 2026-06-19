use dx_litehouse::{
    LitehouseHeader, LitehousePageArtifact, LitehouseRunner, import_lighthouse_result,
};
use serde_json::Value;
use serde_json::json;

#[test]
fn dx_check_engine_reexports_litehouse_api() {
    assert_eq!(dx_check_engine::litehouse::LITEHOUSE_ENGINE, "dx-litehouse");
}

#[test]
fn native_litehouse_runner_builds_report_from_page_artifact() {
    let html = r#"<!doctype html>
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
</html>"#;

    let report = LitehouseRunner.audit(&LitehousePageArtifact {
        target_id: "home".to_string(),
        url: "https://example.com/".to_string(),
        final_url: Some("https://example.com/".to_string()),
        status: Some(200),
        response_time_ms: Some(220),
        html_bytes: Some(html.len() as u64),
        body_truncated: false,
        required_status: Some(200),
        max_html_bytes: Some(64_000),
        security_header_count: 4,
        headers: vec![
            LitehouseHeader {
                name: "content-security-policy".to_string(),
                value: "default-src 'self'".to_string(),
            },
            LitehouseHeader {
                name: "x-content-type-options".to_string(),
                value: "nosniff".to_string(),
            },
        ],
        html: html.to_string(),
    });

    assert_eq!(report.schema_version, "dx.check.web_lighthouse");
    assert_eq!(report.engine, "dx-litehouse");
    assert_eq!(report.mode, "native-http-html");
    assert_eq!(report.fallback_from, None);
    assert_eq!(report.id, "home-lighthouse");
    assert_eq!(report.score, 400);
    assert_eq!(report.max_score, 400);
    assert_eq!(report.artifact.status, Some(200));
    assert_eq!(report.artifact.header_count, 2);
    assert!(
        report
            .audits
            .iter()
            .any(|audit| audit.id == "accessibility-image-alt" && audit.status == "ready")
    );
}

#[test]
fn native_litehouse_runner_reports_bounded_failed_documents() {
    let report = LitehouseRunner.failed_report(
        "home",
        "http://localhost:3000/",
        "web-request-failed",
        "Web audit request failed: connection refused",
        "Start the target app, confirm the URL is reachable, then rerun dx check.",
    );

    assert_eq!(report.engine, "dx-litehouse");
    assert_eq!(report.mode, "native-http-html");
    assert_eq!(report.fallback_from, None);
    assert_eq!(report.score, 0);
    assert_eq!(report.max_score, 400);
    assert_eq!(report.categories.len(), 4);
    assert_eq!(report.audits.len(), 4);
    assert!(
        report
            .audits
            .iter()
            .any(|audit| audit.id == "web-request-failed" && audit.status == "warning")
    );
}

#[test]
fn imports_official_lighthouse_result_shape_into_dx_litehouse_report() {
    let report = import_lighthouse_result(
        &json!({
            "lighthouseVersion": "12.8.0",
            "requestedUrl": "https://example.com/",
            "finalDisplayedUrl": "https://example.com/",
            "categories": {
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": [
                        { "id": "first-contentful-paint", "weight": 10 }
                    ]
                },
                "accessibility": {
                    "id": "accessibility",
                    "title": "Accessibility",
                    "score": 1.0,
                    "auditRefs": [
                        { "id": "image-alt", "weight": 1 }
                    ]
                }
            },
            "audits": {
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74,
                    "displayValue": "1.9 s"
                },
                "image-alt": {
                    "id": "image-alt",
                    "title": "Image elements have alt attributes",
                    "description": "Informative images should have short alternate text.",
                    "score": 1
                }
            }
        }),
        "home",
    )
    .expect("imported Lighthouse result");

    assert_eq!(report.engine, "dx-litehouse");
    assert_eq!(report.mode, "lighthouse-json-import");
    assert_eq!(report.fallback_from, None);
    assert_eq!(report.target_id, "home");
    assert_eq!(report.url, "https://example.com/");
    assert_eq!(report.score, 182);
    assert_eq!(report.max_score, 200);
    assert!(
        report
            .categories
            .iter()
            .any(|category| category.id == "performance" && category.score == 82)
    );
    assert!(report.audits.iter().any(|audit| {
        audit.id == "first-contentful-paint"
            && audit.category == "performance"
            && audit.score == 74
            && audit.status == "warning"
    }));
}

#[test]
fn rejects_official_lighthouse_result_without_categories() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({}),
            json!({
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74
                }
            }),
        ),
        "home",
    )
    .expect_err("empty official Lighthouse result must fail");

    assert!(
        error.to_string().contains("categories"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_result_without_audits() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": [{ "id": "first-contentful-paint", "weight": 10 }]
                }
            }),
            json!({}),
        ),
        "home",
    )
    .expect_err("empty official Lighthouse audits must fail");

    assert!(
        error.to_string().contains("audits"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_category_without_audit_refs() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82
                }
            }),
            json!({
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74
                }
            }),
        ),
        "home",
    )
    .expect_err("category without auditRefs must fail");

    assert!(
        error.to_string().contains("auditRefs"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_category_with_empty_audit_refs() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": []
                }
            }),
            json!({
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74
                }
            }),
        ),
        "home",
    )
    .expect_err("category with empty auditRefs must fail");

    assert!(
        error.to_string().contains("auditRefs"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_category_with_audit_refs_without_ids() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": [{ "weight": 10 }]
                }
            }),
            json!({
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74
                }
            }),
        ),
        "home",
    )
    .expect_err("category with auditRefs missing ids must fail");

    assert!(
        error.to_string().contains("auditRefs"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_result_without_referenced_audits() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": [{ "id": "first-contentful-paint", "weight": 10 }]
                }
            }),
            json!({
                "unused-audit": {
                    "id": "unused-audit",
                    "title": "Unused Audit",
                    "description": "This audit is not referenced by any category.",
                    "score": 1
                }
            }),
        ),
        "home",
    )
    .expect_err("unreferenced official Lighthouse audits must fail");

    assert!(
        error.to_string().contains("first-contentful-paint"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_result_with_missing_referenced_audit() {
    let error = import_lighthouse_result(
        &official_lighthouse_result(
            json!({
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0.82,
                    "auditRefs": [
                        { "id": "first-contentful-paint", "weight": 10 },
                        { "id": "largest-contentful-paint", "weight": 25 }
                    ]
                }
            }),
            json!({
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0.74
                }
            }),
        ),
        "home",
    )
    .expect_err("missing referenced official Lighthouse audits must fail");

    assert!(
        error.to_string().contains("largest-contentful-paint"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_official_lighthouse_result_with_runtime_error() {
    let error = import_lighthouse_result(
        &json!({
            "lighthouseVersion": "12.8.0",
            "requestedUrl": "https://example.com/",
            "runtimeError": {
                "code": "NO_FCP",
                "message": "The page did not paint any content."
            },
            "categories": {
                "performance": {
                    "id": "performance",
                    "title": "Performance",
                    "score": 0,
                    "auditRefs": [{ "id": "first-contentful-paint", "weight": 10 }]
                }
            },
            "audits": {
                "first-contentful-paint": {
                    "id": "first-contentful-paint",
                    "title": "First Contentful Paint",
                    "description": "Marks the time at which the first text or image is painted.",
                    "score": 0
                }
            }
        }),
        "home",
    )
    .expect_err("official Lighthouse runtime errors must fail import");

    assert!(
        error.to_string().contains("runtimeError"),
        "unexpected error: {error}"
    );
}

fn official_lighthouse_result(categories: Value, audits: Value) -> Value {
    json!({
        "lighthouseVersion": "12.8.0",
        "requestedUrl": "https://example.com/",
        "categories": categories,
        "audits": audits
    })
}
