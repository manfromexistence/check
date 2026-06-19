use std::fs;
use std::path::Path;

use crate::model::DxDiagnostic;

use super::super::{
    bool_cell, column, config_diagnostic, row_diagnostic, row_label, string_cell, u64_cell,
};
use super::hash_blake3_is_valid;

mod package_proof;

pub const WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE: &str = ".dx/check/web-lighthouse-equivalence.sr";

pub(super) fn verifies_promoted_runtime(
    root: &Path,
    runtime_id: &str,
    executable_hash_blake3: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let source = root.join(WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE);
    if !source.is_file() {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-receipt-missing",
            &source,
            format!("DX JS Lighthouse runtime `{runtime_id}` is missing an equivalence receipt"),
            "Generate .dx/check/web-lighthouse-equivalence.sr before promoting a DX JS Lighthouse runtime to Check.",
        ));
        return false;
    }

    let Some(document) = read_equivalence_document(&source, diagnostics) else {
        return false;
    };

    let Some(schema) = string_cell(document.get_path("schema")) else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-receipt-invalid",
            &source,
            "Web Lighthouse equivalence receipt is missing schema",
            "Use schema=\"dx.check.web_lighthouse_equivalence.v1\" in the equivalence receipt.",
        ));
        return false;
    };
    if schema != "dx.check.web_lighthouse_equivalence.v1" {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-receipt-invalid",
            &source,
            format!("Web Lighthouse equivalence receipt schema `{schema}` is unsupported"),
            "Regenerate the receipt with schema dx.check.web_lighthouse_equivalence.v1.",
        ));
        return false;
    }

    let Some(section) = document.section_by_name("web_lighthouse_equivalence") else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-receipt-invalid",
            &source,
            "Web Lighthouse equivalence receipt is missing web_lighthouse_equivalence",
            "Add a web_lighthouse_equivalence table for the proven DX JS runtime.",
        ));
        return false;
    };
    let Some(columns) = EquivalenceColumns::read(section, &source, diagnostics) else {
        return false;
    };

    let mut matching_rows = Vec::new();
    for (row_index, row) in section.rows.iter().enumerate() {
        let current_row_label = row_label(row_index);
        let Some(row_runtime_id) = equivalence_cell(
            row.get(columns.runtime),
            &source,
            &current_row_label,
            "runtime_id",
            diagnostics,
        ) else {
            continue;
        };
        if row_runtime_id != runtime_id {
            continue;
        }
        let Some(provider) = equivalence_cell(
            row.get(columns.provider),
            &source,
            &current_row_label,
            "provider",
            diagnostics,
        ) else {
            continue;
        };
        matching_rows.push((row_index, provider));
    }

    if matching_rows.len() > 1 {
        let (first_index, first_provider) = &matching_rows[0];
        let (second_index, second_provider) = &matching_rows[1];
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-duplicate",
            &source,
            &row_label(*second_index),
            format!(
                "Web Lighthouse equivalence receipt has multiple rows for runtime `{runtime_id}`: {} provider `{}` and {} provider `{}`",
                row_label(*first_index),
                first_provider,
                row_label(*second_index),
                second_provider
            ),
            "Keep exactly one equivalence row for each promoted dx-js runtime so Check cannot accept order-dependent proof.",
        ));
        return false;
    }

    if let Some((row_index, provider)) = matching_rows.into_iter().next() {
        if provider != "dx-js" {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-equivalence-provider-mismatch",
                &source,
                &row_label(row_index),
                format!(
                    "Web Lighthouse equivalence row for runtime `{runtime_id}` uses provider `{provider}` instead of `dx-js`"
                ),
                "Regenerate the equivalence receipt with provider dx-js for the promoted DX JS Lighthouse runtime.",
            ));
            return false;
        }
        let row_label = row_label(row_index);
        if !row_verifies_runtime(
            &section.rows[row_index],
            &source,
            &row_label,
            runtime_id,
            executable_hash_blake3,
            &columns,
            diagnostics,
        ) {
            return false;
        }
        return package_proof::verifies_runtime(&document, &source, root, runtime_id, diagnostics);
    }

    diagnostics.push(config_diagnostic(
        "web-lighthouse-equivalence-runtime-missing",
        &source,
        format!("Web Lighthouse equivalence receipt has no verified row for runtime `{runtime_id}`"),
        "Add a web_lighthouse_equivalence row matching the promoted DX JS runtime id and executable hash.",
    ));
    false
}

fn equivalence_cell(
    value: Option<&serializer::DxLlmValue>,
    source: &Path,
    row_label: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<String> {
    match string_cell(value) {
        Some(value) => Some(value),
        None => {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-equivalence-receipt-invalid",
                source,
                row_label,
                format!("DX JS Lighthouse equivalence is missing `{field}`"),
                "Regenerate the DX JS Lighthouse equivalence receipt with all required fields.",
            ));
            None
        }
    }
}

fn read_equivalence_document(
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<serializer::DxDocument> {
    let body = match fs::read_to_string(source) {
        Ok(body) => body,
        Err(error) => {
            diagnostics.push(config_diagnostic(
                "web-lighthouse-equivalence-receipt-read-failed",
                source,
                format!("Web Lighthouse equivalence receipt could not be read: {error}"),
                "Regenerate the DX JS Lighthouse equivalence receipt, then rerun dx check.",
            ));
            return None;
        }
    };
    match serializer::llm_to_document(&body) {
        Ok(document) => Some(document),
        Err(error) => {
            diagnostics.push(config_diagnostic(
                "web-lighthouse-equivalence-receipt-parse-failed",
                source,
                format!("Web Lighthouse equivalence receipt could not be parsed: {error}"),
                "Regenerate a valid serializer equivalence receipt for the DX JS Lighthouse runtime.",
            ));
            None
        }
    }
}

struct EquivalenceColumns {
    runtime: usize,
    provider: usize,
    hash: usize,
    status: usize,
    sample_count: usize,
    category_scores_match: usize,
    lhr_json_shape_match: usize,
}

impl EquivalenceColumns {
    fn read(
        section: &serializer::DxSection,
        source: &Path,
        diagnostics: &mut Vec<DxDiagnostic>,
    ) -> Option<Self> {
        Some(Self {
            runtime: column(section, "runtime_id", source, diagnostics)?,
            provider: column(section, "provider", source, diagnostics)?,
            hash: column(section, "executable_hash_blake3", source, diagnostics)?,
            status: column(section, "status", source, diagnostics)?,
            sample_count: column(section, "sample_count", source, diagnostics)?,
            category_scores_match: column(section, "category_scores_match", source, diagnostics)?,
            lhr_json_shape_match: column(section, "lhr_json_shape_match", source, diagnostics)?,
        })
    }
}

fn row_verifies_runtime(
    row: &[serializer::DxLlmValue],
    source: &Path,
    row_label: &str,
    runtime_id: &str,
    executable_hash_blake3: &str,
    columns: &EquivalenceColumns,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let Some(status) = equivalence_cell(
        row.get(columns.status),
        source,
        row_label,
        "status",
        diagnostics,
    ) else {
        return false;
    };
    if status != "verified" {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-unverified",
            source,
            row_label,
            format!("DX JS Lighthouse runtime `{runtime_id}` equivalence status is `{status}`"),
            "Use status verified only after comparing official Lighthouse category scores and LHR JSON shape.",
        ));
        return false;
    }

    let Some(proof_hash) = equivalence_cell(
        row.get(columns.hash),
        source,
        row_label,
        "executable_hash_blake3",
        diagnostics,
    ) else {
        return false;
    };
    if !hash_blake3_is_valid(&proof_hash) {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-invalid-hash",
            source,
            row_label,
            "DX JS Lighthouse equivalence executable_hash_blake3 is not a 64-character hex digest",
            "Regenerate the equivalence receipt with the promoted executable BLAKE3 hash.",
        ));
        return false;
    }
    if proof_hash != executable_hash_blake3 {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-hash-mismatch",
            source,
            row_label,
            format!("DX JS Lighthouse runtime `{runtime_id}` equivalence hash does not match the runtime executable"),
            "Regenerate the equivalence receipt after promoting the DX JS/build Lighthouse executable.",
        ));
        return false;
    }

    let Some(sample_count) = row.get(columns.sample_count).and_then(u64_cell) else {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-insufficient-samples",
            source,
            row_label,
            "DX JS Lighthouse equivalence sample_count is missing or invalid",
            "Record at least one official Lighthouse equivalence sample before promotion.",
        ));
        return false;
    };
    if sample_count == 0 {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-insufficient-samples",
            source,
            row_label,
            "DX JS Lighthouse equivalence sample_count must be greater than zero",
            "Record at least one official Lighthouse equivalence sample before promotion.",
        ));
        return false;
    }
    let Some(category_scores_match) = required_equivalence_bool(
        row.get(columns.category_scores_match),
        source,
        row_label,
        "category_scores_match",
        diagnostics,
    ) else {
        return false;
    };
    if !category_scores_match {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-category-mismatch",
            source,
            row_label,
            "DX JS Lighthouse equivalence does not prove category score parity",
            "Compare official and DX JS Lighthouse category scores before promotion.",
        ));
        return false;
    }
    let Some(lhr_json_shape_match) = required_equivalence_bool(
        row.get(columns.lhr_json_shape_match),
        source,
        row_label,
        "lhr_json_shape_match",
        diagnostics,
    ) else {
        return false;
    };
    if !lhr_json_shape_match {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-lhr-shape-mismatch",
            source,
            row_label,
            "DX JS Lighthouse equivalence does not prove LHR JSON shape parity",
            "Compare official and DX JS Lighthouse LHR JSON shape before promotion.",
        ));
        return false;
    }

    true
}

fn required_equivalence_bool(
    value: Option<&serializer::DxLlmValue>,
    source: &Path,
    row_label: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<bool> {
    match bool_cell(value) {
        Some(value) => Some(value),
        None => {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-equivalence-invalid-boolean",
                source,
                row_label,
                format!("DX JS Lighthouse equivalence field `{field}` must be true or false"),
                "Regenerate the equivalence receipt with explicit boolean parity fields.",
            ));
            None
        }
    }
}
