use std::fs;
use std::path::{Path, PathBuf};

use dx_check_engine::model::{
    DxCheckEngineOptions, DxCheckEngineReport, DxSeverity, DxToolPlan, DxToolTarget,
    DxWebAuditResult,
};
use dx_check_engine::{adapters::plan_tools, analyze_project};
use serializer::{SerializerOutput, SerializerOutputConfig};
use tempfile::tempdir;

fn web_audit_plan(root: &tempfile::TempDir) -> DxToolPlan {
    plan_tools(root.path(), &[DxToolTarget::Audit])
        .into_iter()
        .find(|plan| plan.id == "web-audit-home")
        .expect("web audit plan")
}

#[test]
fn parses_web_audit_targets_from_extensionless_dx_config() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
name="demo"

web_audit_targets[id url required_status max_html_bytes](
home "http://localhost:3000/" 200 200000
docs "https://docs.example.com/guide" 200 300000
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(report.web_audit_targets.len(), 2);
    assert_eq!(report.web_audit_targets[0].id, "home");
    assert_eq!(report.web_audit_targets[0].url, "http://localhost:3000/");
    assert_eq!(report.web_audit_targets[0].required_status, Some(200));
    assert_eq!(report.web_audit_targets[0].max_html_bytes, Some(200000));
    assert_eq!(report.web_audit_targets[1].id, "docs");
    assert_eq!(
        report.web_audit_targets[1].url,
        "https://docs.example.com/guide"
    );
}

#[test]
fn invalid_web_audit_targets_emit_diagnostics_without_adapter_plans() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes](
home "ftp://example.com" 200 120000
home "http://example.com" 999 0
)
"#,
    )
    .unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert!(report.web_audit_targets.is_empty());
    assert!(
        !report
            .adapter_plans
            .iter()
            .any(|plan| { plan.target == DxToolTarget::Audit && plan.parser == "web-audit-json" })
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-audit-target-unsupported-url"
            && diagnostic.severity == DxSeverity::Failure
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-audit-target-duplicate-id"
            && diagnostic.severity == DxSeverity::Failure
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-audit-target-invalid-status"
            && diagnostic.severity == DxSeverity::Failure
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-audit-target-invalid-byte-limit"
            && diagnostic.severity == DxSeverity::Failure
    }));
}

#[test]
fn configured_web_audit_target_emits_safe_adapter_plan() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes](
home "http://localhost:3000/" 200 200000
)
"#,
    )
    .unwrap();

    let plan = web_audit_plan(&root);

    assert_eq!(plan.target, DxToolTarget::Audit);
    assert_eq!(plan.executable, "dx-check-web-audit");
    assert_eq!(plan.parser, "web-audit-json");
    assert_eq!(
        plan.args,
        [
            "--id",
            "home",
            "--url",
            "http://localhost:3000/",
            "--required-status",
            "200",
            "--max-html-bytes",
            "200000",
            "--lighthouse",
            "official"
        ]
    );
    assert_eq!(plan.detected_from, ["dx"]);
}

#[test]
fn official_lighthouse_target_without_dx_runtime_emits_pinned_repo_adapter_plan() {
    let root = tempdir().unwrap();
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
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let plan = web_audit_plan(&root);

    assert_eq!(plan.target, DxToolTarget::Audit);
    assert_eq!(plan.executable, "dx-check-web-audit");
    assert_eq!(plan.parser, "web-audit-json");
    assert!(
        plan.args
            .windows(2)
            .any(|pair| pair == ["--lighthouse", "official"])
    );
    assert!(plan.args.windows(2).any(|pair| {
        pair[0] == "--lighthouse-repo"
            && pair[1]
                .replace('\\', "/")
                .ends_with("third_party/google-lighthouse")
    }));
    assert_eq!(plan.detected_from, ["dx", "third_party/google-lighthouse"]);
}

#[test]
fn verified_dx_js_lighthouse_runtime_emits_command_adapter_plan() {
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
dx-js dx-js "{}" "{}" "{}" {} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
dx-js 0 js
dx-js 1 lighthouse
)
"#,
            command_receipt_path,
            cwd_receipt_path,
            command_receipt_path,
            hash
        ),
    )
    .unwrap();
    write_dx_js_lighthouse_equivalence_receipt(&root, &command);
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let plan = web_audit_plan(&root);
    let command_arg = command.display().to_string().replace('\\', "/");
    let cwd_arg = root.path().display().to_string().replace('\\', "/");

    assert!(
        plan.args
            .windows(2)
            .any(|pair| { pair[0] == "--lighthouse-command" && pair[1] == command_arg }),
        "plan args: {:#?}",
        plan.args
    );
    assert!(
        plan.args
            .windows(2)
            .any(|pair| { pair[0] == "--lighthouse-command-cwd" && pair[1] == cwd_arg })
    );
    assert!(
        plan.args
            .windows(2)
            .any(|pair| { pair[0] == "--lighthouse-command-arg" && pair[1] == "js" })
    );
    assert!(
        plan.args
            .windows(2)
            .any(|pair| { pair[0] == "--lighthouse-command-arg" && pair[1] == "lighthouse" })
    );
    assert!(!plan.args.iter().any(|arg| arg == "--lighthouse-repo"));
    assert_eq!(
        plan.detected_from,
        [
            "dx",
            ".dx/check/web-lighthouse-runtimes.sr",
            ".dx/check/web-lighthouse-equivalence.sr"
        ]
    );
}

#[test]
fn dx_js_lighthouse_runtime_requires_separate_equivalence_receipt() {
    let report = report_for_dx_js_lighthouse_runtime_args(&[(0, "js"), (1, "lighthouse")]);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-equivalence-receipt-missing");
}

#[test]
fn dx_js_lighthouse_runtime_rejects_stale_equivalence_receipt_hash() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    write_dx_js_lighthouse_equivalence_receipt_with_hash(&root, &"b".repeat(64), 1, true, true);
    write_pinned_lighthouse_repo(&root);
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-equivalence-hash-mismatch");
}

#[test]
fn dx_js_lighthouse_runtime_rejects_equivalence_receipt_without_samples() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_hash(&root, &hash, 0, true, true);
    write_pinned_lighthouse_repo(&root);
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-insufficient-samples",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_duplicate_runtime_rows() {
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
dx-js-backup dx-js "{0}" "{1}" "{0}" {2} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
dx-js 0 js
dx-js 1 lighthouse
dx-js-backup 0 js
dx-js-backup 1 lighthouse
)
"#,
            command_receipt_path, cwd_receipt_path, hash
        ),
    )
    .unwrap();
    write_dx_js_lighthouse_equivalence_receipt(&root, &command);
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-runtime-duplicate");
}

#[test]
fn dx_js_lighthouse_runtime_rejects_provider_id_mismatch() {
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
dx-build dx-js "{0}" "{1}" "{0}" {2} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
dx-build 0 js
dx-build 1 lighthouse
)
"#,
            command_receipt_path, cwd_receipt_path, hash
        ),
    )
    .unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("web-lighthouse-equivalence.sr"),
        format!(
            r#"
schema="dx.check.web_lighthouse_equivalence.v1"

web_lighthouse_equivalence[runtime_id provider executable_hash_blake3 status sample_count category_scores_match lhr_json_shape_match](
dx-build dx-js {hash} verified 1 true true
)
"#
        ),
    )
    .unwrap();
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-runtime-provider-id-mismatch",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_duplicate_equivalence_rows() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
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
dx-js dx-js {0} verified 1 true true
dx-js dx-js {0} verified 1 true true
)
"#,
            hash
        ),
    )
    .unwrap();
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-equivalence-duplicate");
}

#[test]
fn dx_js_lighthouse_runtime_requires_dx_build_machine_receipt_path() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_without_build_receipt_path(&root, &hash);
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-missing-build-receipt-path",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_stale_dx_build_machine_receipt_hash() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    let hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    let (receipt_path, _) = write_dx_build_lighthouse_machine_receipt(&root);
    write_dx_js_lighthouse_equivalence_receipt_with_build_receipt(
        &root,
        &hash,
        1,
        true,
        true,
        &receipt_path,
        &"d".repeat(64),
    );
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-hash-mismatch",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_dx_build_receipt_outside_serializer_path() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    let runtime_hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    let (_, build_receipt_hash) = write_dx_build_lighthouse_machine_receipt(&root);
    write_dx_js_lighthouse_equivalence_receipt_with_build_receipt(
        &root,
        &runtime_hash,
        1,
        true,
        true,
        "build/lighthouse-package.machine",
        &build_receipt_hash,
    );
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-invalid-build-receipt-path",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_dx_build_machine_receipt_for_wrong_package() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, &[(0, "js"), (1, "lighthouse")]);
    let runtime_hash = blake3::hash(&fs::read(&command).unwrap())
        .to_hex()
        .to_string();
    let (build_receipt_path, build_receipt_hash) = write_dx_build_machine_receipt(
        &root,
        "wrong-package",
        r#"
schema="dx.build.lighthouse_package.v1"
package="not-lighthouse"
status="verified"
package_assets_filesystem_addressable=true
dynamic_imports_runtime_compatible=true
node_builtins_runtime_compatible=true
chrome_launcher_unstubbed=true
"#,
    );
    write_dx_js_lighthouse_equivalence_receipt_with_build_receipt(
        &root,
        &runtime_hash,
        1,
        true,
        true,
        &build_receipt_path,
        &build_receipt_hash,
    );
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-equivalence-package-proof-machine-package-mismatch",
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_contract_metadata_args() {
    let report = report_for_dx_js_lighthouse_runtime_args(&[
        (0, "js"),
        (1, "lighthouse"),
        (2, "--contract"),
        (3, "--json"),
    ]);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-runtime-invalid-args");
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "web-lighthouse-runtime-invalid-args"
            && diagnostic.message.contains("js lighthouse")
    }));
}

#[test]
fn dx_js_lighthouse_runtime_requires_exact_command_args() {
    let report = report_for_dx_js_lighthouse_runtime_args(&[(0, "lighthouse"), (1, "js")]);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-runtime-invalid-args");
}

#[test]
fn dx_js_lighthouse_runtime_rejects_missing_and_unsafe_extra_args() {
    let invalid_arg_sets = [
        Vec::new(),
        vec![(0, "js"), (1, "lighthouse"), (2, "--help")],
        vec![(0, "js"), (1, "lighthouse"), (2, "https://example.com/")],
        vec![(0, "js"), (1, "lighthouse"), (2, "--output=json")],
    ];

    for runtime_args in invalid_arg_sets {
        let report = report_for_dx_js_lighthouse_runtime_args(&runtime_args);

        assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-runtime-invalid-args");
    }
}

#[test]
fn dx_js_lighthouse_runtime_rejects_sparse_arg_positions() {
    let report = report_for_dx_js_lighthouse_runtime_args(&[(4, "js"), (5, "lighthouse")]);

    assert_rejected_dx_js_lighthouse_runtime(&report, "web-lighthouse-runtime-arg-position-gap");
}

#[test]
fn dx_js_lighthouse_runtime_rejects_duplicate_arg_position_even_if_first_rows_match() {
    let report = report_for_dx_js_lighthouse_runtime_args(&[
        (0, "js"),
        (1, "lighthouse"),
        (1, "--contract"),
    ]);

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-runtime-arg-duplicate-position",
    );
}

#[test]
fn stale_dx_js_lighthouse_runtime_hash_does_not_emit_command_plan() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::write(&command, b"new executable bytes").unwrap();
    let command_receipt_path = command.display().to_string().replace('\\', "/");
    let cwd_receipt_path = root.path().display().to_string().replace('\\', "/");
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
dx-js dx-js "{}" "{}" "{}" {} proven_bundle verified
)
"#,
            command_receipt_path,
            cwd_receipt_path,
            command_receipt_path,
            "a".repeat(64)
        ),
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();
    let plan = report
        .adapter_plans
        .iter()
        .find(|plan| plan.id == "web-audit-home")
        .expect("web audit plan");

    assert!(!plan.args.iter().any(|arg| arg == "--lighthouse-command"));
    assert!(plan.args.iter().any(|arg| arg == "--lighthouse-repo"));
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "web-lighthouse-runtime-hash-stale"
                && diagnostic.severity == DxSeverity::Failure
        }),
        "diagnostics: {:#?}",
        report.diagnostics
    );
}

#[test]
fn dx_js_lighthouse_runtime_rejects_command_executable_mismatch() {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    let executable = root.path().join("verified").join("dx.exe");
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&command, b"unverified runtime bytes").unwrap();
    fs::write(&executable, b"verified runtime bytes").unwrap();
    write_dx_js_lighthouse_runtime_receipt_with_executable(
        &root,
        &command,
        &executable,
        &[(0, "js"), (1, "lighthouse")],
    );
    write_pinned_lighthouse_repo(&root);
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert_rejected_dx_js_lighthouse_runtime(
        &report,
        "web-lighthouse-runtime-command-executable-mismatch",
    );
}

#[test]
fn dx_js_lighthouse_runtime_accepts_canonical_command_executable_match() {
    let root = tempdir().unwrap();
    let executable = root.path().join("tools").join("dx.exe");
    fs::create_dir_all(executable.parent().unwrap()).unwrap();
    fs::write(&executable, b"dx js lighthouse").unwrap();
    let command = root
        .path()
        .join("tools")
        .join("..")
        .join("tools")
        .join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt_with_executable(
        &root,
        &command,
        &executable,
        &[(0, "js"), (1, "lighthouse")],
    );
    write_dx_js_lighthouse_equivalence_receipt(&root, &executable);
    write_pinned_lighthouse_repo(&root);
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes lighthouse](
home "https://example.com/" 200 200000 official
)
"#,
    )
    .unwrap();

    let plan = web_audit_plan(&root);
    let command_arg = command.display().to_string().replace('\\', "/");

    assert!(
        plan.args
            .windows(2)
            .any(|pair| { pair[0] == "--lighthouse-command" && pair[1] == command_arg }),
        "plan args: {:#?}",
        plan.args
    );
    assert!(!plan.args.iter().any(|arg| arg == "--lighthouse-repo"));
    assert_eq!(
        plan.detected_from,
        [
            "dx",
            ".dx/check/web-lighthouse-runtimes.sr",
            ".dx/check/web-lighthouse-equivalence.sr"
        ]
    );
}

fn write_dx_js_lighthouse_runtime_receipt(
    root: &tempfile::TempDir,
    command: &Path,
    runtime_args: &[(u64, &str)],
) {
    fs::create_dir_all(command.parent().unwrap()).unwrap();
    fs::write(command, b"dx js lighthouse").unwrap();
    write_dx_js_lighthouse_runtime_receipt_with_executable(root, command, command, runtime_args);
}

fn write_dx_js_lighthouse_runtime_receipt_with_executable(
    root: &tempfile::TempDir,
    command: &Path,
    executable: &Path,
    runtime_args: &[(u64, &str)],
) {
    let command_receipt_path = command.display().to_string().replace('\\', "/");
    let executable_receipt_path = executable.display().to_string().replace('\\', "/");
    let cwd_receipt_path = root.path().display().to_string().replace('\\', "/");
    let hash = blake3::hash(&fs::read(executable).unwrap())
        .to_hex()
        .to_string();
    let runtime_arg_rows = runtime_args
        .iter()
        .map(|(position, arg)| format!("dx-js {position} {arg}"))
        .collect::<Vec<_>>()
        .join("\n");
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
dx-js dx-js "{}" "{}" "{}" {} proven_bundle verified
)

web_lighthouse_runtime_args[runtime_id position arg](
{}
)
"#,
            command_receipt_path, cwd_receipt_path, executable_receipt_path, hash, runtime_arg_rows
        ),
    )
    .unwrap();
}

fn write_dx_js_lighthouse_equivalence_receipt(root: &tempfile::TempDir, executable: &Path) {
    let hash = blake3::hash(&fs::read(executable).unwrap())
        .to_hex()
        .to_string();
    write_dx_js_lighthouse_equivalence_receipt_with_hash(root, &hash, 1, true, true);
}

fn write_dx_js_lighthouse_equivalence_receipt_with_hash(
    root: &tempfile::TempDir,
    hash: &str,
    sample_count: u64,
    category_scores_match: bool,
    lhr_json_shape_match: bool,
) {
    let (build_receipt_path, build_receipt_hash) = write_dx_build_lighthouse_machine_receipt(root);
    write_dx_js_lighthouse_equivalence_receipt_with_build_receipt(
        root,
        hash,
        sample_count,
        category_scores_match,
        lhr_json_shape_match,
        &build_receipt_path,
        &build_receipt_hash,
    );
}

fn write_dx_js_lighthouse_equivalence_receipt_with_build_receipt(
    root: &tempfile::TempDir,
    hash: &str,
    sample_count: u64,
    category_scores_match: bool,
    lhr_json_shape_match: bool,
    build_receipt_path: &str,
    build_receipt_hash: &str,
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

web_lighthouse_package_proofs[runtime_id provider package_name status build_receipt_path build_receipt_hash_blake3 package_assets_filesystem_addressable dynamic_imports_runtime_compatible node_builtins_runtime_compatible chrome_launcher_unstubbed](
dx-js dx-build lighthouse verified "{}" {} true true true true
)
"#,
            hash,
            sample_count,
            category_scores_match,
            lhr_json_shape_match,
            build_receipt_path,
            build_receipt_hash
        ),
    )
    .unwrap();
}

fn write_dx_js_lighthouse_equivalence_receipt_without_build_receipt_path(
    root: &tempfile::TempDir,
    hash: &str,
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
dx-js dx-js {hash} verified 1 true true
)

web_lighthouse_package_proofs[runtime_id provider package_name status build_receipt_hash_blake3 package_assets_filesystem_addressable dynamic_imports_runtime_compatible node_builtins_runtime_compatible chrome_launcher_unstubbed](
dx-js dx-build lighthouse verified {} true true true true
)
"#,
            "c".repeat(64)
        ),
    )
    .unwrap();
}

fn write_dx_build_lighthouse_machine_receipt(root: &tempfile::TempDir) -> (String, String) {
    write_dx_build_machine_receipt(
        root,
        "lighthouse-package",
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
}

fn write_dx_build_machine_receipt(
    root: &tempfile::TempDir,
    name: &str,
    body: &str,
) -> (String, String) {
    let source = root
        .path()
        .join(".dx")
        .join("build")
        .join(format!("{name}.sr"));
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, body).unwrap();
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

fn report_for_dx_js_lighthouse_runtime_args(runtime_args: &[(u64, &str)]) -> DxCheckEngineReport {
    let root = tempdir().unwrap();
    let command = root.path().join("tools").join("dx.exe");
    write_dx_js_lighthouse_runtime_receipt(&root, &command, runtime_args);
    write_pinned_lighthouse_repo(&root);
    write_web_audit_target(&root);

    analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Audit],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap()
}

fn write_web_audit_target(root: &tempfile::TempDir) {
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

fn write_pinned_lighthouse_repo(root: &tempfile::TempDir) {
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

#[test]
fn web_audit_result_json_produces_rule_findings() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(root.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(
        root.path().join(".dx").join("check").join("web.sr"),
        r#"
rule_pack(id=web-check version=1 title=WebCheck kind=dx-check-rule-pack)

categories[id label weight](
web-performance "Web Performance" 100
)

rules[id category severity weight metric op threshold docs provenance](
web-http-status web-performance failure 15 web_http_status eq 200 docs/check/web.md local
web-security-headers web-performance warning 10 web_security_header_count min 3 docs/check/web.md local
web-title-present web-performance warning 8 web_title_present min 1 docs/check/web.md local
web-html-budget web-performance warning 7 web_html_bytes max 200000 docs/check/web.md local
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes](
home "http://localhost:3000/" 200 200000
)

web_audit_results[id target_id url status html_bytes title_present description_present security_header_count source](
home-run home "http://localhost:3000/" 404 250000 0 1 1 ".dx/receipts/check/web-home.json"
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(report.web_audit_results.len(), 1);
    assert!(report.findings.iter().any(|finding| {
        finding.id == "web-http-status"
            && finding.category == "web-performance"
            && finding.actual.as_deref() == Some("404")
            && finding.threshold.as_deref() == Some("200")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "web-security-headers"
            && finding.actual.as_deref() == Some("1")
            && finding.threshold.as_deref() == Some("3")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "web-title-present" && finding.actual.as_deref() == Some("0")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "web-html-budget" && finding.actual.as_deref() == Some("250000")
    }));
}

#[test]
fn invalid_web_audit_result_status_reports_result_diagnostic() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
web_audit_targets[id url required_status max_html_bytes](
home "http://localhost:3000/" 200 200000
)

web_audit_results[id target_id url status html_bytes title_present description_present security_header_count source](
home-run home "http://localhost:3000/" 99 1000 1 1 3 ".dx/receipts/check/web-home.json"
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "web-audit-result-invalid-status"),
        "invalid imported web audit result status should identify the result row, not the target config: {:#?}",
        report.diagnostics
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "web-audit-target-invalid-status"),
        "result status validation must not reuse the target diagnostic id: {:#?}",
        report.diagnostics
    );
}

#[test]
fn legacy_engine_report_json_defaults_web_audit_fields() {
    let report: dx_check_engine::DxCheckEngineReport = serde_json::from_str(
        r#"{
  "rule_packs": [],
  "findings": [],
  "diagnostics": [],
  "adapter_plans": [],
  "test_inventory": {
    "rust_tests": 0,
    "js_tests": 0,
    "python_tests": 0,
    "go_tests": 0
  },
  "checked_paths": []
}"#,
    )
    .expect("legacy report");

    assert!(report.web_audit_targets.is_empty());
    assert!(report.web_audit_results.is_empty());
}

#[test]
fn web_audit_result_keeps_source_path_for_zed_receipts() {
    let result = DxWebAuditResult {
        id: "home-run".to_string(),
        target_id: "home".to_string(),
        url: "http://localhost:3000/".to_string(),
        status: Some(200),
        final_url: Some("http://localhost:3000/".to_string()),
        response_time_ms: Some(42),
        html_bytes: Some(18200),
        title_present: true,
        description_present: true,
        canonical_present: true,
        viewport_present: true,
        security_header_count: 4,
        source: Some(PathBuf::from(".dx/receipts/check/web-home.json")),
    };

    let encoded = serde_json::to_string(&result).expect("result json");

    assert!(encoded.contains(".dx/receipts/check/web-home.json"));
}
