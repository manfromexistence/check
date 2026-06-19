use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::model::{
    DxDiagnostic, DxMeasurementKind, DxSeverity, DxToolPlan, DxToolProcessOutput, DxToolRunResult,
    DxToolRunStatus, DxToolTarget,
};

const DEFAULT_BLOCKED_REASON: &str = "engine adapter was blocked by configuration";

pub fn executable_is_blocked(executable: &str) -> bool {
    matches!(
        executable.to_ascii_lowercase().as_str(),
        "cmd"
            | "cmd.exe"
            | "cmd.cmd"
            | "cmd.bat"
            | "powershell"
            | "powershell.exe"
            | "powershell.cmd"
            | "powershell.bat"
            | "pwsh"
            | "pwsh.exe"
            | "pwsh.cmd"
            | "pwsh.bat"
            | "sh"
            | "sh.exe"
            | "sh.cmd"
            | "sh.bat"
            | "bash"
            | "bash.exe"
            | "bash.cmd"
            | "bash.bat"
    )
}

pub fn run_tool_plan(root: &Path, plan: &DxToolPlan) -> anyhow::Result<DxToolRunResult> {
    run_tool_plan_with_executor(root, plan, execute_process)
}

pub fn run_tool_plan_with_executor(
    root: &Path,
    plan: &DxToolPlan,
    executor: impl FnOnce(&DxToolPlan) -> anyhow::Result<DxToolProcessOutput>,
) -> anyhow::Result<DxToolRunResult> {
    if let Some(blocked_reason) = validate_tool_plan(root, plan) {
        return Ok(blocked_tool_run(plan, blocked_reason));
    }

    let output = match executor(plan) {
        Ok(output) => output,
        Err(error) => {
            return Ok(blocked_tool_run(
                plan,
                format!(
                    "engine adapter executable `{}` could not be started: {error}",
                    plan.executable
                ),
            ));
        }
    };
    let diagnostics = crate::diagnostics::parse_tool_output(plan, &output.stdout, &output.stderr);
    let status = if output.exit_code == Some(0)
        && !has_invalid_runner_output(&diagnostics)
        && !has_blocking_diagnostic(plan, &diagnostics)
    {
        DxToolRunStatus::Passed
    } else {
        DxToolRunStatus::Failed
    };

    Ok(DxToolRunResult {
        plan: plan.clone(),
        status,
        exit_code: output.exit_code,
        duration_ms: output.duration_ms,
        stdout: output.stdout,
        stderr: output.stderr,
        diagnostics,
        blocked_reason: None,
    })
}

fn execute_process(plan: &DxToolPlan) -> anyhow::Result<DxToolProcessOutput> {
    let started = Instant::now();
    let output = Command::new(&plan.executable)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .output()?;

    Ok(DxToolProcessOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code: output.status.code(),
        duration_ms: started.elapsed().as_millis(),
    })
}

fn validate_tool_plan(root: &Path, plan: &DxToolPlan) -> Option<String> {
    if let Some(blocked_reason) = blocked_adapter_plan_reason(plan) {
        return Some(blocked_reason);
    }

    let executable = plan.executable.trim();
    if executable.is_empty()
        || executable != plan.executable
        || executable.contains('\0')
        || executable.chars().any(char::is_whitespace)
        || executable.contains('/')
        || executable.contains('\\')
        || executable_is_blocked(executable)
    {
        return Some(format!(
            "engine adapter selected an unsafe executable `{}`",
            plan.executable
        ));
    }

    if plan.args.iter().any(|arg| arg.contains('\0')) {
        return Some("engine adapter args contain a NUL byte".to_string());
    }

    if executable_is_command_shim(executable)
        && plan.args.iter().any(|arg| arg_has_shell_metacharacter(arg))
    {
        return Some("engine adapter command-shim args contain a shell metacharacter".to_string());
    }

    let root = match std::fs::canonicalize(root) {
        Ok(root) => root,
        Err(error) => {
            return Some(format!(
                "engine adapter project root could not be resolved: {error}"
            ));
        }
    };
    let cwd = match std::fs::canonicalize(&plan.cwd) {
        Ok(cwd) => cwd,
        Err(error) => {
            return Some(format!("engine adapter cwd could not be resolved: {error}"));
        }
    };

    if !cwd.starts_with(root) {
        return Some("engine adapter cwd is outside the project root".to_string());
    }

    None
}

pub fn blocked_adapter_plan_diagnostic(plan: &DxToolPlan) -> Option<DxDiagnostic> {
    let blocked_reason = blocked_adapter_plan_reason(plan)?;
    Some(DxDiagnostic {
        id: format!("{}:adapter-blocked", plan.id),
        source: plan.id.clone(),
        severity: DxSeverity::Failure,
        file: None,
        line: None,
        column: None,
        message: format!("Adapter plan `{}` was blocked: {blocked_reason}", plan.id),
        next_action:
            "Resolve the adapter configuration or toolchain evidence, then rerun dx check."
                .to_string(),
        measurement: DxMeasurementKind::Skipped,
    })
}

fn blocked_adapter_plan_reason(plan: &DxToolPlan) -> Option<String> {
    if plan.executable != "dx-check-blocked" {
        return None;
    }

    Some(
        plan.args
            .first()
            .map(|reason| sanitize_reflected_reason(reason))
            .unwrap_or_else(|| DEFAULT_BLOCKED_REASON.to_string()),
    )
}

fn executable_is_command_shim(executable: &str) -> bool {
    let executable = executable.to_ascii_lowercase();
    executable.ends_with(".cmd") || executable.ends_with(".bat")
}

fn arg_has_shell_metacharacter(arg: &str) -> bool {
    arg.chars().any(|character| {
        matches!(
            character,
            '&' | '|' | '<' | '>' | '%' | '!' | '^' | '\r' | '\n'
        )
    })
}

fn sanitize_reflected_reason(reason: &str) -> String {
    let mut sanitized = String::with_capacity(reason.len());
    let mut last_was_space = false;
    for character in reason.chars() {
        if matches!(character, '\0' | '\r' | '\n') {
            if !last_was_space {
                sanitized.push(' ');
                last_was_space = true;
            }
            continue;
        }
        sanitized.push(character);
        last_was_space = character.is_whitespace();
    }

    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        DEFAULT_BLOCKED_REASON.to_string()
    } else {
        sanitized.to_string()
    }
}

fn blocked_tool_run(plan: &DxToolPlan, blocked_reason: String) -> DxToolRunResult {
    let diagnostics = blocked_adapter_plan_diagnostic(plan)
        .into_iter()
        .collect::<Vec<_>>();

    DxToolRunResult {
        plan: plan.clone(),
        status: DxToolRunStatus::Blocked,
        exit_code: None,
        duration_ms: 0,
        stdout: Vec::new(),
        stderr: Vec::new(),
        diagnostics,
        blocked_reason: Some(blocked_reason),
    }
}

fn has_invalid_runner_output(diagnostics: &[DxDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.id.ends_with(":runner-output-invalid"))
}

fn has_blocking_diagnostic(plan: &DxToolPlan, diagnostics: &[DxDiagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == DxSeverity::Failure
            || (plan.target == DxToolTarget::Lint && diagnostic.severity == DxSeverity::Warning)
    })
}
