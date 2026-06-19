use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::model::DxDiagnostic;

use super::super::super::super::{bool_cell, row_diagnostic, string_cell};

const LIGHTHOUSE_PACKAGE_NAME: &str = "lighthouse";
const DX_BUILD_LIGHTHOUSE_PACKAGE_SCHEMA: &str = "dx.build.lighthouse_package.v1";

pub(super) fn verifies(
    root: &Path,
    source: &Path,
    row_label: &str,
    runtime_id: &str,
    receipt_path: &str,
    receipt_hash: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    if !project_serializer_machine_path_is_safe(receipt_path) {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-invalid-build-receipt-path",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` build_receipt_path `{receipt_path}` is not a project-relative .dx/serializer/*.machine path"
            ),
            "Store DX Build Lighthouse package receipts as generated serializer machine artifacts under .dx/serializer.",
        ));
        return false;
    }

    let receipt = root.join(Path::new(receipt_path));
    if !receipt.is_file() {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-build-receipt-missing",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` build receipt `{}` does not exist",
                receipt.display()
            ),
            "Regenerate the DX Build Lighthouse package machine receipt before promoting the runtime.",
        ));
        return false;
    }
    if !path_resolves_inside_directory(&receipt, &root.join(".dx").join("serializer")) {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-build-receipt-outside-serializer",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` build receipt `{}` resolves outside .dx/serializer",
                receipt.display()
            ),
            "Use a serializer-generated machine receipt that resolves inside the project .dx/serializer directory.",
        ));
        return false;
    }

    let receipt_bytes = match fs::read(&receipt) {
        Ok(bytes) => bytes,
        Err(error) => {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-equivalence-package-proof-build-receipt-read-failed",
                source,
                row_label,
                format!(
                    "DX JS Lighthouse runtime `{runtime_id}` build receipt `{}` could not be read: {error}",
                    receipt.display()
                ),
                "Fix permissions or regenerate the DX Build Lighthouse package machine receipt.",
            ));
            return false;
        }
    };
    let actual_hash = blake3::hash(&receipt_bytes).to_hex().to_string();
    if actual_hash != receipt_hash {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-hash-mismatch",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` build receipt hash does not match `{}`",
                receipt.display()
            ),
            "Regenerate the package proof after DX Build produces the Lighthouse package receipt.",
        ));
        return false;
    }

    let format = serializer::MachineFormat::new(receipt_bytes);
    let document = match serializer::machine_to_document(&format) {
        Ok(document) => document,
        Err(error) => {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-equivalence-package-proof-machine-invalid",
                source,
                row_label,
                format!(
                    "DX JS Lighthouse runtime `{runtime_id}` build receipt `{}` is not readable serializer machine output: {error}",
                    receipt.display()
                ),
                "Regenerate the DX Build Lighthouse package receipt through the serializer machine output path.",
            ));
            return false;
        }
    };

    receipt_claims_match(&document, source, row_label, runtime_id, diagnostics)
}

fn receipt_claims_match(
    document: &serializer::DxDocument,
    source: &Path,
    row_label: &str,
    runtime_id: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let Some(schema) = string_cell(document.get_path("schema")) else {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-schema-mismatch",
            source,
            row_label,
            format!("DX JS Lighthouse runtime `{runtime_id}` build receipt is missing schema"),
            "Generate a dx.build.lighthouse_package.v1 machine receipt from DX Build.",
        ));
        return false;
    };
    if schema != DX_BUILD_LIGHTHOUSE_PACKAGE_SCHEMA {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-schema-mismatch",
            source,
            row_label,
            format!("DX Build Lighthouse package receipt schema `{schema}` is unsupported"),
            "Generate a dx.build.lighthouse_package.v1 machine receipt from DX Build.",
        ));
        return false;
    }

    let Some(package_name) = string_cell(document.get_path("package")) else {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-package-mismatch",
            source,
            row_label,
            format!("DX JS Lighthouse runtime `{runtime_id}` build receipt is missing package"),
            "Record a DX Build machine receipt for the official lighthouse package.",
        ));
        return false;
    };
    if package_name != LIGHTHOUSE_PACKAGE_NAME {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-package-mismatch",
            source,
            row_label,
            format!(
                "DX Build package receipt is for `{package_name}` instead of `{LIGHTHOUSE_PACKAGE_NAME}`"
            ),
            "Record a DX Build machine receipt for the official lighthouse package.",
        ));
        return false;
    }

    let Some(status) = string_cell(document.get_path("status")) else {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-unverified",
            source,
            row_label,
            format!("DX JS Lighthouse runtime `{runtime_id}` build receipt is missing status"),
            "Promote only a DX Build machine receipt with status verified.",
        ));
        return false;
    };
    if status != "verified" {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-machine-unverified",
            source,
            row_label,
            format!("DX Build Lighthouse package receipt status is `{status}`"),
            "Promote only a DX Build machine receipt with status verified.",
        ));
        return false;
    }

    receipt_bool(
        document,
        source,
        row_label,
        "package_assets_filesystem_addressable",
        diagnostics,
    ) && receipt_bool(
        document,
        source,
        row_label,
        "dynamic_imports_runtime_compatible",
        diagnostics,
    ) && receipt_bool(
        document,
        source,
        row_label,
        "node_builtins_runtime_compatible",
        diagnostics,
    ) && receipt_bool(
        document,
        source,
        row_label,
        "chrome_launcher_unstubbed",
        diagnostics,
    )
}

fn receipt_bool(
    document: &serializer::DxDocument,
    source: &Path,
    row_label: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    if bool_cell(document.get_path(field)).unwrap_or(false) {
        return true;
    }

    diagnostics.push(row_diagnostic(
        "web-lighthouse-equivalence-package-proof-machine-claim-mismatch",
        source,
        row_label,
        format!("DX Build Lighthouse package receipt does not verify `{field}`"),
        "Regenerate the DX Build receipt after proving Lighthouse package assets, dynamic imports, Node built-ins, and Chrome launcher behavior.",
    ));
    false
}

fn project_serializer_machine_path_is_safe(value: &str) -> bool {
    let path = Path::new(value.trim());
    if path.is_absolute()
        || path.extension().and_then(|extension| extension.to_str()) != Some("machine")
    {
        return false;
    }

    let components = path.components().collect::<Vec<_>>();
    components.len() >= 3
        && normal_component_eq(components[0], ".dx")
        && normal_component_eq(components[1], "serializer")
        && components
            .iter()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn normal_component_eq(component: Component<'_>, expected: &str) -> bool {
    match component {
        Component::Normal(value) => value.to_string_lossy().eq_ignore_ascii_case(expected),
        _ => false,
    }
}

fn path_resolves_inside_directory(path: &Path, directory: &Path) -> bool {
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    let Ok(directory) = fs::canonicalize(directory) else {
        return false;
    };
    canonical_path_starts_with(&path, &directory)
}

#[cfg(windows)]
fn canonical_path_starts_with(path: &Path, directory: &Path) -> bool {
    let path = lowercase_path(path);
    let directory = lowercase_path(directory);
    path.starts_with(directory)
}

#[cfg(windows)]
fn lowercase_path(path: &Path) -> PathBuf {
    path.components()
        .map(|component| match component {
            Component::Prefix(prefix) => {
                PathBuf::from(prefix.as_os_str().to_string_lossy().to_ascii_lowercase())
            }
            Component::RootDir => PathBuf::from(component.as_os_str()),
            Component::CurDir => PathBuf::from("."),
            Component::ParentDir => PathBuf::from(".."),
            Component::Normal(value) => PathBuf::from(value.to_string_lossy().to_ascii_lowercase()),
        })
        .collect()
}

#[cfg(not(windows))]
fn canonical_path_starts_with(path: &Path, directory: &Path) -> bool {
    path.starts_with(directory)
}
