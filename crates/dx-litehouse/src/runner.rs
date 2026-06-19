use std::collections::HashSet;

use crate::html::{HtmlMetadata, HtmlSignals, inspect_html_metadata, inspect_html_signals};
use crate::model::{
    LITEHOUSE_ENGINE, LITEHOUSE_SCHEMA_VERSION, LitehouseAudit, LitehouseCategory,
    LitehousePageArtifact, LitehouseReport,
};

const CATEGORY_MAX_SCORE: u16 = 100;
const NATIVE_MODE: &str = "native-http-html";

#[derive(Debug, Clone, Default)]
pub struct LitehouseRunner;

impl LitehouseRunner {
    pub fn audit(&self, artifact: &LitehousePageArtifact) -> LitehouseReport {
        let metadata = inspect_html_metadata(&artifact.html);
        let signals = inspect_html_signals(&artifact.html);
        let mut audits = Vec::new();
        audits.extend(performance_audits(artifact));
        audits.extend(seo_audits(&metadata, &signals));
        audits.extend(accessibility_audits(&signals));
        audits.extend(best_practice_audits(artifact, &signals));

        report_from_audits(
            NATIVE_MODE,
            &format!("{}-lighthouse", artifact.target_id),
            &artifact.target_id,
            &artifact.url,
            artifact.final_url.clone(),
            artifact.summary(),
            audits,
        )
    }

    pub fn failed_report(
        &self,
        target_id: &str,
        url: &str,
        audit_id: &str,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> LitehouseReport {
        let detail = message.into();
        let next_action = next_action.into();
        let audits = vec![
            audit(
                "performance",
                audit_id,
                "Request completed",
                0,
                detail,
                next_action,
            ),
            audit(
                "accessibility",
                "accessibility-document-unavailable",
                "Document available",
                0,
                "The page HTML was not available for accessibility checks.",
                "Restore the web audit target before collecting accessibility evidence.",
            ),
            audit(
                "seo",
                "seo-document-unavailable",
                "Document available",
                0,
                "The page HTML was not available for SEO checks.",
                "Restore the web audit target before collecting SEO evidence.",
            ),
            audit(
                "best-practices",
                "best-practices-document-unavailable",
                "Document available",
                0,
                "The page HTML was not available for best-practice checks.",
                "Restore the web audit target before collecting best-practice evidence.",
            ),
        ];

        report_from_audits(
            NATIVE_MODE,
            &format!("{target_id}-lighthouse"),
            target_id,
            url,
            None,
            Default::default(),
            audits,
        )
    }
}

pub(crate) fn report_from_audits(
    mode: &str,
    id: &str,
    target_id: &str,
    url: &str,
    final_url: Option<String>,
    artifact: crate::model::LitehouseArtifactSummary,
    audits: Vec<LitehouseAudit>,
) -> LitehouseReport {
    let categories = vec![
        category("performance", "Performance", &audits),
        category("accessibility", "Accessibility", &audits),
        category("seo", "SEO", &audits),
        category("best-practices", "Best Practices", &audits),
    ];
    let score = categories
        .iter()
        .map(|category| category.score)
        .sum::<u16>();
    let max_score = categories
        .iter()
        .map(|category| category.max_score)
        .sum::<u16>();

    LitehouseReport {
        schema_version: LITEHOUSE_SCHEMA_VERSION.to_string(),
        engine: LITEHOUSE_ENGINE.to_string(),
        mode: mode.to_string(),
        fallback_from: None,
        id: id.to_string(),
        target_id: target_id.to_string(),
        url: url.to_string(),
        final_url,
        score,
        max_score,
        artifact,
        categories,
        audits,
    }
}

fn performance_audits(artifact: &LitehousePageArtifact) -> Vec<LitehouseAudit> {
    vec![
        audit(
            "performance",
            "performance-http-status",
            "HTTP status",
            http_status_score(artifact.status, artifact.required_status),
            artifact
                .status
                .map(|status| format!("Returned HTTP {status}"))
                .unwrap_or_else(|| "No HTTP status was captured.".to_string()),
            "Fix failing routes or configure the expected status with evidence.",
        ),
        audit(
            "performance",
            "performance-response-time",
            "Initial response time",
            response_time_score(artifact.response_time_ms),
            artifact
                .response_time_ms
                .map(|time| format!("Initial response completed in {time}ms"))
                .unwrap_or_else(|| "No initial response timing was captured.".to_string()),
            "Reduce server work before first byte or collect deeper browser timing evidence.",
        ),
        audit(
            "performance",
            "performance-html-size",
            "Initial HTML size",
            html_size_score(artifact.html_bytes, artifact.max_html_bytes),
            artifact
                .html_bytes
                .map(|bytes| format!("Initial HTML response is {bytes} bytes"))
                .unwrap_or_else(|| "No initial HTML byte count was captured.".to_string()),
            "Reduce initial HTML size or raise the budget only with measured evidence.",
        ),
        audit(
            "performance",
            "performance-bounded-capture",
            "Bounded capture",
            if artifact.body_truncated { 30 } else { 100 },
            if artifact.body_truncated {
                "HTML capture hit the bounded byte limit."
            } else {
                "HTML capture completed inside the bounded byte limit."
            },
            "Keep web audit responses small enough for deterministic local receipts.",
        ),
    ]
}

fn seo_audits(metadata: &HtmlMetadata, signals: &HtmlSignals) -> Vec<LitehouseAudit> {
    vec![
        boolean_audit(
            "seo",
            "seo-title",
            "Document title",
            metadata.title_present,
            "The initial document has a non-empty title.",
            "Add a concise, descriptive title to the initial HTML document.",
        ),
        boolean_audit(
            "seo",
            "seo-description",
            "Meta description",
            metadata.description_present,
            "The initial document has a meta description.",
            "Add a useful meta description for search and link previews.",
        ),
        boolean_audit(
            "seo",
            "seo-viewport",
            "Viewport",
            metadata.viewport_present,
            "The initial document has a viewport meta tag.",
            "Add a viewport meta tag so mobile rendering can be audited.",
        ),
        audit(
            "seo",
            "seo-canonical",
            "Canonical link",
            if metadata.canonical_present { 100 } else { 70 },
            if metadata.canonical_present {
                "The document declares a canonical URL."
            } else {
                "No canonical URL was found in the initial HTML."
            },
            "Add a canonical URL when the route has a stable public address.",
        ),
        audit(
            "seo",
            "seo-h1",
            "Primary heading",
            if signals.h1_present { 100 } else { 70 },
            if signals.h1_present {
                "The document includes an H1 heading."
            } else {
                "No H1 heading was found in the captured HTML."
            },
            "Add one clear H1 that matches the route purpose.",
        ),
    ]
}

fn accessibility_audits(signals: &HtmlSignals) -> Vec<LitehouseAudit> {
    vec![
        boolean_audit(
            "accessibility",
            "accessibility-html-lang",
            "HTML language",
            signals.html_lang_present,
            "The html element declares a language.",
            "Add a lang attribute to the html element.",
        ),
        count_audit(
            "accessibility",
            "accessibility-image-alt",
            "Image alt text",
            signals.missing_image_alt_count,
            "Every captured image has alt text.",
            "Add alt text for meaningful images or empty alt text for decorative images.",
        ),
        count_audit(
            "accessibility",
            "accessibility-button-name",
            "Button names",
            signals.missing_button_name_count,
            "Every captured button has visible or ARIA text.",
            "Give icon-only buttons aria-label text or visible labels.",
        ),
        count_audit(
            "accessibility",
            "accessibility-form-label",
            "Form labels",
            signals.missing_form_label_count,
            "Every captured form input has a label.",
            "Connect labels to form controls with for/id or ARIA labels.",
        ),
        audit(
            "accessibility",
            "accessibility-main-landmark",
            "Main landmark",
            if signals.main_landmark_present {
                100
            } else {
                70
            },
            if signals.main_landmark_present {
                "The page exposes a main landmark."
            } else {
                "No main landmark was found in the captured HTML."
            },
            "Add a main element or role=main to support assistive navigation.",
        ),
    ]
}

fn best_practice_audits(
    artifact: &LitehousePageArtifact,
    signals: &HtmlSignals,
) -> Vec<LitehouseAudit> {
    let headers = artifact
        .headers
        .iter()
        .map(|header| header.name.as_str())
        .collect::<HashSet<_>>();

    vec![
        audit(
            "best-practices",
            "best-practices-https",
            "HTTPS",
            if artifact.url.starts_with("https://")
                && artifact
                    .final_url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://"))
            {
                100
            } else {
                60
            },
            if artifact
                .final_url
                .as_deref()
                .is_some_and(|url| url.starts_with("https://"))
            {
                "The final URL uses HTTPS."
            } else {
                "The audited URL does not use HTTPS."
            },
            "Use HTTPS for public targets and record localhost exceptions explicitly.",
        ),
        audit(
            "best-practices",
            "best-practices-security-headers",
            "Security headers",
            security_header_score(artifact.security_header_count),
            format!(
                "Detected {} common security headers",
                artifact.security_header_count
            ),
            "Add CSP, HSTS, X-Content-Type-Options, Referrer-Policy, and Permissions-Policy where appropriate.",
        ),
        header_audit(
            "best-practices",
            "best-practices-csp",
            "Content Security Policy",
            headers.contains("content-security-policy"),
            "A Content Security Policy header is present.",
            "Add a Content Security Policy for production routes.",
        ),
        header_audit(
            "best-practices",
            "best-practices-content-type-options",
            "Content type hardening",
            headers.contains("x-content-type-options"),
            "X-Content-Type-Options is present.",
            "Add X-Content-Type-Options: nosniff.",
        ),
        count_audit(
            "best-practices",
            "best-practices-insecure-references",
            "Insecure references",
            signals.insecure_reference_count,
            "No insecure http:// resource references were found.",
            "Serve scripts, styles, and media over HTTPS.",
        ),
    ]
}

fn category(id: &'static str, label: &'static str, audits: &[LitehouseAudit]) -> LitehouseCategory {
    let category_audits = audits
        .iter()
        .filter(|audit| audit.category == id)
        .collect::<Vec<_>>();
    let score = if category_audits.is_empty() {
        0
    } else {
        category_audits.iter().map(|audit| audit.score).sum::<u16>() / category_audits.len() as u16
    };

    LitehouseCategory {
        id: id.to_string(),
        label: label.to_string(),
        score,
        max_score: CATEGORY_MAX_SCORE,
        status: category_status_for_score(score).to_string(),
    }
}

fn boolean_audit(
    category: &'static str,
    id: &'static str,
    label: &'static str,
    present: bool,
    ready_detail: &'static str,
    next_action: &'static str,
) -> LitehouseAudit {
    audit(
        category,
        id,
        label,
        if present { 100 } else { 0 },
        if present {
            ready_detail.to_string()
        } else {
            next_action.to_string()
        },
        next_action,
    )
}

fn count_audit(
    category: &'static str,
    id: &'static str,
    label: &'static str,
    missing_count: usize,
    ready_detail: &'static str,
    next_action: &'static str,
) -> LitehouseAudit {
    let detail = if missing_count == 0 {
        ready_detail.to_string()
    } else {
        format!("{missing_count} captured element(s) need attention.")
    };
    audit(
        category,
        id,
        label,
        if missing_count == 0 { 100 } else { 0 },
        detail,
        next_action,
    )
}

fn header_audit(
    category: &'static str,
    id: &'static str,
    label: &'static str,
    present: bool,
    ready_detail: &'static str,
    next_action: &'static str,
) -> LitehouseAudit {
    audit(
        category,
        id,
        label,
        if present { 100 } else { 50 },
        if present {
            ready_detail.to_string()
        } else {
            next_action.to_string()
        },
        next_action,
    )
}

pub(crate) fn audit(
    category: impl Into<String>,
    id: impl Into<String>,
    label: impl Into<String>,
    score: u16,
    detail: impl Into<String>,
    next_action: impl Into<String>,
) -> LitehouseAudit {
    let score = score.min(CATEGORY_MAX_SCORE);
    LitehouseAudit {
        id: id.into(),
        category: category.into(),
        label: label.into(),
        score,
        max_score: CATEGORY_MAX_SCORE,
        status: audit_status_for_score(score).to_string(),
        detail: detail.into(),
        next_action: next_action.into(),
    }
}

pub(crate) fn category_status_for_score(score: u16) -> &'static str {
    if score == CATEGORY_MAX_SCORE {
        "ready"
    } else if score == 0 {
        "blocked"
    } else {
        "warning"
    }
}

pub(crate) fn audit_status_for_score(score: u16) -> &'static str {
    if score == CATEGORY_MAX_SCORE {
        "ready"
    } else {
        "warning"
    }
}

fn http_status_score(status: Option<u16>, required_status: Option<u16>) -> u16 {
    let Some(status) = status else {
        return 0;
    };
    if let Some(required_status) = required_status {
        return if status == required_status { 100 } else { 0 };
    }
    if (200..=399).contains(&status) {
        100
    } else {
        0
    }
}

fn response_time_score(response_time_ms: Option<u128>) -> u16 {
    match response_time_ms {
        Some(0..=500) => 100,
        Some(501..=1_200) => 80,
        Some(1_201..=2_500) => 50,
        Some(_) => 20,
        None => 0,
    }
}

fn html_size_score(html_bytes: Option<u64>, budget: Option<u64>) -> u16 {
    let Some(html_bytes) = html_bytes else {
        return 0;
    };
    if let Some(budget) = budget {
        if html_bytes <= budget {
            return 100;
        }
        if html_bytes <= budget.saturating_mul(3) / 2 {
            return 70;
        }
        return 30;
    }

    match html_bytes {
        0..=200_000 => 100,
        200_001..=512_000 => 80,
        512_001..=1_000_000 => 50,
        _ => 20,
    }
}

fn security_header_score(count: u16) -> u16 {
    match count {
        0 => 30,
        1 => 60,
        2 | 3 => 80,
        _ => 100,
    }
}
