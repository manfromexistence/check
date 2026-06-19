use std::fs;
use std::path::Path;

use dx_check_engine::{
    DxCheckEngineOptions, DxCheckEngineReport, DxSeverity, DxToolTarget, analyze_project,
};
use serializer::{SerializerOutput, SerializerOutputConfig};
use tempfile::{TempDir, tempdir};

#[test]
fn dx_js_lighthouse_runtime_rejects_shadow_provider_claims() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::write(&command, b"dx js lighthouse").unwrap();
    let command_receipt_path = command.display().to_string().replace('\\', "/");
    let cwd_receipt_path = root.path().display().to_string().replace('\\', "/");
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-runtimes.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_runtimes.v1"

web_lighthouse_runtimes[id provider command cwd executable hash_blake3 claim_status equivalence_status](
dx-js dx-js "{0}" "{1}" "{0}" {2} proven_bundle verified
dx-build dx-build "{0}" "{1}" "{0}" {2} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
dx-js 0 js
dx-js 1 lighthouse
dx-build 0 js
dx-build 1 lighthouse
)
"#,
            command_receipt_path, cwd_receipt_path, hash
        ),
    )
    .unwrap();
    write_dx_js_lighthouse_equivalence_receipt(&root, &command);
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-runtime-unsupported-provider",
    );
}

#[test]
fn dx_js_lighthouse_equivalence_reports_provider_mismatch() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-equivalence.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_equivalence.v1"

web_lighthouse_equivalence[runtime_id provider executable_hash_blake3 status sample_count category_scores_match lhr_json_shape_match](
dx-js dx-build {hash} verified 1 true true
)
"#
        ),
    )
    .unwrap();
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-provider-mismatch",
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-equivalence-provider-mismatch"
            && diagnostic.message.contains("dx-build")
            && diagnostic.message.contains("dx-js")
    }));
}

#[test]
fn dx_js_lighthouse_equivalence_rejects_invalid_category_boolean() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_cells(&root, &hash, "1", "maybe", "true");
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-equivalence-invalid-boolean");
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-equivalence-invalid-boolean"
            && diagnostic.message.contains("category_scores_match")
    }));
}

#[test]
fn dx_js_lighthouse_equivalence_rejects_invalid_lhr_shape_boolean() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_cells(&root, &hash, "1", "true", "maybe");
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-equivalence-invalid-boolean");
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-equivalence-invalid-boolean"
            && diagnostic.message.contains("lhr_json_shape_match")
    }));
}

#[test]
fn dx_js_lighthouse_equivalence_requires_dx_build_package_proof() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_without_package_proof(
        &root, &hash, "1", "true", "true",
    );
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-missing",
    );
}

#[test]
fn dx_js_lighthouse_equivalence_rejects_unverified_dx_build_package_proof() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_package_proof(
        &root,
        &hash,
        "1",
        "true",
        "true",
        PackageProofCells {
            dynamic_imports_runtime_compatible: "false",
            ..PackageProofCells::default()
        },
    );
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = audit_report(&root);

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-unverified",
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-equivalence-package-proof-unverified"
            && diagnostic
                .message
                .contains("dynamic_imports_runtime_compatible")
    }));
}

fn audit_report(root: &TempDir) -> DxCheckEngineReport {
    analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap()
}

fn write_dx_js_lighthouse_runtime_receipt(root: &TempDir, command: &Path) {
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::write(command, b"dx js lighthouse").unwrap();
    let command_receipt_path = command.display().to_string().replace('\\', "/");
    let cwd_receipt_path = root.path().display().to_string().replace('\\', "/");
    let hash = blake3::hash(&fs::read(command).unwrap())
        .to_hex()
        .to_string();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-runtimes.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_runtimes.v1"

web_lighthouse_runtimes[id provider command cwd executable hash_blake3 claim_status equivalence_status](
dx-js dx-js "{0}" "{1}" "{0}" {2} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
dx-js 0 js
dx-js 1 lighthouse
)
"#,
            command_receipt_path, cwd_receipt_path, hash
        ),
    )
    .unwrap();
}

fn write_dx_js_lighthouse_equivalence_receipt(root: &TempDir, executable: &Path) {
    let hash = blake3::hash(&fs::read(executable).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_cells(root, &hash, "1", "true", "true");
}

fn write_dx_js_lighthouse_equivalence_receipt_with_cells(
    root: &TempDir,
    hash: &str,
    sample_count: &str,
    category_scores_match: &str,
    lhr_json_shape_match: &str,
) {
    write_dx_js_lighthouse_equivalence_receipt_with_package_proof(
        root,
        hash,
        sample_count,
        category_scores_match,
        lhr_json_shape_match,
        PackageProofCells::default(),
    );
}

fn write_dx_js_lighthouse_equivalence_receipt_without_package_proof(
    root: &TempDir,
    hash: &str,
    sample_count: &str,
    category_scores_match: &str,
    lhr_json_shape_match: &str,
) {
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-equivalence.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_equivalence.v1"

web_lighthouse_equivalence[runtime_id provider executable_hash_blake3 status sample_count category_scores_match lhr_json_shape_match](
dx-js dx-js {} verified {} {} {}
)
"#,
            hash, sample_count, category_scores_match, lhr_json_shape_match
        ),
    )
    .unwrap();
}

fn write_dx_js_lighthouse_equivalence_receipt_with_package_proof(
    root: &TempDir,
    hash: &str,
    sample_count: &str,
    category_scores_match: &str,
    lhr_json_shape_match: &str,
    package_proof: PackageProofCells,
) {
    let (build_receipt_path, build_receipt_hash) = write_dx_build_lighthouse_machine_receipt(root);
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-equivalence.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_equivalence.v1"

web_lighthouse_equivalence[runtime_id provider executable_hash_blake3 status sample_count category_scores_match lhr_json_shape_match](
dx-js dx-js {} verified {} {} {}
)

web_lighthouse_package_proofs[runtime_id provider package_name status build_receipt_path build_receipt_hash_blake3 package_assets_filesystem_addressable dynamic_imports_runtime_compatible node_builtins_runtime_compatible chrome_launcher_unstubbed](
dx-js dx-build lighthouse verified "{}" {} {} {} {} {}
)
"#,
            hash,
            sample_count,
            category_scores_match,
            lhr_json_shape_match,
            build_receipt_path,
            build_receipt_hash,
            package_proof.package_assets_filesystem_addressable,
            package_proof.dynamic_imports_runtime_compatible,
            package_proof.node_builtins_runtime_compatible,
            package_proof.chrome_launcher_unstubbed
        ),
    )
    .unwrap();
}

fn write_dx_build_lighthouse_machine_receipt(root: &TempDir) -> (String, String) {
    let source = root
        .path()
        .join(".dx")
        .join("build")
        .join("lighthouse-package.sr");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        r#"
schema="dx.build.lighthouse_package.v1"
package="lighthouse"
status="verified"
package_assets_filesystem_addressable=true
dynamic_imports_runtime_compatible=true
node_builtins_runtime_compatible=true
chrome_launcher_unstubbed=true
"#,
    )
    .unwrap();
    let serializer = SerializerOutput::with_config(
        SerializerOutputConfig::new()
            .with_output_dir(root.path().join(".dx").join("serializer"))
            .with_llm(false)
            .with_machine(true),
    );
    let output = serializer
        .process_file(&source)
        .expect("generate DX Build Lighthouse package machine receipt");
    let machine = output.paths.machine;
    let relative_machine = machine
        .strip_prefix(root.path())
        .expect("machine receipt inside project")
        .display()
        .to_string()
        .replace('\\', "/");
    let hash = blake3::hash(&fs::read(machine).unwrap())
        .to_hex()
        .to_string();

    (relative_machine, hash)
}

#[derive(Clone, Copy)]
struct PackageProofCells {
    package_assets_filesystem_addressable: &'static str,
    dynamic_imports_runtime_compatible: &'static str,
    node_builtins_runtime_compatible: &'static str,
    chrome_launcher_unstubbed: &'static str,
}

impl Default for PackageProofCells {
    fn default() -> Self {
        Self {
            package_assets_filesystem_addressable: "true",
            dynamic_imports_runtime_compatible: "true",
            node_builtins_runtime_compatible: "true",
            chrome_launcher_unstubbed: "true",
        }
    }
}

fn write_pinned_lighthouse_repo(root: &TempDir) {
    let lighthouse_cli = root
        .path()
        .join("third_party")
        .join("google-lighthouse")
        .join("cli");
    fs::create_dir_all(&lighthouse_cli).unwrap();
    fs::write(
        lighthouse_cli.join("index.js"),
        "console.log('lighthouse');\n",
    )
    .unwrap();
}

fn write_web_audit_target(root: &TempDir) {
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();
}

fn assert_rejected_dx_js_lighthouse_runtime(report: &DxCheckEngineReport, diagnostic_id: &str) {
    let plan = report
        .adapter_plans
        .iter()
        .find(|plan| plan.id == "web-audit-home")
        .expect("web audit plan");

    assert!(!plan.args.iter().any(|arg| arg == "--lighthouse-command"));
    assert!(plan.args.iter().any(|arg| arg == "--lighthouse-repo"));
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == diagnostic_id && diagnostic.severity == DxSeverity::Failure
        }),
        "diagnostics: {:#?}",
        report.diagnostics
    );
}
