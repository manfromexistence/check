use std::process::ExitCode;

use dx_check_engine::web_audit_runner::{
    DxWebAuditRunnerRequest, DxWebLighthouseCommand, DxWebLighthouseMode, run_web_audit,
    validate_web_audit_runner_request,
};

fn main() -> ExitCode {
    let request = match parse_args(std::env::args().skip(1)) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    let output = match run_web_audit(&request) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    match serde_json::to_writer(std::io::stdout(), &output) {
        Ok(()) if output.has_failure() => ExitCode::from(1),
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("web audit JSON could not be written: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<DxWebAuditRunnerRequest, String> {
    let mut id = None;
    let mut url = None;
    let mut required_status = None;
    let mut max_html_bytes = None;
    let mut timeout_seconds = 8;
    let mut lighthouse_timeout_seconds = None;
    let mut lighthouse_mode = None;
    let mut lighthouse_json = None;
    let mut lighthouse_binary = None;
    let mut lighthouse_repo = None;
    let mut lighthouse_command = None;
    let mut lighthouse_command_args = Vec::new();
    let mut lighthouse_command_cwd = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--id" => id = Some(required_value("--id", args.next())?),
            "--url" => url = Some(required_value("--url", args.next())?),
            "--required-status" => {
                required_status = Some(parse_status(&required_value(
                    "--required-status",
                    args.next(),
                )?)?)
            }
            "--max-html-bytes" => {
                max_html_bytes = Some(parse_u64(
                    "--max-html-bytes",
                    &required_value("--max-html-bytes", args.next())?,
                )?)
            }
            "--timeout-seconds" => {
                timeout_seconds = parse_u64(
                    "--timeout-seconds",
                    &required_value("--timeout-seconds", args.next())?,
                )?
            }
            "--lighthouse-timeout-seconds" => {
                lighthouse_timeout_seconds = Some(parse_u64(
                    "--lighthouse-timeout-seconds",
                    &required_value("--lighthouse-timeout-seconds", args.next())?,
                )?)
            }
            "--lighthouse" => {
                lighthouse_mode = Some(parse_lighthouse_mode(&required_value(
                    "--lighthouse",
                    args.next(),
                )?)?)
            }
            "--lighthouse-json" => {
                let path = required_value("--lighthouse-json", args.next())?;
                lighthouse_json =
                    Some(std::fs::read_to_string(&path).map_err(|error| {
                        format!("--lighthouse-json could not be read: {error}")
                    })?);
                lighthouse_mode.get_or_insert(DxWebLighthouseMode::Official);
            }
            "--lighthouse-bin" => {
                lighthouse_binary = Some(required_value("--lighthouse-bin", args.next())?);
                lighthouse_mode.get_or_insert(DxWebLighthouseMode::Official);
            }
            "--lighthouse-command" => {
                lighthouse_command = Some(required_value("--lighthouse-command", args.next())?);
                lighthouse_mode.get_or_insert(DxWebLighthouseMode::Official);
            }
            "--lighthouse-command-arg" => {
                lighthouse_command_args
                    .push(required_value("--lighthouse-command-arg", args.next())?);
            }
            "--lighthouse-command-cwd" => {
                lighthouse_command_cwd = Some(std::path::PathBuf::from(required_value(
                    "--lighthouse-command-cwd",
                    args.next(),
                )?));
            }
            "--lighthouse-repo" => {
                lighthouse_repo = Some(std::path::PathBuf::from(required_value(
                    "--lighthouse-repo",
                    args.next(),
                )?));
                lighthouse_mode.get_or_insert(DxWebLighthouseMode::Official);
            }
            "--help" | "-h" => return Err(help_text()),
            _ => return Err(format!("unsupported web audit argument `{arg}`")),
        }
    }

    if lighthouse_command.is_none()
        && (!lighthouse_command_args.is_empty() || lighthouse_command_cwd.is_some())
    {
        return Err("a Lighthouse command field requires --lighthouse-command".to_string());
    }

    let mut request = DxWebAuditRunnerRequest::new(
        id.ok_or_else(|| "--id is required".to_string())?,
        url.ok_or_else(|| "--url is required".to_string())?,
    );
    request.required_status = required_status;
    request.max_html_bytes = max_html_bytes;
    request.timeout_seconds = timeout_seconds;
    if let Some(lighthouse_timeout_seconds) = lighthouse_timeout_seconds {
        request.lighthouse_timeout_seconds = lighthouse_timeout_seconds;
    }
    if let Some(lighthouse_mode) = lighthouse_mode {
        request.lighthouse_mode = lighthouse_mode;
    }
    request.lighthouse_json = lighthouse_json;
    request.lighthouse_binary = lighthouse_binary;
    request.lighthouse_repo = lighthouse_repo;
    request.lighthouse_command = lighthouse_command.map(|executable| DxWebLighthouseCommand {
        executable,
        args: lighthouse_command_args,
        cwd: lighthouse_command_cwd,
    });
    validate_web_audit_runner_request(&request)?;
    Ok(request)
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_status(value: &str) -> Result<u16, String> {
    let status = value
        .parse::<u16>()
        .map_err(|_| format!("--required-status must be an HTTP status code, got `{value}`"))?;
    if !(100..=599).contains(&status) {
        return Err(format!(
            "--required-status must be between 100 and 599, got `{value}`"
        ));
    }
    Ok(status)
}

fn parse_u64(flag: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be a non-negative integer, got `{value}`"))
}

fn parse_lighthouse_mode(value: &str) -> Result<DxWebLighthouseMode, String> {
    DxWebLighthouseMode::parse(value)
        .ok_or_else(|| format!("--lighthouse must be native, official, or auto, got `{value}`"))
}

fn help_text() -> String {
    "usage: dx-check-web-audit --id <target-id> --url <http-url> [--required-status <status>] [--max-html-bytes <bytes>] [--timeout-seconds <seconds>] [--lighthouse native|official|auto] [--lighthouse-timeout-seconds <seconds>] [--lighthouse-json <path>] [--lighthouse-bin <command>] [--lighthouse-command <command>] [--lighthouse-command-arg <arg>] [--lighthouse-command-cwd <path>] [--lighthouse-repo <path>]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_lighthouse_flags() {
        let request = parse_args([
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "https://example.com/".to_string(),
            "--lighthouse".to_string(),
            "official".to_string(),
            "--lighthouse-timeout-seconds".to_string(),
            "180".to_string(),
            "--lighthouse-bin".to_string(),
            "lighthouse".to_string(),
        ])
        .expect("parsed request");

        assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
        assert_eq!(request.lighthouse_timeout_seconds, 180);
        assert_eq!(request.lighthouse_binary.as_deref(), Some("lighthouse"));
    }

    #[test]
    fn parses_official_lighthouse_repo_flags() {
        let request = parse_args([
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "https://example.com/".to_string(),
            "--lighthouse".to_string(),
            "official".to_string(),
            "--lighthouse-repo".to_string(),
            "third_party/google-lighthouse".to_string(),
        ])
        .expect("parsed request");

        assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
        assert_eq!(
            request
                .lighthouse_repo
                .as_ref()
                .map(|path| path.as_os_str()),
            Some(std::ffi::OsStr::new("third_party/google-lighthouse"))
        );
    }

    #[test]
    fn defaults_to_official_lighthouse_mode() {
        let request = parse_args([
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "https://example.com/".to_string(),
        ])
        .expect("parsed request");

        assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
    }

    #[test]
    fn parses_structured_lighthouse_command_flags() {
        let request = parse_args([
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "https://example.com/".to_string(),
            "--lighthouse-command".to_string(),
            "G:\\Dx\\bin\\dx.exe".to_string(),
            "--lighthouse-command-cwd".to_string(),
            "G:\\Dx".to_string(),
            "--lighthouse-command-arg".to_string(),
            "js".to_string(),
            "--lighthouse-command-arg".to_string(),
            "lighthouse".to_string(),
        ])
        .expect("parsed request");

        let command = request
            .lighthouse_command
            .as_ref()
            .expect("lighthouse command");
        assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
        assert_eq!(command.executable, "G:\\Dx\\bin\\dx.exe");
        assert_eq!(command.cwd.as_deref(), Some(std::path::Path::new("G:\\Dx")));
        assert_eq!(command.args, ["js", "lighthouse"]);
    }

    #[test]
    fn rejects_noncanonical_lighthouse_command_args() {
        for extra in [
            vec!["--lighthouse-command", "G:\\Dx\\bin\\dx.exe"],
            vec![
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "lighthouse",
                "--lighthouse-command-arg",
                "js",
            ],
            vec![
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "js",
                "--lighthouse-command-arg",
                "lighthouse",
                "--lighthouse-command-arg",
                "--contract",
            ],
            vec![
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "js",
                "--lighthouse-command-arg",
                "lighthouse",
                "--lighthouse-command-arg",
                "https://example.com/",
            ],
        ] {
            let error =
                parse_cli(&extra).expect_err("noncanonical lighthouse command args must fail");
            assert!(error.contains("must be exactly `js lighthouse`"), "{error}");
        }
    }

    #[test]
    fn rejects_orphan_lighthouse_command_fields() {
        for extra in [
            vec!["--lighthouse-command-arg", "js"],
            vec!["--lighthouse-command-cwd", "G:\\Dx"],
        ] {
            let error = parse_cli(&extra).expect_err("orphan command field must fail");
            assert!(error.contains("requires --lighthouse-command"), "{error}");
        }
    }

    #[test]
    fn rejects_ambiguous_lighthouse_sources() {
        for extra in [
            vec![
                "--lighthouse-bin",
                "lighthouse",
                "--lighthouse-repo",
                "third_party/google-lighthouse",
            ],
            vec![
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "js",
                "--lighthouse-command-arg",
                "lighthouse",
                "--lighthouse-bin",
                "lighthouse",
            ],
            vec![
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "js",
                "--lighthouse-command-arg",
                "lighthouse",
                "--lighthouse-repo",
                "third_party/google-lighthouse",
            ],
        ] {
            let error = parse_cli(&extra).expect_err("ambiguous lighthouse source must fail");
            assert!(
                error.contains("choose only one Lighthouse source"),
                "{error}"
            );
        }
    }

    #[test]
    fn parses_lighthouse_json_file() {
        let file = tempfile::NamedTempFile::new().expect("temp lighthouse json");
        std::fs::write(file.path(), r#"{"lighthouseVersion":"13.3.0"}"#)
            .expect("wrote lighthouse json");

        let request = parse_args([
            "--id".to_string(),
            "home".to_string(),
            "--url".to_string(),
            "https://example.com/".to_string(),
            "--lighthouse-json".to_string(),
            file.path().display().to_string(),
        ])
        .expect("parsed request");

        assert_eq!(request.lighthouse_mode, DxWebLighthouseMode::Official);
        assert!(
            request
                .lighthouse_json
                .as_deref()
                .is_some_and(|json| { json.contains(r#""lighthouseVersion":"13.3.0""#) })
        );
    }

    #[test]
    fn rejects_lighthouse_json_combined_with_execution_source() {
        let file = tempfile::NamedTempFile::new().expect("temp lighthouse json");
        std::fs::write(file.path(), r#"{"lighthouseVersion":"13.3.0"}"#)
            .expect("wrote lighthouse json");
        let path = file.path().display().to_string();

        let args = vec![
            "--lighthouse-json",
            path.as_str(),
            "--lighthouse-bin",
            "lighthouse",
        ];
        let error = parse_cli(&args).expect_err("json import plus binary must be ambiguous");

        assert!(
            error.contains("choose only one Lighthouse source"),
            "{error}"
        );
    }

    #[test]
    fn rejects_native_lighthouse_mode_with_official_source_flags() {
        for extra in [
            vec!["--lighthouse", "native", "--lighthouse-bin", "lighthouse"],
            vec![
                "--lighthouse",
                "native",
                "--lighthouse-command",
                "G:\\Dx\\bin\\dx.exe",
                "--lighthouse-command-arg",
                "js",
                "--lighthouse-command-arg",
                "lighthouse",
            ],
            vec![
                "--lighthouse",
                "native",
                "--lighthouse-repo",
                "third_party/google-lighthouse",
            ],
        ] {
            let error = parse_cli(&extra)
                .expect_err("native mode must reject official Lighthouse source flags");
            assert!(error.contains("official Lighthouse source"), "{error}");
        }
    }

    fn parse_cli(extra: &[&str]) -> Result<DxWebAuditRunnerRequest, String> {
        let mut args = vec!["--id", "home", "--url", "https://example.com/"];
        args.extend_from_slice(extra);
        parse_args(args.into_iter().map(str::to_string))
    }
}
