use std::fs;
use std::path::{Path, PathBuf};

use serializer::DxLlmValue;

use crate::model::DxDiagnostic;

use super::{column, config_diagnostic, row_diagnostic, row_label, string_cell};

mod arguments;
mod equivalence;

use arguments::{
    dx_js_lighthouse_args_are_valid, format_runtime_args, parse_lighthouse_runtime_args,
};

pub use equivalence::WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE;

pub const WEB_LIGHTHOUSE_RUNTIMES_SOURCE: &str = ".dx/check/web-lighthouse-runtimes.sr";
const DX_JS_LIGHTHOUSE_RUNTIME_ID: &str = "dx-js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxWebLighthouseRuntime {
    pub id: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub(super) fn load_project_lighthouse_runtime(
    root: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<DxWebLighthouseRuntime> {
    let source = root.join(WEB_LIGHTHOUSE_RUNTIMES_SOURCE);
    if !source.is_file() {
        return None;
    }

    let body = match fs::read_to_string(&source) {
        Ok(body) => body,
        Err(error) => {
            diagnostics.push(config_diagnostic(
                "web-lighthouse-runtime-receipt-read-failed",
                &source,
                format!("Web Lighthouse runtime receipt could not be read: {error}"),
                "Regenerate the DX Check web Lighthouse runtime receipt, then rerun dx check.",
            ));
            return None;
        }
    };
    let document = match serializer::llm_to_document(&body) {
        Ok(document) => document,
        Err(error) => {
            diagnostics.push(config_diagnostic(
                "web-lighthouse-runtime-receipt-parse-failed",
                &source,
                format!("Web Lighthouse runtime receipt could not be parsed: {error}"),
                "Regenerate a valid serializer receipt for the DX JS Lighthouse runtime.",
            ));
            return None;
        }
    };
    let Some(schema) = string_cell(document.get_path("schema")) else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-runtime-receipt-invalid",
            &source,
            "Web Lighthouse runtime receipt is missing schema",
            "Use schema=\"dx.check.web_lighthouse_runtimes.v1\" in the runtime receipt.",
        ));
        return None;
    };
    if schema != "dx.check.web_lighthouse_runtimes.v1" {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-runtime-receipt-invalid",
            &source,
            format!("Web Lighthouse runtime receipt schema `{schema}` is unsupported"),
            "Regenerate the receipt with schema dx.check.web_lighthouse_runtimes.v1.",
        ));
        return None;
    }

    let args_by_runtime = parse_lighthouse_runtime_args(&document, &source, diagnostics);
    let Some(section) = document.section_by_name("web_lighthouse_runtimes") else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-runtime-receipt-invalid",
            &source,
            "Web Lighthouse runtime receipt is missing web_lighthouse_runtimes",
            "Add a web_lighthouse_runtimes table with a proven dx-js runtime.",
        ));
        return None;
    };
    let id_index = column(section, "id", &source, diagnostics)?;
    let provider_index = column(section, "provider", &source, diagnostics)?;
    let command_index = column(section, "command", &source, diagnostics)?;
    let cwd_index = column(section, "cwd", &source, diagnostics)?;
    let executable_index = column(section, "executable", &source, diagnostics)?;
    let hash_index = column(section, "hash_blake3", &source, diagnostics)?;
    let claim_index = column(section, "claim_status", &source, diagnostics)?;
    let equivalence_index = column(section, "equivalence_status", &source, diagnostics)?;
    if has_unsupported_provider_rows(section, provider_index, &source, diagnostics) {
        return None;
    }
    if has_duplicate_dx_js_runtime_rows(section, id_index, provider_index, &source, diagnostics) {
        return None;
    }

    for (row_index, row) in section.rows.iter().enumerate() {
        let row_label = row_label(row_index);
        let Some(id) = runtime_cell(row.get(id_index), &source, &row_label, "id", diagnostics)
        else {
            continue;
        };
        let Some(provider) = runtime_cell(
            row.get(provider_index),
            &source,
            &row_label,
            "provider",
            diagnostics,
        ) else {
            continue;
        };
        if provider != "dx-js" {
            continue;
        }
        if id != DX_JS_LIGHTHOUSE_RUNTIME_ID {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-provider-id-mismatch",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime provider `dx-js` must use id `{DX_JS_LIGHTHOUSE_RUNTIME_ID}`, got `{id}`"
                ),
                "Regenerate the runtime and equivalence receipts with the canonical dx-js runtime id.",
            ));
            continue;
        }
        let Some(claim_status) = runtime_cell(
            row.get(claim_index),
            &source,
            &row_label,
            "claim_status",
            diagnostics,
        ) else {
            continue;
        };
        let Some(equivalence_status) = runtime_cell(
            row.get(equivalence_index),
            &source,
            &row_label,
            "equivalence_status",
            diagnostics,
        ) else {
            continue;
        };
        if claim_status != "proven_bundle" || equivalence_status != "verified" {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-unverified",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime `{id}` has claim_status `{claim_status}` and equivalence_status `{equivalence_status}`"
                ),
                "Promote only a proven DX JS Lighthouse bundle with verified category-score equivalence.",
            ));
            continue;
        }

        let Some(command) = runtime_cell(
            row.get(command_index),
            &source,
            &row_label,
            "command",
            diagnostics,
        ) else {
            continue;
        };
        let Some(cwd) = runtime_cell(row.get(cwd_index), &source, &row_label, "cwd", diagnostics)
        else {
            continue;
        };
        let Some(executable) = runtime_cell(
            row.get(executable_index),
            &source,
            &row_label,
            "executable",
            diagnostics,
        ) else {
            continue;
        };
        let Some(hash_blake3) = runtime_cell(
            row.get(hash_index),
            &source,
            &row_label,
            "hash_blake3",
            diagnostics,
        ) else {
            continue;
        };
        let command = resolve_receipt_path(root, &command);
        let cwd = resolve_receipt_path(root, &cwd);
        let executable = resolve_receipt_path(root, &executable);
        if !command.is_file() {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-missing-command",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime command `{}` does not exist",
                    command.display()
                ),
                "Install or promote the DX JS Lighthouse command before selecting it in dx check.",
            ));
            continue;
        }
        if !cwd.is_dir() {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-missing-cwd",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime cwd `{}` does not exist",
                    cwd.display()
                ),
                "Point the runtime receipt cwd at the DX hub root used by the runtime contract.",
            ));
            continue;
        }
        if !hash_blake3_is_valid(&hash_blake3) {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-invalid-hash",
                &source,
                &row_label,
                "DX JS Lighthouse runtime hash_blake3 is not a 64-character hex digest",
                "Regenerate the runtime receipt with the executable BLAKE3 hash.",
            ));
            continue;
        }
        let Ok(executable_bytes) = fs::read(&executable) else {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-missing-executable",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime executable `{}` could not be read",
                    executable.display()
                ),
                "Install or promote the hashed DX JS runtime executable before selecting it in dx check.",
            ));
            continue;
        };
        if !paths_refer_to_same_file(&command, &executable) {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-command-executable-mismatch",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime `{id}` command `{}` is not the hashed executable `{}`",
                    command.display(),
                    executable.display()
                ),
                "Use the same file for command and executable so Check hashes exactly the DX JS runtime it will execute.",
            ));
            continue;
        }
        let actual_hash = blake3::hash(&executable_bytes).to_hex().to_string();
        if actual_hash != hash_blake3 {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-hash-stale",
                &source,
                &row_label,
                format!("DX JS Lighthouse runtime `{id}` executable hash does not match receipt"),
                "Regenerate the runtime receipt after promoting the DX JS/build Lighthouse executable.",
            ));
            continue;
        }
        let args = args_by_runtime.get(&id).cloned().unwrap_or_default();
        if !dx_js_lighthouse_args_are_valid(&args) {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-invalid-args",
                &source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime `{id}` args must be exactly `js lighthouse`, got `{}`",
                    format_runtime_args(&args)
                ),
                "Regenerate the runtime receipt with web_lighthouse_runtime_args rows for position 0 `js` and position 1 `lighthouse` only.",
            ));
            continue;
        }
        if !equivalence::verifies_promoted_runtime(root, &id, &actual_hash, diagnostics) {
            continue;
        }
        return Some(DxWebLighthouseRuntime {
            id,
            command,
            args,
            cwd,
        });
    }

    None
}

fn has_unsupported_provider_rows(
    section: &serializer::DxSection,
    provider_index: usize,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    for (row_index, row) in section.rows.iter().enumerate() {
        let Some(provider) = string_cell(row.get(provider_index)) else {
            continue;
        };
        if provider == "dx-js" {
            continue;
        }

        let row_label = row_label(row_index);
        diagnostics.push(row_diagnostic(
            "web-lighthouse-runtime-unsupported-provider",
            source,
            &row_label,
            format!("Web Lighthouse runtime provider `{provider}` is not supported"),
            "Keep only the canonical dx-js provider in the DX JS Lighthouse runtime receipt until another provider has a separate verified Check contract.",
        ));
        return true;
    }

    false
}

fn has_duplicate_dx_js_runtime_rows(
    section: &serializer::DxSection,
    id_index: usize,
    provider_index: usize,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let mut first_id: Option<String> = None;
    for (row_index, row) in section.rows.iter().enumerate() {
        let Some(provider) = string_cell(row.get(provider_index)) else {
            continue;
        };
        if provider != "dx-js" {
            continue;
        }
        let runtime_id = string_cell(row.get(id_index)).unwrap_or_else(|| "<missing>".to_string());
        let Some(previous_id) = first_id.as_ref() else {
            first_id = Some(runtime_id);
            continue;
        };

        let row_label = row_label(row_index);
        diagnostics.push(row_diagnostic(
            "web-lighthouse-runtime-duplicate",
            source,
            &row_label,
            format!(
                "Web Lighthouse runtime receipt has multiple dx-js runtime rows: `{previous_id}` and `{runtime_id}`"
            ),
            "Keep exactly one promoted dx-js Lighthouse runtime row so Check cannot select an order-dependent command.",
        ));
        return true;
    }

    false
}

fn runtime_cell(
    value: Option<&DxLlmValue>,
    source: &Path,
    row_label: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<String> {
    match string_cell(value) {
        Some(value) => Some(value),
        None => {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-receipt-invalid",
                source,
                row_label,
                format!("DX JS Lighthouse runtime is missing `{field}`"),
                "Regenerate the DX JS/build Lighthouse runtime receipt with all required fields.",
            ));
            None
        }
    }
}

fn resolve_receipt_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    let Ok(left) = fs::canonicalize(left) else {
        return false;
    };
    let Ok(right) = fs::canonicalize(right) else {
        return false;
    };
    canonical_paths_match(&left, &right)
}

#[cfg(windows)]
fn canonical_paths_match(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(right.to_string_lossy().as_ref())
}

#[cfg(not(windows))]
fn canonical_paths_match(left: &Path, right: &Path) -> bool {
    left == right
}

fn hash_blake3_is_valid(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}
