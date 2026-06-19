use std::path::Path;

use crate::model::DxDiagnostic;

use super::super::super::{
    bool_cell, column, config_diagnostic, row_diagnostic, row_label, string_cell,
};
use super::super::hash_blake3_is_valid;
use super::equivalence_cell;

mod build_receipt;

const DX_BUILD_PROVIDER: &str = "dx-build";
const LIGHTHOUSE_PACKAGE_NAME: &str = "lighthouse";

pub(super) fn verifies_runtime(
    document: &serializer::DxDocument,
    source: &Path,
    root: &Path,
    runtime_id: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let Some(section) = document.section_by_name("web_lighthouse_package_proofs") else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-package-proof-missing",
            source,
            format!("DX JS Lighthouse runtime `{runtime_id}` is missing DX Build package proof"),
            "Add a web_lighthouse_package_proofs table proving DX Build packaged Lighthouse without breaking runtime assets.",
        ));
        return false;
    };
    let Some(columns) = PackageProofColumns::read(section, source, diagnostics) else {
        return false;
    };

    let mut matching_rows = Vec::new();
    for (row_index, row) in section.rows.iter().enumerate() {
        let current_row_label = row_label(row_index);
        let Some(row_runtime_id) = equivalence_cell(
            row.get(columns.runtime),
            source,
            &current_row_label,
            "runtime_id",
            diagnostics,
        ) else {
            continue;
        };
        if row_runtime_id == runtime_id {
            matching_rows.push(row_index);
        }
    }

    if matching_rows.len() > 1 {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-duplicate",
            source,
            &row_label(matching_rows[1]),
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` has multiple DX Build package proof rows"
            ),
            "Keep exactly one package proof row for each promoted DX JS Lighthouse runtime.",
        ));
        return false;
    }

    let Some(row_index) = matching_rows.into_iter().next() else {
        diagnostics.push(config_diagnostic(
            "web-lighthouse-equivalence-package-proof-missing",
            source,
            format!(
                "Web Lighthouse equivalence receipt has no DX Build package proof row for runtime `{runtime_id}`"
            ),
            "Add a web_lighthouse_package_proofs row matching the promoted dx-js runtime id.",
        ));
        return false;
    };

    row_verifies_package_proof(
        &section.rows[row_index],
        source,
        root,
        &row_label(row_index),
        runtime_id,
        &columns,
        diagnostics,
    )
}

struct PackageProofColumns {
    runtime: usize,
    provider: usize,
    package_name: usize,
    status: usize,
    build_receipt_path: usize,
    build_receipt_hash: usize,
    package_assets_filesystem_addressable: usize,
    dynamic_imports_runtime_compatible: usize,
    node_builtins_runtime_compatible: usize,
    chrome_launcher_unstubbed: usize,
}

impl PackageProofColumns {
    fn read(
        section: &serializer::DxSection,
        source: &Path,
        diagnostics: &mut Vec<DxDiagnostic>,
    ) -> Option<Self> {
        Some(Self {
            runtime: column(section, "runtime_id", source, diagnostics)?,
            provider: column(section, "provider", source, diagnostics)?,
            package_name: column(section, "package_name", source, diagnostics)?,
            status: column(section, "status", source, diagnostics)?,
            build_receipt_path: build_receipt_path_column(section, source, diagnostics)?,
            build_receipt_hash: column(section, "build_receipt_hash_blake3", source, diagnostics)?,
            package_assets_filesystem_addressable: column(
                section,
                "package_assets_filesystem_addressable",
                source,
                diagnostics,
            )?,
            dynamic_imports_runtime_compatible: column(
                section,
                "dynamic_imports_runtime_compatible",
                source,
                diagnostics,
            )?,
            node_builtins_runtime_compatible: column(
                section,
                "node_builtins_runtime_compatible",
                source,
                diagnostics,
            )?,
            chrome_launcher_unstubbed: column(
                section,
                "chrome_launcher_unstubbed",
                source,
                diagnostics,
            )?,
        })
    }
}

fn row_verifies_package_proof(
    row: &[serializer::DxLlmValue],
    source: &Path,
    root: &Path,
    row_label: &str,
    runtime_id: &str,
    columns: &PackageProofColumns,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let Some(provider) = equivalence_cell(
        row.get(columns.provider),
        source,
        row_label,
        "provider",
        diagnostics,
    ) else {
        return false;
    };
    if provider != DX_BUILD_PROVIDER {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-provider-mismatch",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` package proof uses provider `{provider}` instead of `{DX_BUILD_PROVIDER}`"
            ),
            "Regenerate the package proof from the DX Build Lighthouse packaging lane.",
        ));
        return false;
    }

    let Some(package_name) = equivalence_cell(
        row.get(columns.package_name),
        source,
        row_label,
        "package_name",
        diagnostics,
    ) else {
        return false;
    };
    if package_name != LIGHTHOUSE_PACKAGE_NAME {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-package-mismatch",
            source,
            row_label,
            format!(
                "DX JS Lighthouse runtime `{runtime_id}` package proof is for `{package_name}` instead of `{LIGHTHOUSE_PACKAGE_NAME}`"
            ),
            "Prove the official Lighthouse package specifically before promoting the runtime.",
        ));
        return false;
    }

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
            "web-lighthouse-equivalence-package-proof-unverified",
            source,
            row_label,
            format!("DX Build Lighthouse package proof status is `{status}`"),
            "Use status verified only after DX Build packaging behavior is proven.",
        ));
        return false;
    }

    let Some(build_receipt_path) = package_proof_cell(
        row.get(columns.build_receipt_path),
        source,
        row_label,
        "build_receipt_path",
        "web-lighthouse-equivalence-package-proof-missing-build-receipt-path",
        diagnostics,
    ) else {
        return false;
    };
    let Some(build_receipt_hash) = equivalence_cell(
        row.get(columns.build_receipt_hash),
        source,
        row_label,
        "build_receipt_hash_blake3",
        diagnostics,
    ) else {
        return false;
    };
    if !hash_blake3_is_valid(&build_receipt_hash) {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-invalid-hash",
            source,
            row_label,
            "DX Build Lighthouse package proof build_receipt_hash_blake3 is not a 64-character hex digest",
            "Regenerate the package proof with the DX Build Lighthouse receipt hash.",
        ));
        return false;
    }
    if !build_receipt::verifies(
        root,
        source,
        row_label,
        runtime_id,
        &build_receipt_path,
        &build_receipt_hash,
        diagnostics,
    ) {
        return false;
    }

    required_package_proof_bool(
        row.get(columns.package_assets_filesystem_addressable),
        source,
        row_label,
        "package_assets_filesystem_addressable",
        diagnostics,
    ) && required_package_proof_bool(
        row.get(columns.dynamic_imports_runtime_compatible),
        source,
        row_label,
        "dynamic_imports_runtime_compatible",
        diagnostics,
    ) && required_package_proof_bool(
        row.get(columns.node_builtins_runtime_compatible),
        source,
        row_label,
        "node_builtins_runtime_compatible",
        diagnostics,
    ) && required_package_proof_bool(
        row.get(columns.chrome_launcher_unstubbed),
        source,
        row_label,
        "chrome_launcher_unstubbed",
        diagnostics,
    )
}

fn required_package_proof_bool(
    value: Option<&serializer::DxLlmValue>,
    source: &Path,
    row_label: &str,
    field: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> bool {
    let Some(value) = bool_cell(value) else {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-invalid-boolean",
            source,
            row_label,
            format!("DX Build Lighthouse package proof `{field}` must be a boolean"),
            "Regenerate the package proof with true or false boolean values.",
        ));
        return false;
    };
    if !value {
        diagnostics.push(row_diagnostic(
            "web-lighthouse-equivalence-package-proof-unverified",
            source,
            row_label,
            format!("DX Build Lighthouse package proof `{field}` is false"),
            "Do not promote DX JS Lighthouse until DX Build preserves package assets, dynamic imports, Node built-ins, and Chrome launcher behavior.",
        ));
        return false;
    }
    true
}

fn build_receipt_path_column(
    section: &serializer::DxSection,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<usize> {
    match section.column_index("build_receipt_path") {
        Some(index) => Some(index),
        None => {
            diagnostics.push(config_diagnostic(
                "web-lighthouse-equivalence-package-proof-missing-build-receipt-path",
                source,
                "DX Build Lighthouse package proof is missing build_receipt_path",
                "Record the project-relative .dx/serializer/*.machine receipt produced by DX Build packaging.",
            ));
            None
        }
    }
}

fn package_proof_cell(
    value: Option<&serializer::DxLlmValue>,
    source: &Path,
    row_label: &str,
    field: &str,
    missing_id: &str,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Option<String> {
    match string_cell(value) {
        Some(value) => Some(value),
        None => {
            diagnostics.push(row_diagnostic(
                missing_id,
                source,
                row_label,
                format!("DX Build Lighthouse package proof is missing `{field}`"),
                "Regenerate the package proof with the DX Build machine receipt path.",
            ));
            None
        }
    }
}
