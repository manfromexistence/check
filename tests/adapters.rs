use std::fs;

use tempfile::tempdir;

use dx_check_engine::adapters::{executable_is_blocked, plan_tools};
use dx_check_engine::model::{DxToolPlan, DxToolTarget};

fn assert_blocked_javascript_plan(
    plans: &[DxToolPlan],
    target: DxToolTarget,
    id: &str,
    case: &str,
) {
    let plan = plans
        .iter()
        .find(|plan| plan.id == id && plan.target == target)
        .unwrap_or_else(|| panic!("{case}: {id} should be visible as a blocked JavaScript plan"));

    assert_eq!(plan.executable, "dx-check-blocked", "{case}: {id}");
    assert_eq!(plan.parser, "blocked", "{case}: {id}");
    assert!(
        plan.args
            .iter()
            .any(|arg| arg.contains("not safe for dx check")),
        "{case}: {id} should explain why it is blocked: {:?}",
        plan.args
    );
}

#[test]
fn blocks_shell_executables() {
    assert!(executable_is_blocked("powershell.exe"));
    assert!(executable_is_blocked("PowerShell.CMD"));
    assert!(executable_is_blocked("cmd"));
    assert!(executable_is_blocked("cmd.exe"));
    assert!(executable_is_blocked("cmd.cmd"));
    assert!(executable_is_blocked("bash"));
    assert!(executable_is_blocked("bash.exe"));
    assert!(executable_is_blocked("sh.exe"));
    assert!(executable_is_blocked("SH.CMD"));
    assert!(!executable_is_blocked("cargo"));
}

#[test]
fn rust_plans_use_safe_argv_and_single_job_tests() {
    let temp = tempdir().unwrap();
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();

    let plans = plan_tools(
        temp.path(),
        &[
            DxToolTarget::Lint,
            DxToolTarget::Format,
            DxToolTarget::Typecheck,
            DxToolTarget::Test,
        ],
    );

    assert!(plans.iter().any(|plan| {
        plan.id == "rust-clippy"
            && plan.executable == "cargo"
            && plan.args.contains(&"-j".to_string())
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "rustfmt-check" && plan.args == ["fmt", "--all", "--", "--check"]
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "cargo-test" && plan.args.windows(2).any(|pair| pair == ["-j", "1"])
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "cargo-test"
            && plan.parser == "cargo-json"
            && plan.args.contains(&"--message-format=json".to_string())
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "cargo-check"
            && plan.parser == "cargo-json"
            && plan.args.contains(&"--message-format=json".to_string())
    }));
}

#[test]
fn python_and_go_plans_emit_matching_parsers() {
    let python = tempdir().unwrap();
    fs::write(python.path().join("ruff.toml"), "line-length = 100\n").unwrap();
    let python_plans = plan_tools(
        python.path(),
        &[DxToolTarget::Lint, DxToolTarget::Format, DxToolTarget::Test],
    );

    assert!(python_plans.iter().any(|plan| {
        plan.id == "ruff-check" && plan.parser == "ruff-json" && plan.detected_from == ["ruff.toml"]
    }));
    assert!(python_plans.iter().any(|plan| {
        plan.id == "ruff-format-check"
            && plan.parser == "ruff-format"
            && plan.detected_from == ["ruff.toml"]
    }));

    let go = tempdir().unwrap();
    fs::write(go.path().join("go.mod"), "module demo\n").unwrap();
    let go_plans = plan_tools(
        go.path(),
        &[DxToolTarget::Lint, DxToolTarget::Format, DxToolTarget::Test],
    );

    assert!(
        go_plans
            .iter()
            .any(|plan| { plan.id == "gofmt-check" && plan.parser == "gofmt-list" })
    );
    assert!(
        go_plans
            .iter()
            .any(|plan| plan.id == "go-vet" && plan.parser == "go-vet")
    );
    assert!(
        go_plans
            .iter()
            .any(|plan| plan.id == "go-test" && plan.parser == "go-test")
    );
}

#[test]
fn python_black_pyproject_uses_black_format_check_without_unconfigured_ruff_format() {
    let python = tempdir().unwrap();
    fs::write(
        python.path().join("pyproject.toml"),
        "[tool.black]\nline-length = 100\n",
    )
    .unwrap();

    let plans = plan_tools(python.path(), &[DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "black-format-check"
            && plan.target == DxToolTarget::Format
            && plan.executable == "black"
            && plan.args == ["--check", "--diff", "."]
            && plan.parser == "black"
            && plan.detected_from == ["pyproject.toml"]
    }));
    assert!(
        !plans.iter().any(|plan| plan.id == "ruff-format-check"),
        "a pyproject.toml with only [tool.black] must not pretend Ruff formatting is configured"
    );
}

#[test]
fn javascript_format_plans_require_check_only_script() {
    let bare_format = tempdir().unwrap();
    fs::write(
        bare_format.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write ."
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let bare_plans = plan_tools(bare_format.path(), &[DxToolTarget::Format]);
    assert!(
        !bare_plans
            .iter()
            .any(|plan| plan.target == DxToolTarget::Format && plan.id == "js-format"),
        "bare `scripts.format` must not become a check-only JS format plan"
    );

    let checked_format = tempdir().unwrap();
    fs::write(
        checked_format.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "prettier --check ."
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let checked_plans = plan_tools(checked_format.path(), &[DxToolTarget::Format]);
    assert!(checked_plans.iter().any(|plan| {
        plan.target == DxToolTarget::Format
            && plan.id == "js-format:check"
            && plan.args.iter().any(|arg| arg == "format:check")
    }));
}

#[test]
fn biome_lint_and_format_plans_prefer_biome_when_extensionless_dx_configured() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("pnpm-lock.yaml"), "").unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint . true
format . true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
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

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);
    let lint = plans
        .iter()
        .find(|plan| plan.id == "biome-lint")
        .expect("Biome lint plan");
    let format = plans
        .iter()
        .find(|plan| plan.id == "biome-format")
        .expect("Biome format plan");

    assert_eq!(lint.target, DxToolTarget::Lint);
    assert_eq!(lint.parser, "biome-json");
    assert_eq!(
        lint.args,
        [
            "exec",
            "biome",
            "lint",
            "--reporter=json",
            "--max-diagnostics=none",
            "--colors=off",
            "--no-errors-on-unmatched",
            "."
        ]
    );
    assert_eq!(lint.detected_from, ["package.json", "dx", "pnpm-lock.yaml"]);
    assert_eq!(format.target, DxToolTarget::Format);
    assert_eq!(format.parser, "biome-json");
    assert_eq!(
        format.args,
        [
            "exec",
            "biome",
            "format",
            "--reporter=json",
            "--max-diagnostics=none",
            "--colors=off",
            "--no-errors-on-unmatched",
            "."
        ]
    );
    assert!(
        !format
            .args
            .iter()
            .any(|arg| arg == "--write" || arg == "--fix")
    );
    assert!(
        !plans
            .iter()
            .any(|plan| plan.id == "js-lint" || plan.id == "js-format:check"),
        "Biome should be the preferred JS lint/format adapter when configured"
    );
}

#[test]
fn biome_json_config_without_dx_config_keeps_package_script_format() {
    let root = tempdir().unwrap();
    fs::write(root.path().join(".biome.json"), "{}\n").unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format:check": "prettier --check ."
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);

    assert!(plans.iter().all(|plan| plan.id != "biome-format"));
    assert!(plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn biome_dependency_without_config_keeps_package_script_lint_and_format() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
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

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(!plans.iter().any(|plan| plan.id.starts_with("biome-")));
    assert!(plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn invalid_biome_dx_config_blocks_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint "../outside" true
format "../outside" true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"lint": "eslint .",
"format:check": "prettier --check ."
  },
  "devDependencies": {
"eslint": "^9.0.0",
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "biome-lint"
            && plan.target == DxToolTarget::Lint
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("invalid Biome dx config"))
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "biome-format"
            && plan.target == DxToolTarget::Format
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("invalid Biome dx config"))
    }));
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn stale_biome_dx_paths_block_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint missing-source true
format missing-source true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
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

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "biome-lint"
            && plan.target == DxToolTarget::Lint
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("missing-source") && arg.contains("does not exist"))
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "biome-format"
            && plan.target == DxToolTarget::Format
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("missing-source") && arg.contains("does not exist"))
    }));
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn linked_biome_dx_paths_that_escape_root_block_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(
        outside.path().join("escape.ts"),
        "export const escape = true;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint linked-source true
format linked-source true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
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

    if create_directory_link(outside.path(), &root.path().join("linked-source")).is_err() {
        return;
    }

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "biome-lint"
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("linked-source") && arg.contains("outside the project"))
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "biome-format"
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("linked-source") && arg.contains("outside the project"))
    }));
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn empty_biome_dx_paths_block_javascript_fallback_plans() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint src true
format src true
)
"#,
    )
    .unwrap();
    fs::write(
        root.path().join("package.json"),
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

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "biome-lint"
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("src") && arg.contains("contains no Biome-supported files"))
    }));
    assert!(plans.iter().any(|plan| {
        plan.id == "biome-format"
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("src") && arg.contains("contains no Biome-supported files"))
    }));
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(!plans.iter().any(|plan| plan.id == "js-format:check"));
}

#[test]
fn biome_dx_config_without_local_biome_dependency_blocks_fallback_for_configured_targets() {
    let root = tempdir().unwrap();
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
  "scripts": {
"lint": "eslint .",
"format:check": "prettier --check ."
  },
  "devDependencies": {
"eslint": "^9.0.0",
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "biome-lint"
            && plan.executable == "dx-check-blocked"
            && plan
                .args
                .iter()
                .any(|arg| arg.contains("package.json") && arg.contains("@biomejs/biome"))
    }));
    assert!(!plans.iter().any(|plan| plan.id == "js-lint"));
    assert!(plans.iter().any(|plan| plan.id == "js-format:check"));
    assert!(!plans.iter().any(|plan| plan.id == "biome-format"));
}

#[test]
fn biome_dx_config_paths_are_trimmed_deduplicated_and_scoped_by_target() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("tests")).unwrap();
    fs::write(
        root.path().join("src").join("app.ts"),
        "export const app = 1;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tests").join("app.test.ts"),
        "export const testApp = true;\n",
    )
    .unwrap();
    fs::write(root.path().join("pnpm-lock.yaml"), "").unwrap();
    fs::write(
        root.path().join("dx"),
        r#"
biome[target path enabled](
lint " src " true
lint src true
lint tests true
format . true
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
        .expect("Biome lint plan");
    let format = plans
        .iter()
        .find(|plan| plan.id == "biome-format")
        .expect("Biome format plan");

    assert_eq!(
        lint.args,
        [
            "exec",
            "biome",
            "lint",
            "--reporter=json",
            "--max-diagnostics=none",
            "--colors=off",
            "--no-errors-on-unmatched",
            "src",
            "tests"
        ]
    );
    assert_eq!(
        format.args,
        [
            "exec",
            "biome",
            "format",
            "--reporter=json",
            "--max-diagnostics=none",
            "--colors=off",
            "--no-errors-on-unmatched",
            "."
        ]
    );
}

#[test]
fn js_package_manager_detected_from_omits_missing_npm_lockfile() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"test": "vitest run"
  },
  "devDependencies": {
"vitest": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
    let plan = plans
        .iter()
        .find(|plan| plan.id == "js-test")
        .expect("js test plan");

    assert_eq!(plan.detected_from, ["package.json"]);
}

#[test]
fn js_package_manager_detected_from_reports_bun_lockb() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("bun.lockb"), "").unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"test": "bun test"
  },
  "devDependencies": {
"bun-types": "^1.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
    let plan = plans
        .iter()
        .find(|plan| plan.id == "js-test")
        .expect("js test plan");

    assert_eq!(plan.detected_from, ["package.json", "bun.lockb"]);
}

#[test]
fn javascript_lint_plans_reject_write_risk_scripts() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"lint": "eslint . --fix"
  },
  "devDependencies": {
"eslint": "^9.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);

    assert_blocked_javascript_plan(&plans, DxToolTarget::Lint, "js-lint", "lint-write-risk");
}

#[test]
fn javascript_format_plans_reject_format_check_delegating_to_bare_format() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "npm run format"
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_plan(
        &plans,
        DxToolTarget::Format,
        "js-format:check",
        "format-delegates-to-format",
    );
}

#[test]
fn javascript_format_plans_reject_chained_format_check_delegation() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "prettier --check . && npm run format"
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_plan(
        &plans,
        DxToolTarget::Format,
        "js-format:check",
        "format-chain-delegates-to-format",
    );
}

#[test]
fn javascript_format_plans_reject_chained_format_check_delegation_with_package_manager_flags() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "prettier --check . && npm --silent run format"
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_plan(
        &plans,
        DxToolTarget::Format,
        "js-format:check",
        "format-chain-delegates-with-flags",
    );
}

#[test]
fn javascript_format_plans_reject_format_check_delegation_with_package_manager_option_value() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{
  "scripts": {
"format": "prettier --write .",
"format:check": "prettier --check . && npm --prefix nested run format"
  },
  "devDependencies": {
"prettier": "^3.0.0"
  }
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format]);
    assert_blocked_javascript_plan(
        &plans,
        DxToolTarget::Format,
        "js-format:check",
        "format-chain-delegates-with-option-value",
    );
}

#[test]
fn javascript_format_plans_reject_workspace_format_delegation() {
    let cases = [
        (
            "npm-workspace",
            "prettier --check . && npm -w app run format",
        ),
        (
            "npm-workspaces",
            "prettier --check . && npm --workspaces run format",
        ),
        (
            "yarn-workspace",
            "prettier --check . && yarn workspace app format",
        ),
        (
            "yarn-workspaces-foreach",
            "prettier --check . && yarn workspaces foreach run format",
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
        assert_blocked_javascript_plan(&plans, DxToolTarget::Format, "js-format:check", name);
    }
}

#[cfg(unix)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
