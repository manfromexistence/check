use std::{fs, path::Path};

use dx_check_engine::adapters::plan_tools;
use dx_check_engine::model::{DxToolPlan, DxToolTarget};
use tempfile::tempdir;

fn assert_blocked_javascript_plan<'a>(
    plans: &'a [DxToolPlan],
    target: DxToolTarget,
    id: &str,
    expected_reason_parts: &[&str],
) -> &'a DxToolPlan {
    let plan = plans
        .iter()
        .find(|plan| plan.id == id && plan.target == target)
        .unwrap_or_else(|| panic!("{id} should be visible as a blocked JavaScript plan"));

    assert_eq!(plan.executable, "dx-check-blocked", "{id}");
    assert_eq!(plan.parser, "blocked", "{id}");
    for expected in expected_reason_parts {
        assert!(
            plan.args.iter().any(|arg| arg.contains(expected)),
            "{id} blocked reason should contain `{expected}`: {:?}",
            plan.args
        );
    }

    plan
}

fn assert_blocked_javascript_target(plans: &[DxToolPlan], target: DxToolTarget) -> &DxToolPlan {
    assert_blocked_javascript_target_for_case(plans, target, javascript_plan_id(target))
}

fn assert_blocked_javascript_target_for_case<'a>(
    plans: &'a [DxToolPlan],
    target: DxToolTarget,
    case: &str,
) -> &'a DxToolPlan {
    let id = javascript_plan_id(target);
    let plan = plans
        .iter()
        .find(|plan| plan.id == id && plan.target == target)
        .unwrap_or_else(|| panic!("{case}: {id} should be visible as a blocked JavaScript plan"));

    assert_eq!(plan.executable, "dx-check-blocked", "{case}: {id}");
    assert_eq!(plan.parser, "blocked", "{case}: {id}");
    for expected in ["package.json", "not safe for dx check"] {
        assert!(
            plan.args.iter().any(|arg| arg.contains(expected)),
            "{case}: {id} blocked reason should contain `{expected}`: {:?}",
            plan.args
        );
    }

    plan
}

fn javascript_plan_id(target: DxToolTarget) -> &'static str {
    match target {
        DxToolTarget::Lint => "js-lint",
        DxToolTarget::Format => "js-format:check",
        DxToolTarget::Typecheck => "js-typecheck",
        DxToolTarget::Test => "js-test",
        DxToolTarget::Audit => "js-audit",
    }
}

fn write_biome_package_with_javascript_fallbacks(root: &Path) {
    fs::write(
        root.join("package.json"),
        r#"{
  "scripts": {
"lint": "eslint .",
"format:check": "prettier --check ."
  },
  "devDependencies": {
"@biomejs/biome": "^2.4.16",
"eslint": "^9.0.0",
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();
}

#[test]
fn biome_dx_config_option_like_paths_block_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("--write")).unwrap();
    fs::write(
        root.path().join("--write").join("app.ts"),
        "export const app = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint "--write" true
format "--write" true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "devDependencies": {
"@biomejs/biome": "^2.4.16"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "biome-lint")
        .expect("blocked Biome lint plan");
    let format = plans
        .iter()
        .find(|plan| plan.id == "biome-format")
        .expect("blocked Biome format plan");

    assert_eq!(lint.executable, "dx-check-blocked");
    assert_eq!(format.executable, "dx-check-blocked");
    assert!(
        lint.args
            .iter()
            .any(|arg| arg.contains("path cannot look like a command-line option")),
        "lint should explain the invalid option-like path: {:?}",
        lint.args
    );
    assert!(
        format
            .args
            .iter()
            .any(|arg| arg.contains("path cannot look like a command-line option")),
        "format should explain the invalid option-like path: {:?}",
        format.args
    );
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn biome_dx_config_normalized_option_like_paths_block_javascript_fallback_plans() {
    for path in ["./--write", ".//--write"] {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("--write")).unwrap();
        fs::write(
            root.path().join("--write").join("app.ts"),
            "export const app = true;\n",
        )
        .unwrap();
        fs::write(
            root.path().join("dx"),
            format!(
                r#"
biome[target path enabled](
lint "{path}" true
format "{path}" true
)
"#
            ),
        )
        .unwrap();
        fs::write(
            root.path().join("package.json"),
            r#"{
  "devDependencies": {
"@biomejs/biome": "^2.4.16"
  }
}
"#,
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);
        let lint = plans
            .iter()
            .find(|plan| plan.id == "biome-lint")
            .expect("blocked Biome lint plan");
        let format = plans
            .iter()
            .find(|plan| plan.id == "biome-format")
            .expect("blocked Biome format plan");

        assert_eq!(lint.executable, "dx-check-blocked");
        assert_eq!(format.executable, "dx-check-blocked");
        assert!(
            lint.args
                .iter()
                .any(|arg| arg.contains("path cannot look like a command-line option")),
            "{path} lint should explain the invalid normalized option-like path: {:?}",
            lint.args
        );
        assert!(
            format
                .args
                .iter()
                .any(|arg| arg.contains("path cannot look like a command-line option")),
            "{path} format should explain the invalid normalized option-like path: {:?}",
            format.args
        );
        assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
        assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
    }
}

#[test]
fn biome_dx_config_reserved_directory_targets_block_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("node_modules").join("package")).unwrap();
    fs::create_dir_all(root.path().join("dist")).unwrap();
    fs::write(
        root.path()
            .join("node_modules")
            .join("package")
            .join("app.ts"),
        "export const dependency = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("dist").join("app.ts"),
        "export const generated = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint node_modules true
format dist true
)
"#,
    )
    .unwrap();
    write_biome_package_with_javascript_fallbacks(root.path());

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "biome-lint")
        .expect("blocked Biome lint plan");
    let format = plans
        .iter()
        .find(|plan| plan.id == "biome-format")
        .expect("blocked Biome format plan");

    assert_eq!(lint.executable, "dx-check-blocked");
    assert_eq!(format.executable, "dx-check-blocked");
    assert!(
        lint.args
            .iter()
            .any(|arg| arg.contains("node_modules") && arg.contains("generated/dependency")),
        "lint should explain reserved dependency directories: {:?}",
        lint.args
    );
    assert!(
        format
            .args
            .iter()
            .any(|arg| arg.contains("dist") && arg.contains("generated/dependency")),
        "format should explain reserved generated directories: {:?}",
        format.args
    );
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn biome_dx_config_reserved_directory_targets_are_case_insensitive() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("Node_Modules").join("package")).unwrap();
    fs::create_dir_all(root.path().join("BUILD")).unwrap();
    fs::write(
        root.path()
            .join("Node_Modules")
            .join("package")
            .join("app.ts"),
        "export const dependency = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("BUILD").join("app.ts"),
        "export const generated = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint Node_Modules true
format BUILD true
)
"#,
    )
    .unwrap();
    write_biome_package_with_javascript_fallbacks(root.path());

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "biome-lint")
        .expect("blocked Biome lint plan");
    let format = plans
        .iter()
        .find(|plan| plan.id == "biome-format")
        .expect("blocked Biome format plan");

    assert_eq!(lint.executable, "dx-check-blocked");
    assert_eq!(format.executable, "dx-check-blocked");
    assert!(
        lint.args
            .iter()
            .any(|arg| arg.contains("Node_Modules") && arg.contains("generated/dependency")),
        "lint should reject case variants of dependency directories: {:?}",
        lint.args
    );
    assert!(
        format
            .args
            .iter()
            .any(|arg| arg.contains("BUILD") && arg.contains("generated/dependency")),
        "format should reject case variants of generated directories: {:?}",
        format.args
    );
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn biome_uses_declared_package_manager_before_stale_lockfiles() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("bun.lock"), "").unwrap();
    fs::write(
        root.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();
    fs::write(root.path().join("package-lock.json"), "{}\n").unwrap();
    fs::write(root.path().join("app.ts"), "export const app = true;\n").unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint . true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "packageManager": "pnpm@9.15.0",
  "devDependencies": {
"@biomejs/biome": "^2.4.16"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "biome-lint")
        .expect("Biome lint plan");

    assert!(
        lint.executable == "pnpm" || lint.executable == "pnpm.cmd",
        "packageManager should win over stale lockfiles: {}",
        lint.executable
    );
    assert_eq!(
        lint.args
            .iter()
            .take(2)
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["exec", "biome"]
    );
    assert_eq!(
        lint.detected_from,
        [
            "package.json".to_string(),
            "dx".to_string(),
            "pnpm-lock.yaml".to_string()
        ]
    );
}

#[test]
fn js_package_manager_blocks_ambiguous_lockfiles_without_declared_package_manager() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("bun.lock"), "").unwrap();
    fs::write(root.path().join("package-lock.json"), "{}\n").unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"lint": "eslint ."
  },
  "devDependencies": {
"eslint": "^9.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "js-lint")
        .expect("blocked js lint plan");

    assert_eq!(lint.executable, "dx-check-blocked");
    assert_eq!(
        lint.detected_from,
        [
            "package.json".to_string(),
            "bun.lock".to_string(),
            "package-lock.json".to_string()
        ]
    );
    assert!(
        lint.args.iter().any(|arg| {
            arg.contains("multiple JavaScript lockfiles")
                && arg.contains("packageManager")
                && arg.contains("bun.lock")
                && arg.contains("package-lock.json")
        }),
        "blocked plan should explain ambiguous package manager evidence: {:?}",
        lint.args
    );
}

#[test]
fn js_package_manager_blocks_invalid_declared_package_manager() {
    for (name, descriptor, expected_reason) in [
        (
            "missing-version",
            "pnpm@",
            "declares invalid packageManager",
        ),
        ("missing-at", "pnpm", "declares invalid packageManager"),
        (
            "unsupported-manager",
            "corepack@1.0.0",
            "declares unsupported packageManager",
        ),
    ] {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "packageManager": "{descriptor}",
  "scripts": {{
"lint": "eslint ."
  }},
  "devDependencies": {{
"eslint": "^9.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
        let lint = plans
            .iter()
            .find(|plan| plan.id == "js-lint")
            .unwrap_or_else(|| panic!("{name} should create a blocked js lint plan"));

        assert_eq!(lint.executable, "dx-check-blocked", "{name}");
        assert_eq!(lint.detected_from, ["package.json".to_string()], "{name}");
        assert!(
            lint.args
                .iter()
                .any(|arg| arg.contains(expected_reason) && arg.contains(descriptor)),
            "{name} should explain invalid packageManager descriptor: {:?}",
            lint.args
        );
    }
}

#[test]
fn javascript_lint_write_risk_script_is_visible_as_blocked_plan() {
    let root = tempdir().unwrap();
    write_javascript_package(root.path(), r#""lint": "eslint . --fix""#);

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
    let lint = assert_blocked_javascript_plan(
        &plans,
        DxToolTarget::Lint,
        "js-lint",
        &["package.json", "lint", "--fix", "read-only lint"],
    );

    assert_eq!(lint.detected_from, ["package.json".to_string()]);
}

#[test]
fn javascript_typecheck_plans_reject_tsc_without_no_emit() {
    for (name, script) in [
        ("missing-no-emit", "tsc -p tsconfig.json"),
        ("generate-trace", "tsc --noEmit --generateTrace traces"),
    ] {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"typecheck": "{script}"
  }},
  "devDependencies": {{
"typescript": "^5.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_typecheck_plans_reject_value_attached_emit_flags() {
    for (name, script) in [
        ("out-dir-value", "tsc --noEmit --outDir=dist"),
        (
            "generate-trace-value",
            "tsc --noEmit --generateTrace=traces",
        ),
        (
            "tsbuildinfo-value",
            "tsc --noEmit --tsBuildInfoFile=.tsbuildinfo",
        ),
        ("project-build", "tsc --noEmit --build"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""typecheck": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_typecheck_plans_reject_later_separated_no_emit_false() {
    for (name, script) in [
        ("later-false", "tsc --noEmit --noEmit false"),
        ("later-zero", "tsc --noEmit --noEmit 0"),
        ("true-then-false", "tsc --noEmit true --noEmit false"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""typecheck": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_typecheck_plans_allow_no_emit_compilers() {
    for (name, script) in [
        ("typescript", "tsc --noEmit -p tsconfig.json"),
        ("vue-typescript", "vue-tsc --noEmit"),
    ] {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"typecheck": "{script}"
  }},
  "devDependencies": {{
"typescript": "^5.0.0",
"vue-tsc": "^3.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert!(
            plans
                .iter()
                .any(|plan| plan.id == "js-typecheck" && plan.target == DxToolTarget::Typecheck),
            "{name} should remain an approved no-emit typecheck plan"
        );
    }
}

#[test]
fn javascript_typecheck_plans_reject_build_script_delegation() {
    for (name, script) in [
        ("npm-run-build", "npm run build"),
        ("pnpm-shortcut-build", "pnpm build"),
        ("yarn-run-compile", "yarn run compile"),
    ] {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"typecheck": "{script}",
"build": "tsc",
"compile": "tsc"
  }},
  "devDependencies": {{
"typescript": "^5.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_package_scripts_reject_delegated_mutating_script_bodies() {
    let cases = [
        (
            "lint-delegated-fix",
            DxToolTarget::Lint,
            r#""lint": "npm run verify",
"verify": "eslint . --fix""#,
        ),
        (
            "format-delegated-write",
            DxToolTarget::Format,
            r#""format:check": "pnpm pretty",
"pretty": "prettier --write .""#,
        ),
        (
            "typecheck-delegated-build",
            DxToolTarget::Typecheck,
            r#""typecheck": "npm run types",
"types": "tsc -b""#,
        ),
    ];

    for (name, target, scripts) in cases {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), scripts);

        let plans = plan_tools(root.path(), &[target]);
        assert_blocked_javascript_target_for_case(&plans, target, name);
    }
}

#[test]
fn javascript_package_scripts_reject_recursive_delegation_cycles() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""lint": "npm run verify",
"verify": "npm run lint""#,
    );

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);

    assert_blocked_javascript_target(&plans, DxToolTarget::Lint);
}

#[test]
fn javascript_package_scripts_reject_mutating_lifecycle_hooks() {
    let cases = [
        (
            "lint-prehook-fix",
            DxToolTarget::Lint,
            r#""lint": "eslint .",
"prelint": "eslint . --fix""#,
        ),
        (
            "format-posthook-write",
            DxToolTarget::Format,
            r#""format:check": "prettier --check .",
"postformat:check": "prettier --write .""#,
        ),
        (
            "typecheck-prehook-build",
            DxToolTarget::Typecheck,
            r#""typecheck": "tsc --noEmit",
"pretypecheck": "tsc -b""#,
        ),
        (
            "test-posthook-update",
            DxToolTarget::Test,
            r#""test": "vitest run",
"posttest": "vitest --update-snapshot""#,
        ),
    ];

    for (name, target, scripts) in cases {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), scripts);

        let plans = plan_tools(root.path(), &[target]);
        assert_blocked_javascript_target_for_case(&plans, target, name);
    }
}

#[test]
fn javascript_package_scripts_reject_delegated_lifecycle_hooks() {
    let cases = [
        (
            "lint-delegated-prehook-fix",
            DxToolTarget::Lint,
            r#""lint": "npm run verify",
"verify": "eslint .",
"preverify": "eslint . --fix""#,
        ),
        (
            "format-delegated-posthook-write",
            DxToolTarget::Format,
            r#""format:check": "pnpm run pretty",
"pretty": "prettier --check .",
"postpretty": "prettier --write .""#,
        ),
        (
            "typecheck-delegated-prehook-build",
            DxToolTarget::Typecheck,
            r#""typecheck": "npm run types",
"types": "tsc --noEmit",
"pretypes": "tsc -b""#,
        ),
        (
            "test-delegated-posthook-update",
            DxToolTarget::Test,
            r#""test": "yarn run verify-tests",
"verify-tests": "vitest run",
"postverify-tests": "vitest --update-snapshot""#,
        ),
    ];

    for (name, target, scripts) in cases {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), scripts);

        let plans = plan_tools(root.path(), &[target]);
        assert_blocked_javascript_target_for_case(&plans, target, name);
    }
}

#[test]
fn javascript_typecheck_plans_reject_bun_naked_build_script_delegation() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""typecheck": "bun build",
"build": "tsc -b""#,
    );

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);

    assert_blocked_javascript_target(&plans, DxToolTarget::Typecheck);
}

#[test]
fn javascript_lint_plans_reject_bun_naked_mutating_script_delegation() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""lint": "bun verify",
"verify": "eslint . --fix""#,
    );

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);

    assert_blocked_javascript_target(&plans, DxToolTarget::Lint);
}

#[test]
fn javascript_typecheck_plans_reject_yarn_workspaces_foreach_build_after_value_options() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""typecheck": "yarn workspaces foreach --from app --jobs 2 run build",
"build": "tsc -b""#,
    );

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);

    assert_blocked_javascript_target(&plans, DxToolTarget::Typecheck);
}

#[test]
fn javascript_typecheck_plans_reject_workspace_build_after_additional_value_options() {
    for (name, script) in [
        (
            "yarn-workspaces-foreach-since",
            "yarn workspaces foreach --since main run build",
        ),
        (
            "pnpm-workspace-concurrency",
            "pnpm --workspace-concurrency 2 build",
        ),
        ("pnpm-recursive-run-build", "pnpm recursive run build"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(
            root.path(),
            &format!(
                r#""typecheck": "{script}",
"build": "tsc -b""#
            ),
        );

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_format_plans_reject_yarn_workspaces_foreach_mutation_after_value_options() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""format:check": "yarn workspaces foreach --include app -j 2 run format",
"format": "prettier --write .""#,
    );

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);

    assert_blocked_javascript_target(&plans, DxToolTarget::Format);
}

#[test]
fn javascript_typecheck_plans_reject_direct_framework_build_commands() {
    for (name, script) in [
        ("next-build", "next build"),
        ("vite-build", "vite build"),
        ("webpack", "webpack --config webpack.config.js"),
        ("rollup", "rollup -c"),
        ("tsup", "tsup src/index.ts"),
        ("esbuild", "esbuild src/index.ts --bundle"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""typecheck": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_typecheck_plans_reject_direct_framework_build_wrappers() {
    for (name, script) in [
        ("next-cmd", "next.cmd build"),
        ("vite-exe", "vite.exe build"),
        ("bun-cmd", "bun.cmd build"),
        ("webpack-cmd", "webpack.cmd --config webpack.config.js"),
        ("rollup-exe", "rollup.exe -c"),
        ("tsup-cmd", "tsup.cmd src/index.ts"),
        ("esbuild-exe", "esbuild.exe src/index.ts --bundle"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""typecheck": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Typecheck, name);
    }
}

#[test]
fn javascript_typecheck_and_test_scripts_require_string_values() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"typecheck": true,
"test": ["vitest", "run"]
  },
  "devDependencies": {
"typescript": "^5.0.0",
"vitest": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    assert!(
        !plans
            .iter()
            .any(|plan| plan.target == DxToolTarget::Typecheck),
        "non-string typecheck scripts must not become approved package-script plans"
    );
    assert!(
        !plans.iter().any(|plan| plan.target == DxToolTarget::Test),
        "non-string test scripts must not become approved package-script plans"
    );
}

#[test]
fn javascript_lint_and_format_plans_reject_cache_and_report_file_writes() {
    let cases = [
        (
            "eslint-cache",
            DxToolTarget::Lint,
            r#""lint": "eslint . --cache""#,
        ),
        (
            "eslint-output-short",
            DxToolTarget::Lint,
            r#""lint": "eslint . -o lint-report.json""#,
        ),
        (
            "eslint-output-long",
            DxToolTarget::Lint,
            r#""lint": "eslint . --output-file lint-report.json""#,
        ),
        (
            "prettier-cache",
            DxToolTarget::Format,
            r#""format:check": "prettier --check . --cache""#,
        ),
        (
            "prettier-cache-location",
            DxToolTarget::Format,
            r#""format:check": "prettier --check . --cache-location .cache/prettier""#,
        ),
    ];

    for (name, target, scripts) in cases {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), scripts);

        let plans = plan_tools(root.path(), &[target]);
        assert_blocked_javascript_target_for_case(&plans, target, name);
    }
}

#[test]
fn javascript_test_plans_reject_watch_by_default_runners() {
    for (name, scripts) in [
        ("vitest-default-watch", r#""test": "vitest""#),
        (
            "delegated-vitest-default-watch",
            r#""test": "npm run test:unit",
"test:unit": "vitest""#,
        ),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), scripts);

        let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Test, name);
    }
}

#[test]
fn javascript_test_plans_allow_explicit_non_interactive_vitest_modes() {
    for (name, script) in [
        ("vitest-run-command", "vitest run"),
        ("vitest-run-flag", "vitest --run"),
        ("vitest-watch-false", "vitest --watch=false"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""test": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
        assert!(
            plans
                .iter()
                .any(|plan| plan.id == "js-test" && plan.target == DxToolTarget::Test),
            "{name} should remain an approved non-interactive test plan"
        );
    }
}

#[test]
fn javascript_test_plans_reject_snapshot_update_flags() {
    for (name, script) in [
        ("vitest-short-update", "vitest -u"),
        ("vitest-update", "vitest --update"),
        ("vitest-quote-spliced-update", "vitest --up\\\"date\\\""),
        ("jest-update-snapshot", "jest --updateSnapshot"),
        ("jest-caret-spliced-update", "jest --update^Snapshot"),
        (
            "playwright-update-snapshots",
            "playwright test --update-snapshots",
        ),
        ("npm-delegated-update", "npm run test:unit -- -u"),
    ] {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"test": "{script}"
  }},
  "devDependencies": {{
"vitest": "^3.0.0",
"jest": "^30.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Test, name);
    }
}

#[test]
fn javascript_test_plans_reject_coverage_output_watch_and_ui_modes() {
    for (name, script) in [
        ("vitest-coverage", "vitest run --coverage"),
        ("jest-output-file", "jest --json --outputFile=results.json"),
        ("playwright-output", "playwright test --output test-results"),
        (
            "playwright-output-file",
            "playwright test --output-file results.json",
        ),
        ("jest-watch-all", "jest --watchAll"),
        ("playwright-ui", "playwright test --ui"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""test": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Test, name);
    }
}

#[test]
fn javascript_lint_plans_reject_quote_spliced_fix_flags() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"lint": "eslint . --f\"ix\""
  },
  "devDependencies": {
"eslint": "^9.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
    assert_blocked_javascript_target(&plans, DxToolTarget::Lint);
}

#[test]
fn javascript_lint_plans_reject_mutating_script_delegation() {
    for (name, script) in [
        ("npm-lint-fix", "npm run lint:fix"),
        ("pnpm-format-shortcut", "pnpm format"),
        ("yarn-format-write", "yarn run format:write"),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""lint": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Lint, name);
    }
}

#[test]
fn javascript_format_plans_reject_quote_spliced_write_flags() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format:check": "prettier --w'rite' ."
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_target(&plans, DxToolTarget::Format);
}

#[test]
fn javascript_format_plans_reject_mutating_format_variant_delegation() {
    for (name, script) in [
        (
            "npm-format-write",
            "prettier --check . && npm run format:write",
        ),
        (
            "pnpm-prettier-write",
            "prettier --check . && pnpm prettier:write",
        ),
        (
            "yarn-format-fix",
            "prettier --check . && yarn run format:fix",
        ),
    ] {
        let root = tempdir().unwrap();
        write_javascript_package(root.path(), &format!(r#""format:check": "{script}""#));

        let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Format, name);
    }
}

#[test]
fn javascript_format_plans_reject_quote_spliced_package_manager_delegation() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "prettier --check . && n\"pm\" run format"
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_target(&plans, DxToolTarget::Format);
}

#[test]
fn javascript_package_scripts_reject_shell_output_redirection() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""lint": "eslint . > lint.txt",
"format:check": "prettier --check . >> format.txt",
"typecheck": "tsc --noEmit 2> types.txt",
"test": "vitest run | tee results.txt""#,
    );

    let plans = plan_tools(
        root.path(),
        &[
            DxToolTarget::Lint,
            DxToolTarget::Format,
            DxToolTarget::Typecheck,
            DxToolTarget::Test,
        ],
    );
    assert_blocked_javascript_target(&plans, DxToolTarget::Lint);
    assert_blocked_javascript_target(&plans, DxToolTarget::Format);
    assert_blocked_javascript_target(&plans, DxToolTarget::Typecheck);
    assert_blocked_javascript_target(&plans, DxToolTarget::Test);
}

#[test]
fn javascript_package_scripts_reject_shell_chained_arbitrary_mutations() {
    let root = tempdir().unwrap();
    write_javascript_package(
        root.path(),
        r#""lint": "eslint . && node -e \"require('fs').writeFileSync('lint.txt','1')\"",
"format:check": "prettier --check . && node -e \"require('fs').writeFileSync('format.txt','1')\"",
"typecheck": "tsc --noEmit && node -e \"require('fs').writeFileSync('types.txt','1')\"",
"test": "vitest run && node -e \"require('fs').writeFileSync('test.txt','1')\"""#,
    );

    let plans = plan_tools(
        root.path(),
        &[
            DxToolTarget::Lint,
            DxToolTarget::Format,
            DxToolTarget::Typecheck,
            DxToolTarget::Test,
        ],
    );

    assert_blocked_javascript_target(&plans, DxToolTarget::Lint);
    assert_blocked_javascript_target(&plans, DxToolTarget::Format);
    assert_blocked_javascript_target(&plans, DxToolTarget::Typecheck);
    assert_blocked_javascript_target(&plans, DxToolTarget::Test);
}

#[test]
fn javascript_format_plans_reject_unspaced_shell_operator_delegation() {
    let cases = [
        ("and", "prettier --check .&&npm run format"),
        ("or", "prettier --check .||pnpm run format"),
        ("semicolon", "prettier --check .;yarn format"),
    ];

    for (name, script) in cases {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"format": "prettier --write .",
"format:check": "{script}"
  }},
  "devDependencies": {{
"prettier": "^3.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Format, name);
    }
}

fn write_javascript_package(root: &Path, scripts: &str) {
    fs::write(
        root.join("package.json"),
        format!(
            r#"{{
  "scripts": {{
{scripts}
  }},
  "devDependencies": {{
"eslint": "^9.0.0",
"jest": "^30.0.0",
"playwright": "^1.0.0",
"prettier": "^3.0.0",
"typescript": "^5.0.0",
"vitest": "^3.0.0"
  }}
}}
"#
        ),
    )
    .unwrap();
}

#[test]
fn javascript_format_plans_reject_shell_wrapped_package_manager_delegation() {
    let cases = [
        (
            "parenthesized-npm",
            "prettier --check . && (npm run format)",
        ),
        (
            "command-substitution-npm",
            "prettier --check . && $(npm run format)",
        ),
        (
            "parenthesized-pnpm-shortcut",
            "prettier --check . && (pnpm format)",
        ),
    ];

    for (name, script) in cases {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"format": "prettier --write .",
"format:check": "{script}"
  }},
  "devDependencies": {{
"prettier": "^3.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Format, name);
    }
}

#[test]
fn javascript_format_plans_reject_run_script_format_delegation() {
    let cases = [
        (
            "npm-run-script",
            "prettier --check . && npm run-script format",
        ),
        (
            "pnpm-run-script",
            "prettier --check . && pnpm run-script format",
        ),
        (
            "yarn-run-script",
            "prettier --check . && yarn run-script format",
        ),
    ];

    for (name, script) in cases {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"format": "prettier --write .",
"format:check": "{script}"
  }},
  "devDependencies": {{
"prettier": "^3.0.0"
  }}
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Format, name);
    }
}

#[test]
fn javascript_format_plans_reject_shell_escaped_write_and_delegation_tokens() {
    let cases = [
        ("backslash-write", "prettier --wri\\te ."),
        ("caret-write", "prettier --wri^te ."),
        (
            "backslash-package-manager",
            "prettier --check . && n\\pm run format",
        ),
        (
            "caret-package-manager",
            "prettier --check . && n^pm run format",
        ),
    ];

    for (name, script) in cases {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("package.json"),
            format!(
                r#"{{
  "scripts": {{
"format": "prettier --write .",
"format:check": "{}"
  }},
  "devDependencies": {{
"prettier": "^3.0.0"
  }}
}}
"#,
                script.replace('\\', "\\\\")
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
        assert_blocked_javascript_target_for_case(&plans, DxToolTarget::Format, name);
    }
}
