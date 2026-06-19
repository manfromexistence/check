use std::cell::Cell;
use std::path::Path;

use tempfile::tempdir;

use dx_check_engine::adapters::{
    DxToolProcessOutput, DxToolRunStatus, run_tool_plan_with_executor,
};
use dx_check_engine::model::{DxMeasurementKind, DxSeverity, DxToolPlan, DxToolTarget};

fn plan(root: &Path, executable: &str) -> DxToolPlan {
    DxToolPlan {
        id: "cargo-check".to_string(),
        target: DxToolTarget::Typecheck,
        executable: executable.to_string(),
        args: vec!["check".to_string(), "-j".to_string(), "1".to_string()],
        cwd: root.to_path_buf(),
        detected_from: vec!["Cargo.toml".to_string()],
        parser: "cargo-json".to_string(),
    }
}

#[test]
fn rejects_blocked_shell_executable_without_running_executor() {
    let temp = tempdir().unwrap();
    let called = Cell::new(false);
    let result =
        run_tool_plan_with_executor(temp.path(), &plan(temp.path(), "pwsh.exe"), |_plan| {
            called.set(true);
            Ok(DxToolProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: Some(0),
                duration_ms: 1,
            })
        })
        .expect("blocked result");

    assert!(
        !called.get(),
        "blocked adapters must not invoke the executor"
    );
    assert_eq!(result.status, DxToolRunStatus::Blocked);
    assert_eq!(result.exit_code, None);
    assert!(
        result
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| { reason.contains("unsafe executable") })
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn rejects_blocked_shell_aliases_without_running_executor() {
    for executable in ["bash.exe", "sh.exe", "PowerShell.CMD", "cmd.bat"] {
        let temp = tempdir().unwrap();
        let called = Cell::new(false);
        let result =
            run_tool_plan_with_executor(temp.path(), &plan(temp.path(), executable), |_| {
                called.set(true);
                Ok(DxToolProcessOutput {
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    exit_code: Some(0),
                    duration_ms: 1,
                })
            })
            .expect("blocked result");

        assert!(
            !called.get(),
            "{executable} must be blocked before executor invocation"
        );
        assert_eq!(result.status, DxToolRunStatus::Blocked, "{executable}");
        assert!(
            result
                .blocked_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("unsafe executable")),
            "{executable} should report an unsafe executable reason"
        );
    }
}

#[test]
fn rejects_command_shim_shell_metacharacters_without_running_executor() {
    for (name, args) in [
        ("shell-and", vec!["run", "lint&whoami"]),
        ("pipe", vec!["run", "lint|whoami"]),
        ("redirect", vec!["run", "lint>out.txt"]),
        ("percent-expansion", vec!["run", "%USERNAME%"]),
        ("delayed-expansion", vec!["run", "!USERNAME!"]),
        ("caret-escape", vec!["run", "lint^&whoami"]),
        ("newline", vec!["run", "lint\r\nwhoami"]),
    ] {
        let temp = tempdir().unwrap();
        let called = Cell::new(false);
        let mut shim_plan = plan(temp.path(), "npm.cmd");
        shim_plan.target = DxToolTarget::Lint;
        shim_plan.args = args.into_iter().map(str::to_string).collect();
        shim_plan.parser = "package-script".to_string();

        let result = run_tool_plan_with_executor(temp.path(), &shim_plan, |_| {
            called.set(true);
            Ok(DxToolProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code: Some(0),
                duration_ms: 1,
            })
        })
        .expect("blocked result");

        assert!(
            !called.get(),
            "{name} must block before executor invocation"
        );
        assert_eq!(result.status, DxToolRunStatus::Blocked, "{name}");
        assert!(
            result
                .blocked_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("shell metacharacter")),
            "{name} should report shell metacharacter risk"
        );
    }
}

#[test]
fn allows_command_shim_plain_script_args() {
    let temp = tempdir().unwrap();
    let called = Cell::new(false);
    let mut shim_plan = plan(temp.path(), "npm.cmd");
    shim_plan.target = DxToolTarget::Lint;
    shim_plan.args = vec!["run".to_string(), "lint".to_string()];
    shim_plan.parser = "package-script".to_string();

    let result = run_tool_plan_with_executor(temp.path(), &shim_plan, |_| {
        called.set(true);
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 1,
        })
    })
    .expect("run result");

    assert!(called.get());
    assert_eq!(result.status, DxToolRunStatus::Passed);
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn runs_safe_plan_and_imports_parsed_cargo_json_diagnostic() {
    let temp = tempdir().unwrap();
    let result = run_tool_plan_with_executor(temp.path(), &plan(temp.path(), "cargo"), |_plan| {
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: br#"{"reason":"compiler-message","message":{"message":"expected item","code":null,"level":"error","spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true}]}}"#.to_vec(),
            exit_code: Some(101),
            duration_ms: 7,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(101));
    assert_eq!(result.duration_ms, 7);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].source, "cargo-check");
    assert_eq!(result.diagnostics[0].file.as_deref(), Some("src/main.rs"));
}

#[test]
fn executor_start_error_becomes_blocked_result() {
    let temp = tempdir().unwrap();
    let result = run_tool_plan_with_executor(temp.path(), &plan(temp.path(), "cargo"), |_plan| {
        anyhow::bail!("program not found")
    })
    .expect("blocked result");

    assert_eq!(result.status, DxToolRunStatus::Blocked);
    assert_eq!(result.exit_code, None);
    assert!(result.diagnostics.is_empty());
    assert!(result.blocked_reason.as_deref().is_some_and(|reason| {
        reason.contains("could not be started") && reason.contains("program not found")
    }));
}

#[test]
fn blocked_adapter_plan_uses_config_reason_without_running_executor() {
    let temp = tempdir().unwrap();
    let called = Cell::new(false);
    let mut plan = plan(temp.path(), "dx-check-blocked");
    plan.id = "biome-lint".to_string();
    plan.target = DxToolTarget::Lint;
    plan.args = vec!["invalid Biome dx config: path cannot escape".to_string()];
    plan.detected_from = vec!["dx".to_string()];
    plan.parser = "blocked".to_string();

    let result = run_tool_plan_with_executor(temp.path(), &plan, |_plan| {
        called.set(true);
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 1,
        })
    })
    .expect("blocked adapter result");

    assert!(!called.get());
    assert_eq!(result.status, DxToolRunStatus::Blocked);
    assert_eq!(
        result.blocked_reason.as_deref(),
        Some("invalid Biome dx config: path cannot escape")
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].id, "biome-lint:adapter-blocked");
    assert_eq!(result.diagnostics[0].source, "biome-lint");
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(
        result.diagnostics[0].measurement,
        DxMeasurementKind::Skipped
    );
    assert!(
        result.diagnostics[0]
            .message
            .contains("invalid Biome dx config: path cannot escape")
    );
}

#[test]
fn blocked_adapter_plan_sanitizes_reflected_reason_control_chars_without_running_executor() {
    let temp = tempdir().unwrap();
    let called = Cell::new(false);
    let mut plan = plan(temp.path(), "dx-check-blocked");
    plan.args = vec!["unsafe reason\r\nhidden\0value".to_string()];

    let result = run_tool_plan_with_executor(temp.path(), &plan, |_plan| {
        called.set(true);
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 1,
        })
    })
    .expect("blocked adapter result");

    assert!(!called.get());
    assert_eq!(result.status, DxToolRunStatus::Blocked);
    let reason = result.blocked_reason.as_deref().unwrap_or_default();
    assert!(reason.contains("unsafe reason"));
    assert!(reason.contains("hidden"));
    assert!(reason.contains("value"));
    assert!(
        !reason
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n')),
        "blocked reason must be safe for terminal and receipt output"
    );
}

#[test]
fn zero_exit_failure_diagnostics_make_tool_run_failed() {
    let temp = tempdir().unwrap();
    let gofmt_plan = DxToolPlan {
        id: "gofmt-check".to_string(),
        target: DxToolTarget::Format,
        executable: "gofmt".to_string(),
        args: vec!["-l".to_string(), ".".to_string()],
        cwd: temp.path().to_path_buf(),
        detected_from: vec!["go.mod".to_string()],
        parser: "gofmt-list".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &gofmt_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: b"main.go\n".to_vec(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 5,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn zero_exit_empty_web_audit_output_makes_tool_run_failed() {
    let temp = tempdir().unwrap();
    let web_plan = DxToolPlan {
        id: "web-audit-home".to_string(),
        target: DxToolTarget::Audit,
        executable: "dx-check-web-audit".to_string(),
        args: vec![
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "http://localhost:3000/".to_string(),
        ],
        cwd: temp.path().to_path_buf(),
        detected_from: vec!["dx".to_string()],
        parser: "web-audit-json".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &web_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 5,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id,
        "web-audit-home:runner-output-invalid"
    );
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn zero_exit_malformed_web_audit_severity_makes_tool_run_failed() {
    let temp = tempdir().unwrap();
    let web_plan = DxToolPlan {
        id: "web-audit-home".to_string(),
        target: DxToolTarget::Audit,
        executable: "dx-check-web-audit".to_string(),
        args: vec![
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "http://localhost:3000/".to_string(),
        ],
        cwd: temp.path().to_path_buf(),
        detected_from: vec!["dx".to_string()],
        parser: "web-audit-json".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &web_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: br#"{
  "url": "http://localhost:3000/",
  "diagnostics": [
    {
      "id": "web-http-status",
      "severity": "fatal",
      "message": "HTTP probe crashed",
      "next_action": "Inspect the web audit runner."
    }
  ]
}"#
            .to_vec(),
            stderr: Vec::new(),
            exit_code: Some(0),
            duration_ms: 5,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].id,
        "web-audit-home:runner-output-invalid"
    );
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Failure);
    assert!(result.diagnostics[0].message.contains("severity"));
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn zero_exit_clang_format_diagnostics_make_tool_run_failed() {
    let temp = tempdir().unwrap();
    let format_plan = DxToolPlan {
        id: "cpp-clang-format-check".to_string(),
        target: DxToolTarget::Format,
        executable: "clang-format".to_string(),
        args: vec![
            "--dry-run".to_string(),
            "--Werror".to_string(),
            "src/main.cpp".to_string(),
        ],
        cwd: temp.path().to_path_buf(),
        detected_from: vec![".clang-format".to_string()],
        parser: "clang-format".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &format_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: b"src/main.cpp:1:1: error: code should be clang-formatted [-Wclang-format-violations]\n"
                .to_vec(),
            exit_code: Some(0),
            duration_ms: 5,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(result.diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn zero_exit_lint_warning_diagnostics_make_tool_run_failed() {
    let temp = tempdir().unwrap();
    let lint_plan = DxToolPlan {
        id: "cpp-clang-tidy".to_string(),
        target: DxToolTarget::Lint,
        executable: "clang-tidy".to_string(),
        args: vec!["src/main.cpp".to_string()],
        cwd: temp.path().to_path_buf(),
        detected_from: vec![".clang-tidy".to_string()],
        parser: "clang-tidy".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &lint_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: b"src/main.cpp:4:7: warning: use auto when initializing with new [modernize-use-auto]\n"
                .to_vec(),
            exit_code: Some(0),
            duration_ms: 6,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Warning);
    assert_eq!(result.diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert_eq!(result.blocked_reason, None);
}

#[test]
fn zero_exit_cppcheck_warning_diagnostics_make_tool_run_failed() {
    let temp = tempdir().unwrap();
    let lint_plan = DxToolPlan {
        id: "cpp-cppcheck".to_string(),
        target: DxToolTarget::Lint,
        executable: "cppcheck".to_string(),
        args: vec!["--enable=all".to_string(), "src/main.cpp".to_string()],
        cwd: temp.path().to_path_buf(),
        detected_from: vec!["src/main.cpp".to_string()],
        parser: "cppcheck-xml".to_string(),
    };

    let result = run_tool_plan_with_executor(temp.path(), &lint_plan, |_plan| {
        Ok(DxToolProcessOutput {
            stdout: Vec::new(),
            stderr: br#"
<results version="2">
  <errors>
    <error id="unusedFunction" severity="style" msg="The function 'helper' is never used.">
      <location file="src/main.cpp" line="7" column="1"/>
    </error>
  </errors>
</results>
"#
            .to_vec(),
            exit_code: Some(0),
            duration_ms: 6,
        })
    })
    .expect("run result");

    assert_eq!(result.status, DxToolRunStatus::Failed);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].severity, DxSeverity::Warning);
    assert_eq!(result.diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert_eq!(result.blocked_reason, None);
}
