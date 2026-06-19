use std::path::Path;

use crate::model::{DxToolPlan, DxToolTarget};

use super::javascript_package_manager::{PackageManagerSelectionError, package_manager};

pub(super) fn package_manager_plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    let package = root.join("package.json");
    if !package.is_file() {
        return Vec::new();
    }

    let Ok(body) = std::fs::read_to_string(&package) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(|scripts| scripts.as_object()) else {
        return Vec::new();
    };

    let manager = match package_manager(root) {
        Ok(manager) => manager,
        Err(error) => return blocked_package_manager_plans(root, targets, scripts, &error),
    };
    let mut plans = Vec::new();
    for target in targets {
        let assessment = script_for_target(scripts, *target);
        if matches!(assessment, ScriptAssessment::Missing) {
            continue;
        }
        let mut detected_from = vec!["package.json".to_string()];
        if let Some(lockfile) = manager.lockfile {
            detected_from.push(lockfile.to_string());
        }
        match assessment {
            ScriptAssessment::Missing => {}
            ScriptAssessment::Safe(script) => plans.push(DxToolPlan {
                id: format!("js-{script}"),
                target: *target,
                executable: manager.executable_name(),
                args: manager.args_for_script(script),
                cwd: root.to_path_buf(),
                detected_from,
                parser: "package-script".to_string(),
            }),
            ScriptAssessment::Blocked { name, reason } => plans.push(DxToolPlan {
                id: format!("js-{name}"),
                target: *target,
                executable: "dx-check-blocked".to_string(),
                args: vec![reason],
                cwd: root.to_path_buf(),
                detected_from,
                parser: "blocked".to_string(),
            }),
        }
    }

    plans
}

fn blocked_package_manager_plans(
    root: &Path,
    targets: &[DxToolTarget],
    scripts: &serde_json::Map<String, serde_json::Value>,
    error: &PackageManagerSelectionError,
) -> Vec<DxToolPlan> {
    targets
        .iter()
        .filter_map(|target| {
            let script = script_for_target(scripts, *target).script_name()?;
            Some(DxToolPlan {
                id: format!("js-{script}"),
                target: *target,
                executable: "dx-check-blocked".to_string(),
                args: vec![error.reason.clone()],
                cwd: root.to_path_buf(),
                detected_from: error.detected_from.clone(),
                parser: "blocked".to_string(),
            })
        })
        .collect()
}

fn script_for_target(
    scripts: &serde_json::Map<String, serde_json::Value>,
    target: DxToolTarget,
) -> ScriptAssessment {
    match target {
        DxToolTarget::Lint => assess_script_for_target(scripts, "lint", ScriptSafety::Lint),
        DxToolTarget::Format => {
            assess_script_for_target(scripts, "format:check", ScriptSafety::Format)
        }
        DxToolTarget::Typecheck => {
            assess_script_for_target(scripts, "typecheck", ScriptSafety::Typecheck)
        }
        DxToolTarget::Test => assess_script_for_target(scripts, "test", ScriptSafety::Test),
        DxToolTarget::Audit => ScriptAssessment::Missing,
    }
}

enum ScriptAssessment {
    Missing,
    Safe(&'static str),
    Blocked { name: &'static str, reason: String },
}

impl ScriptAssessment {
    fn script_name(&self) -> Option<&'static str> {
        match self {
            Self::Missing => None,
            Self::Safe(name) | Self::Blocked { name, .. } => Some(name),
        }
    }
}

#[derive(Clone, Copy)]
enum ScriptSafety {
    Lint,
    Format,
    Typecheck,
    Test,
}

fn assess_script_for_target(
    scripts: &serde_json::Map<String, serde_json::Value>,
    name: &'static str,
    safety: ScriptSafety,
) -> ScriptAssessment {
    let Some(command) = local_script_command(scripts, name) else {
        return ScriptAssessment::Missing;
    };
    if local_script_has_risk(scripts, name, safety, &mut Vec::new()) {
        return ScriptAssessment::Blocked {
            name,
            reason: blocked_script_reason(name, safety, command),
        };
    }

    ScriptAssessment::Safe(name)
}

fn blocked_script_reason(name: &str, safety: ScriptSafety, command: &str) -> String {
    format!(
        "package.json script `{name}` is not safe for dx check: command `{}` {}. {}",
        summarize_script_command(command),
        safety.risk_summary(),
        safety.next_action(),
    )
}

impl ScriptSafety {
    fn risk_summary(self) -> &'static str {
        match self {
            Self::Lint => "can mutate files or delegate to mutating lint/format scripts",
            Self::Format => "can write files or delegate to mutating format scripts",
            Self::Typecheck => "can emit build artifacts or delegate to build scripts",
            Self::Test => {
                "can write snapshots, coverage, output files, or wait for interactive modes"
            }
        }
    }

    fn next_action(self) -> &'static str {
        match self {
            Self::Lint => "Use a read-only lint script, for example eslint . without --fix.",
            Self::Format => "Use a read-only format check script, for example prettier --check .",
            Self::Typecheck => "Use a no-emit typecheck script, for example tsc --noEmit.",
            Self::Test => "Use a non-interactive test script without snapshot or coverage writes.",
        }
    }
}

fn summarize_script_command(command: &str) -> String {
    const MAX_COMMAND_SUMMARY_BYTES: usize = 180;
    let mut summary = String::with_capacity(command.len().min(MAX_COMMAND_SUMMARY_BYTES));
    let mut last_was_space = false;
    for character in command.chars() {
        if character.is_control() {
            if !last_was_space {
                summary.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if summary.len() + character.len_utf8() > MAX_COMMAND_SUMMARY_BYTES {
            summary.push_str("...");
            break;
        }
        summary.push(character);
        last_was_space = character.is_whitespace();
    }

    let summary = summary.trim();
    if summary.is_empty() {
        "<empty>".to_string()
    } else {
        summary.to_string()
    }
}

fn command_has_write_risk_with_stack(
    scripts: &serde_json::Map<String, serde_json::Value>,
    command: &str,
    stack: &mut Vec<String>,
) -> bool {
    let tokens = script_tokens(command);
    let mut matches_script = |name: &str| {
        mutating_lint_script_name(name)
            || local_script_has_risk(scripts, name, ScriptSafety::Lint, stack)
    };
    tokens_have_format_write_risk(&tokens)
        || tokens_have_shell_output_write_risk(&tokens)
        || tokens_have_inline_javascript_write_risk(&tokens)
        || delegates_to_script_matching(&tokens, &mut matches_script)
}

fn format_check_script_is_safe_with_stack(
    scripts: &serde_json::Map<String, serde_json::Value>,
    command: &str,
    stack: &mut Vec<String>,
) -> bool {
    let tokens = script_tokens(command);
    let mut matches_script = |name: &str| {
        mutating_format_script_name(name)
            || local_script_has_risk(scripts, name, ScriptSafety::Format, stack)
    };
    !tokens_have_format_write_risk(&tokens)
        && !tokens_have_shell_output_write_risk(&tokens)
        && !tokens_have_inline_javascript_write_risk(&tokens)
        && !delegates_to_script_matching(&tokens, &mut matches_script)
}

fn typecheck_script_is_safe_with_stack(
    scripts: &serde_json::Map<String, serde_json::Value>,
    command: &str,
    stack: &mut Vec<String>,
) -> bool {
    let tokens = script_tokens(command);
    let mut matches_script = |name: &str| {
        build_script_name(name)
            || local_script_has_risk(scripts, name, ScriptSafety::Typecheck, stack)
    };
    !tokens
        .iter()
        .any(|token| typecheck_token_has_write_risk(token))
        && !tokens_have_shell_output_write_risk(&tokens)
        && !tokens_have_inline_javascript_write_risk(&tokens)
        && !tokens_have_typecheck_build_tool_risk(&tokens)
        && !delegates_to_script_matching(&tokens, &mut matches_script)
        && (!tokens
            .iter()
            .any(|token| token_is_typescript_compiler(token))
            || tokens_enable_no_emit(&tokens))
}

fn test_script_is_safe_with_stack(
    scripts: &serde_json::Map<String, serde_json::Value>,
    command: &str,
    stack: &mut Vec<String>,
) -> bool {
    let tokens = script_tokens(command);
    let mut matches_script =
        |name: &str| local_script_has_risk(scripts, name, ScriptSafety::Test, stack);
    !tokens_have_shell_output_write_risk(&tokens)
        && !tokens_have_inline_javascript_write_risk(&tokens)
        && !tokens.iter().any(|token| test_token_has_write_risk(token))
        && !tokens_have_watch_by_default_test_runner_risk(&tokens)
        && !delegates_to_script_matching(&tokens, &mut matches_script)
}

fn local_script_has_risk(
    scripts: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    safety: ScriptSafety,
    stack: &mut Vec<String>,
) -> bool {
    let normalized_name = name.to_ascii_lowercase();
    if stack.iter().any(|script| script == &normalized_name) {
        return true;
    }

    let Some(command) = local_script_command(scripts, name) else {
        return false;
    };

    stack.push(normalized_name);
    let command_has_risk = match safety {
        ScriptSafety::Lint => command_has_write_risk_with_stack(scripts, command, stack),
        ScriptSafety::Format => !format_check_script_is_safe_with_stack(scripts, command, stack),
        ScriptSafety::Typecheck => !typecheck_script_is_safe_with_stack(scripts, command, stack),
        ScriptSafety::Test => !test_script_is_safe_with_stack(scripts, command, stack),
    };
    let has_risk = command_has_risk
        || lifecycle_script_has_risk(scripts, "pre", name, safety, stack)
        || lifecycle_script_has_risk(scripts, "post", name, safety, stack);
    stack.pop();
    has_risk
}

fn lifecycle_script_has_risk(
    scripts: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
    name: &str,
    safety: ScriptSafety,
    stack: &mut Vec<String>,
) -> bool {
    local_script_has_risk(scripts, &format!("{prefix}{name}"), safety, stack)
}

fn local_script_command<'a>(
    scripts: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<&'a str> {
    scripts
        .get(name)
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            scripts
                .iter()
                .find(|(script_name, _)| script_name.eq_ignore_ascii_case(name))
                .and_then(|(_, command)| command.as_str())
        })
}

fn tokens_have_format_write_risk(tokens: &[String]) -> bool {
    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "--write"
                | "-w"
                | "--fix"
                | "--fix-only"
                | "--apply"
                | "--cache"
                | "--cache-location"
                | "--output-file"
                | "--outputfile"
                | "-o"
                | "write"
                | "fix"
        ) || token.starts_with("--write=")
            || token.starts_with("--fix=")
            || token.starts_with("--cache=")
            || token.starts_with("--cache-location=")
            || token.starts_with("--output-file=")
            || token.starts_with("--outputfile=")
    })
}

fn tokens_have_shell_output_write_risk(tokens: &[String]) -> bool {
    tokens
        .iter()
        .any(|token| token.contains('>') || matches!(token.as_str(), "tee" | "tee.cmd" | "tee.exe"))
}

fn tokens_have_inline_javascript_write_risk(tokens: &[String]) -> bool {
    tokens
        .iter()
        .enumerate()
        .any(|(index, token)| match token.as_str() {
            "node" | "node.cmd" | "node.exe" | "bun" | "bun.cmd" | "bun.exe" => {
                inline_javascript_args_have_write_risk(&tokens[index + 1..])
            }
            _ => false,
        })
}

fn inline_javascript_args_have_write_risk(tokens: &[String]) -> bool {
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if token == "-e" || token == "--eval" {
            return tokens[index + 1..]
                .iter()
                .take(8)
                .any(|token| inline_javascript_token_has_write_risk(token));
        }
        if let Some(script) = token.strip_prefix("--eval=") {
            return inline_javascript_token_has_write_risk(script)
                || tokens[index + 1..]
                    .iter()
                    .take(8)
                    .any(|token| inline_javascript_token_has_write_risk(token));
        }
        index += 1;
    }
    false
}

fn inline_javascript_token_has_write_risk(token: &str) -> bool {
    [
        "writefile",
        "writefilesync",
        "appendfile",
        "appendfilesync",
        "createwritestream",
        "rm(",
        "rmsync",
        "unlink",
        "unlinksync",
        "rmdir",
        "rmdirsync",
        "mkdir",
        "mkdirsync",
        "rename",
        "renamesync",
        "copyfile",
        "copyfilesync",
        "cpsync",
    ]
    .iter()
    .any(|pattern| token.contains(pattern))
}

fn typecheck_token_has_write_risk(token: &str) -> bool {
    matches!(
        token,
        "--build"
            | "-b"
            | "--declaration"
            | "--emitdeclarationonly"
            | "--generatetrace"
            | "--incremental"
            | "--outdir"
            | "--outfile"
            | "--tsbuildinfofile"
            | "--watch"
            | "-w"
    ) || matches!(token, "--noemit=false" | "--noemit=0")
        || token.starts_with("--declaration=")
        || token.starts_with("--declarationdir=")
        || token.starts_with("--emitdeclarationonly=")
        || token.starts_with("--generatetrace=")
        || token.starts_with("--incremental=")
        || token.starts_with("--outdir=")
        || token.starts_with("--outfile=")
        || token.starts_with("--tsbuildinfofile=")
        || token.starts_with("--watch=")
}

fn token_is_typescript_compiler(token: &str) -> bool {
    matches!(
        token,
        "tsc" | "tsc.cmd" | "tsc.exe" | "vue-tsc" | "vue-tsc.cmd" | "vue-tsc.exe"
    )
}

fn tokens_enable_no_emit(tokens: &[String]) -> bool {
    let mut enabled = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token == "--noemit" {
            match tokens.get(index + 1).map(String::as_str) {
                Some("false" | "0") => {
                    enabled = Some(false);
                    index += 2;
                }
                Some("true" | "1") => {
                    enabled = Some(true);
                    index += 2;
                }
                _ => {
                    enabled = Some(true);
                    index += 1;
                }
            }
            continue;
        }

        if matches!(token.as_str(), "--noemit=true" | "--noemit=1") {
            enabled = Some(true);
        } else if matches!(token.as_str(), "--noemit=false" | "--noemit=0") {
            enabled = Some(false);
        }
        index += 1;
    }
    enabled.unwrap_or(false)
}

fn test_token_has_write_risk(token: &str) -> bool {
    matches!(
        token,
        "-u" | "--coverage"
            | "--outputfile"
            | "--output"
            | "--output-file"
            | "--ui"
            | "--update"
            | "--updatesnapshot"
            | "--update-snapshot"
            | "--update-snapshots"
            | "--watch"
            | "--watchall"
            | "coverage"
            | "watch"
    ) || token.starts_with("--updatesnapshot=")
        || token.starts_with("--coverage=")
        || token.starts_with("--outputfile=")
        || token.starts_with("--output=")
        || token.starts_with("--output-file=")
        || token.starts_with("--ui=")
        || token.starts_with("--update-snapshot=")
        || token.starts_with("--update-snapshots=")
        || (token.starts_with("--watch=") && !disabled_bool_flag(token))
        || (token.starts_with("--watchall=") && !disabled_bool_flag(token))
}

fn disabled_bool_flag(token: &str) -> bool {
    token
        .split_once('=')
        .is_some_and(|(_, value)| matches!(value, "false" | "0"))
}

fn tokens_have_watch_by_default_test_runner_risk(tokens: &[String]) -> bool {
    tokens
        .iter()
        .enumerate()
        .any(|(index, token)| match token.as_str() {
            "vitest" | "vitest.cmd" | "vitest.exe" => {
                !test_runner_has_non_interactive_mode(tokens, index + 1)
            }
            _ => false,
        })
}

fn test_runner_has_non_interactive_mode(tokens: &[String], start: usize) -> bool {
    tokens[start..].iter().any(|token| {
        matches!(
            token.as_str(),
            "run" | "--run" | "--run=true" | "--run=1" | "--watch=false" | "--watch=0"
        )
    })
}

fn tokens_have_typecheck_build_tool_risk(tokens: &[String]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        let next = || tokens.get(index + 1).map(String::as_str);
        match token.as_str() {
            "next" | "next.cmd" | "next.exe" | "vite" | "vite.cmd" | "vite.exe" | "bun"
            | "bun.cmd" | "bun.exe" => next() == Some("build"),
            "webpack" | "webpack.cmd" | "webpack.exe" | "webpack-cli" | "webpack-cli.cmd"
            | "webpack-cli.exe" | "rollup" | "rollup.cmd" | "rollup.exe" | "tsup" | "tsup.cmd"
            | "tsup.exe" | "esbuild" | "esbuild.cmd" | "esbuild.exe" => true,
            _ => false,
        }
    })
}

fn delegates_to_script_matching<F>(tokens: &[String], matches_script: &mut F) -> bool
where
    F: FnMut(&str) -> bool,
{
    tokens
        .iter()
        .enumerate()
        .any(|(index, token)| match token.as_str() {
            "npm" | "npm.cmd" | "npm.exe" => {
                package_manager_runs_script_matching(tokens, index + 1, matches_script)
            }
            "bun" | "bun.cmd" | "bun.exe" => {
                package_manager_runs_script_matching(tokens, index + 1, matches_script)
                    || bun_runs_script_matching(tokens, index + 1, matches_script)
            }
            "pnpm" | "pnpm.cmd" | "pnpm.exe" | "yarn" | "yarn.cmd" | "yarn.exe" => {
                package_manager_runs_script_matching(tokens, index + 1, matches_script)
                    || token_at_matches_script(tokens, index + 1, matches_script)
                    || workspace_command_runs_script_matching(tokens, index + 1, matches_script)
            }
            _ => false,
        })
}

fn mutating_lint_script_name(name: &str) -> bool {
    name == "format" || name.contains("fix") || name.contains("write") || name.contains("apply")
}

fn mutating_format_script_name(name: &str) -> bool {
    name == "format" || name.contains("fix") || name.contains("write") || name.contains("apply")
}

fn build_script_name(name: &str) -> bool {
    matches!(name, "build" | "compile")
}

fn package_manager_runs_script_matching(
    tokens: &[String],
    start: usize,
    matches_script: &mut impl FnMut(&str) -> bool,
) -> bool {
    let Some(run_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };

    package_manager_run_command(&tokens[run_index])
        && first_package_manager_arg_after(tokens, run_index + 1).is_some_and(matches_script)
}

fn bun_runs_script_matching(
    tokens: &[String],
    start: usize,
    matches_script: &mut impl FnMut(&str) -> bool,
) -> bool {
    let Some(command_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };
    let command = tokens[command_index].as_str();
    if bun_builtin_command(command) {
        return false;
    }

    matches_script(command)
}

fn bun_builtin_command(command: &str) -> bool {
    matches!(
        command,
        "add"
            | "create"
            | "exec"
            | "help"
            | "init"
            | "install"
            | "pm"
            | "remove"
            | "repl"
            | "run"
            | "test"
            | "update"
            | "upgrade"
            | "x"
    )
}

fn first_package_manager_arg_after(tokens: &[String], start: usize) -> Option<&str> {
    first_package_manager_arg_index_after(tokens, start).map(|index| tokens[index].as_str())
}

fn first_package_manager_arg_index_after(tokens: &[String], start: usize) -> Option<usize> {
    let mut index = start;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        if token == "--" {
            index += 1;
            continue;
        }
        if token.starts_with("--") {
            let skip_value = package_manager_option_takes_value(token);
            index += 1;
            if skip_value && index < tokens.len() && !tokens[index].starts_with('-') {
                index += 1;
            }
            continue;
        }
        if token.starts_with('-') {
            index += if package_manager_option_takes_value(token) {
                2
            } else {
                1
            };
            continue;
        }
        return Some(index);
    }
    None
}

fn package_manager_option_takes_value(token: &str) -> bool {
    matches!(
        token,
        "--prefix"
            | "--workspace"
            | "--from"
            | "--include"
            | "--exclude"
            | "--jobs"
            | "--since"
            | "--changedsince"
            | "--workspace-concurrency"
            | "--filter"
            | "--dir"
            | "--cwd"
            | "--config"
            | "--cache"
            | "--registry"
            | "--userconfig"
            | "--script-shell"
            | "-C"
            | "-c"
            | "-F"
            | "-f"
            | "-j"
            | "-w"
    )
}

fn token_at_matches_script(
    tokens: &[String],
    start: usize,
    matches_script: &mut impl FnMut(&str) -> bool,
) -> bool {
    first_package_manager_arg_after(tokens, start).is_some_and(matches_script)
}

fn workspace_command_runs_script_matching<F>(
    tokens: &[String],
    start: usize,
    matches_script: &mut F,
) -> bool
where
    F: FnMut(&str) -> bool,
{
    let Some(command_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };

    match tokens[command_index].as_str() {
        "workspace" => {
            workspace_script_runs_script_matching(tokens, command_index + 1, matches_script)
        }
        "workspaces" => {
            workspaces_script_runs_script_matching(tokens, command_index + 1, matches_script)
        }
        "recursive" => {
            script_invocation_runs_script_matching(tokens, command_index + 1, matches_script)
        }
        _ => false,
    }
}

fn workspace_script_runs_script_matching(
    tokens: &[String],
    start: usize,
    matches_script: &mut impl FnMut(&str) -> bool,
) -> bool {
    let Some(workspace_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };
    script_invocation_runs_script_matching(tokens, workspace_index + 1, matches_script)
}

fn workspaces_script_runs_script_matching<F>(
    tokens: &[String],
    start: usize,
    matches_script: &mut F,
) -> bool
where
    F: FnMut(&str) -> bool,
{
    let Some(command_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };

    if tokens[command_index] == "foreach" {
        return script_invocation_runs_script_matching(tokens, command_index + 1, matches_script);
    }

    if package_manager_run_command(&tokens[command_index]) {
        return first_package_manager_arg_after(tokens, command_index + 1)
            .is_some_and(matches_script);
    }

    false
}

fn script_invocation_runs_script_matching(
    tokens: &[String],
    start: usize,
    matches_script: &mut impl FnMut(&str) -> bool,
) -> bool {
    let Some(script_index) = first_package_manager_arg_index_after(tokens, start) else {
        return false;
    };

    matches_script(&tokens[script_index])
        || (package_manager_run_command(&tokens[script_index])
            && first_package_manager_arg_after(tokens, script_index + 1)
                .is_some_and(matches_script))
}

fn package_manager_run_command(token: &str) -> bool {
    matches!(token, "run" | "run-script")
}

fn normalized_script_token(token: &str) -> String {
    token
        .trim_matches(|value| {
            matches!(
                value,
                '"' | '\'' | '`' | ';' | ',' | '(' | ')' | '[' | ']' | '{' | '}' | '$'
            )
        })
        .chars()
        .filter(|value| !matches!(value, '"' | '\'' | '`' | '\\' | '^'))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn script_tokens(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();

    for character in command.chars() {
        if character.is_ascii_whitespace() || matches!(character, '&' | '|' | ';') {
            push_script_token(&mut tokens, &mut token);
            continue;
        }
        token.push(character);
    }

    push_script_token(&mut tokens, &mut token);
    tokens
}

fn push_script_token(tokens: &mut Vec<String>, token: &mut String) {
    if token.is_empty() {
        return;
    }
    tokens.push(normalized_script_token(token));
    token.clear();
}
