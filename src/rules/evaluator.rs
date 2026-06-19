use crate::inventory::{ProjectInventory, is_approved_machine_cache_relative_path};
use crate::model::{
    DxFinding, DxMeasurementKind, DxRuleDefinition, DxTestInventory, DxWebAuditResult,
};

use super::component_scan::{
    component_has_boundary_leak, component_lacks_quality_affordance, is_component_file,
};
use super::source_scan::{
    generated_source_leak, rust_file_has_application_unwrap, source_contains_insecure_default,
};

pub fn evaluate_rules(
    inventory: &ProjectInventory,
    test_inventory: &DxTestInventory,
    web_audit_results: &[DxWebAuditResult],
    rules: &[DxRuleDefinition],
) -> Vec<DxFinding> {
    let mut findings = Vec::new();

    for rule in rules {
        evaluate_rule(
            inventory,
            test_inventory,
            web_audit_results,
            rule,
            &mut findings,
        );
    }

    findings
}

fn evaluate_rule(
    inventory: &ProjectInventory,
    test_inventory: &DxTestInventory,
    web_audit_results: &[DxWebAuditResult],
    rule: &DxRuleDefinition,
    findings: &mut Vec<DxFinding>,
) {
    match rule.metric.as_str() {
        "line_count" => evaluate_file_numeric_rule(
            inventory,
            rule,
            |file| file.line_count as u64,
            "lines",
            "Split mixed responsibilities into smaller modules before the file becomes hard to review.",
            findings,
        ),
        "byte_size" => evaluate_file_numeric_rule(
            inventory,
            rule,
            |file| file.bytes,
            "bytes",
            "Extract cohesive modules or move generated artifacts out of hand-authored source.",
            findings,
        ),
        "node_modules" => {
            if rule.operator == "absent" && inventory.contains_node_modules {
                findings.push(rule_finding(
                    rule,
                    "node_modules exists inside the source-owned project tree",
                    "Keep dependency installs outside DX source-owned lanes or document the exception.",
                    None,
                    Some(1),
                ));
            }
        }
        "generated_machine" => {
            if rule.operator != "absent" {
                return;
            }
            for file in inventory.files.iter().filter(|file| {
                file.relative_path.ends_with(".machine")
                    && !is_approved_machine_cache_relative_path(&file.relative_path)
            }) {
                findings.push(rule_finding(
                    rule,
                    format!(
                        "{} looks like a generated machine artifact in source",
                        file.relative_path
                    ),
                    "Store generated machine cache artifacts under an approved .dx cache root.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "generated_source" => {
            if rule.operator != "absent" {
                return;
            }
            for file in inventory
                .files
                .iter()
                .filter(|file| generated_source_leak(&file.relative_path, &file.path))
            {
                findings.push(rule_finding(
                    rule,
                    format!("{} looks like generated source in a hand-authored lane", file.relative_path),
                    "Move generated source behind a generator contract, ignore it from source-owned DX lanes, or document the source ownership exception.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "naming_convention" => {
            if rule.operator != "absent" {
                return;
            }
            for file in inventory
                .files
                .iter()
                .filter(|file| path_has_unfriendly_name(&file.relative_path))
            {
                findings.push(rule_finding(
                    rule,
                    format!(
                        "{} uses spaces or uppercase naming in a source-owned path",
                        file.relative_path
                    ),
                    "Use lowercase kebab-case or snake_case source-owned paths unless the filename is a conventional project root file.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "component_lines" => {
            evaluate_file_numeric_rule_filtered(
                inventory,
                rule,
                |file| is_component_file(&file.relative_path),
                |file| file.line_count as u64,
                "lines in a component-shaped file",
                "Extract state, data access, and presentational pieces into focused modules.",
                findings,
            );
        }
        "component_boundary" => {
            if rule.operator != "absent" {
                return;
            }
            for file in inventory.files.iter().filter(|file| {
                is_component_file(&file.relative_path) && component_has_boundary_leak(&file.path)
            }) {
                findings.push(rule_finding(
                    rule,
                    format!(
                        "{} imports a server-only module from a component boundary",
                        file.relative_path
                    ),
                    "Move filesystem, process, network-server, and other server-only logic behind a server action, API route, or adapter module.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "component_quality" => {
            if rule.operator != "present" {
                return;
            }
            for file in inventory.files.iter().filter(|file| {
                is_component_file(&file.relative_path)
                    && component_lacks_quality_affordance(&file.path)
            }) {
                findings.push(rule_finding(
                    rule,
                    format!(
                        "{} lacks a className or typed props affordance for design-system composition",
                        file.relative_path
                    ),
                    "Expose typed props and a className affordance so the component can compose cleanly without depending on shadcn internals.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "rust_unwraps" => {
            for file in inventory.files.iter().filter(|file| {
                file.relative_path.ends_with(".rs")
                    && rust_file_has_application_unwrap(&file.relative_path, &file.path)
                    && violates_numeric_rule(1, rule)
            }) {
                findings.push(rule_finding(
                    rule,
                    format!(
                        "{} contains unwrap/expect in source code",
                        file.relative_path
                    ),
                    "Use typed errors or narrow expect messages in application paths.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "insecure_source" => {
            for file in inventory.files.iter().filter(|file| {
                source_contains_insecure_default(&file.path) && violates_numeric_rule(1, rule)
            }) {
                findings.push(rule_finding(
                    rule,
                    format!("{} contains an insecure default marker", file.relative_path),
                    "Remove hard-coded secrets and unsafe defaults before launch.",
                    Some(file.relative_path.clone()),
                    Some(1),
                ));
            }
        }
        "project_structure" => {
            let orientation_count = project_orientation_file_count(inventory);
            if violates_numeric_rule(orientation_count, rule)
                || (rule.operator == "present" && orientation_count == 0)
            {
                findings.push(rule_finding(
                    rule,
                    "Project is missing README.md, TODO.md, CHANGELOG.md, AGENTS.md, DX.md, or dx orientation for future DX workers",
                    "Add or maintain README.md, TODO.md, CHANGELOG.md, AGENTS.md, DX.md, or the extensionless dx project config so AI workers can recover project intent quickly.",
                    None,
                    Some(orientation_count),
                ));
            }
        }
        "test_count" => {
            let test_count = discovered_test_count(test_inventory);
            if violates_numeric_rule(test_count, rule)
                || (rule.operator == "present" && test_count == 0)
            {
                findings.push(rule_finding(
                    rule,
                    "No tests were discovered for this project",
                    "Add at least one focused Rust, JS/TS, Python, Go, C, or C++ test before relying on launch readiness.",
                    None,
                    Some(test_count),
                ));
            }
        }
        "web_http_status"
        | "web_html_bytes"
        | "web_title_present"
        | "web_description_present"
        | "web_canonical_present"
        | "web_viewport_present"
        | "web_security_header_count" => {
            evaluate_web_audit_numeric_rule(web_audit_results, rule, findings);
        }
        _ => {}
    }
}

fn evaluate_file_numeric_rule(
    inventory: &ProjectInventory,
    rule: &DxRuleDefinition,
    actual: impl Fn(&crate::inventory::SourceFile) -> u64,
    unit: &str,
    next_action: &str,
    findings: &mut Vec<DxFinding>,
) {
    evaluate_file_numeric_rule_filtered(
        inventory,
        rule,
        |_| true,
        actual,
        unit,
        next_action,
        findings,
    );
}

fn evaluate_file_numeric_rule_filtered(
    inventory: &ProjectInventory,
    rule: &DxRuleDefinition,
    filter: impl Fn(&crate::inventory::SourceFile) -> bool,
    actual: impl Fn(&crate::inventory::SourceFile) -> u64,
    unit: &str,
    next_action: &str,
    findings: &mut Vec<DxFinding>,
) {
    for file in inventory.files.iter().filter(|file| filter(file)) {
        let value = actual(file);
        if violates_numeric_rule(value, rule) {
            let threshold = rule
                .threshold
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            findings.push(rule_finding(
                rule,
                format!(
                    "{} has {} {unit}, {} the rule threshold {}",
                    file.relative_path,
                    value,
                    numeric_rule_relation(rule),
                    threshold
                ),
                next_action,
                Some(file.relative_path.clone()),
                Some(value),
            ));
        }
    }
}

fn numeric_rule_relation(rule: &DxRuleDefinition) -> &'static str {
    match rule.operator.as_str() {
        "min" | ">=" => "below",
        "eq" | "=" | "==" => "different from",
        _ => "above",
    }
}

fn rule_finding(
    rule: &DxRuleDefinition,
    message: impl Into<String>,
    next_action: impl Into<String>,
    file: Option<String>,
    actual: Option<u64>,
) -> DxFinding {
    DxFinding {
        id: rule.id.clone(),
        category: rule.category.clone(),
        severity: rule.severity,
        message: message.into(),
        next_action: next_action.into(),
        measurement: DxMeasurementKind::Measured,
        file,
        actual: actual.map(|value| value.to_string()),
        threshold: rule.threshold.map(|value| value.to_string()),
        weight: rule.weight,
        docs: rule.docs.clone(),
        provenance: rule.provenance.clone(),
    }
}

fn evaluate_web_audit_numeric_rule(
    web_audit_results: &[DxWebAuditResult],
    rule: &DxRuleDefinition,
    findings: &mut Vec<DxFinding>,
) {
    for result in web_audit_results {
        let Some(value) = crate::web_audit::metric_value(result, &rule.metric) else {
            continue;
        };
        if violates_numeric_rule(value, rule) {
            findings.push(rule_finding(
                rule,
                format!(
                    "Web audit `{}` for {} measured {} as {}",
                    result.target_id,
                    result.url,
                    web_metric_label(&rule.metric),
                    value
                ),
                "Fix the reported web audit issue, collect fresh web evidence, then rerun dx check.",
                result
                    .source
                    .as_ref()
                    .map(|source| source.display().to_string())
                    .or_else(|| Some(result.url.clone())),
                Some(value),
            ));
        }
    }
}

fn web_metric_label(metric: &str) -> &'static str {
    match metric {
        "web_http_status" => "HTTP status",
        "web_html_bytes" => "HTML bytes",
        "web_title_present" => "title presence",
        "web_description_present" => "description presence",
        "web_canonical_present" => "canonical presence",
        "web_viewport_present" => "viewport presence",
        "web_security_header_count" => "security header count",
        _ => "web audit metric",
    }
}

fn violates_numeric_rule(actual: u64, rule: &DxRuleDefinition) -> bool {
    let Some(threshold) = rule.threshold else {
        return false;
    };
    match rule.operator.as_str() {
        "max" | "<=" => actual > threshold,
        "min" | ">=" => actual < threshold,
        "eq" | "=" | "==" => actual != threshold,
        _ => false,
    }
}

fn project_orientation_file_count(inventory: &ProjectInventory) -> u64 {
    inventory
        .files
        .iter()
        .filter(|file| {
            matches!(
                file.relative_path.as_str(),
                "README.md" | "TODO.md" | "CHANGELOG.md" | "AGENTS.md" | "DX.md" | "dx"
            )
        })
        .count() as u64
}

fn discovered_test_count(inventory: &DxTestInventory) -> u64 {
    (inventory.rust_tests
        + inventory.js_tests
        + inventory.python_tests
        + inventory.go_tests
        + inventory.c_tests
        + inventory.cpp_tests) as u64
}

fn path_has_unfriendly_name(path: &str) -> bool {
    path.split('/').any(|segment| {
        if segment.is_empty() || segment.starts_with('.') || is_conventional_root_file(segment) {
            return false;
        }
        segment.chars().any(char::is_whitespace)
            || segment
                .chars()
                .any(|character| character.is_ascii_uppercase())
    })
}

fn is_conventional_root_file(segment: &str) -> bool {
    matches!(
        segment,
        "README.md"
            | "TODO.md"
            | "CHANGELOG.md"
            | "LICENSE"
            | "LICENSE.md"
            | "AGENTS.md"
            | "DX.md"
            | "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "tsconfig.json"
            | "tsconfig.build.json"
    )
}
