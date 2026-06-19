use std::path::Path;

use crate::model::{DxToolPlan, DxToolTarget, DxWebAuditTarget, DxWebLighthouseMode};
use crate::web_audit::{
    DxWebLighthouseRuntime, WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE, WEB_LIGHTHOUSE_RUNTIMES_SOURCE,
};

const PINNED_LIGHTHOUSE_REPO: &str = "third_party/google-lighthouse";

pub(super) fn plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    if !targets.contains(&DxToolTarget::Audit) {
        return Vec::new();
    }

    let project = crate::web_audit::load_project_web_audit(root);
    let lighthouse_runtime = project.lighthouse_runtime;
    project
        .targets
        .into_iter()
        .map(|target| plan(root, target, lighthouse_runtime.as_ref()))
        .collect()
}

fn plan(
    root: &Path,
    target: DxWebAuditTarget,
    lighthouse_runtime: Option<&DxWebLighthouseRuntime>,
) -> DxToolPlan {
    let mut args = vec![
        "--id".to_string(),
        target.id.clone(),
        "--url".to_string(),
        target.url,
    ];
    if let Some(required_status) = target.required_status {
        args.push("--required-status".to_string());
        args.push(required_status.to_string());
    }
    if let Some(max_html_bytes) = target.max_html_bytes {
        args.push("--max-html-bytes".to_string());
        args.push(max_html_bytes.to_string());
    }
    let mut detected_from = vec!["dx".to_string()];
    let mode = target
        .lighthouse_mode
        .unwrap_or(DxWebLighthouseMode::Official);
    args.push("--lighthouse".to_string());
    args.push(mode.as_str().to_string());
    if matches!(
        mode,
        DxWebLighthouseMode::Official | DxWebLighthouseMode::Auto
    ) {
        if let Some(runtime) = lighthouse_runtime {
            push_lighthouse_runtime_args(&mut args, runtime);
            detected_from.push(WEB_LIGHTHOUSE_RUNTIMES_SOURCE.to_string());
            detected_from.push(WEB_LIGHTHOUSE_EQUIVALENCE_SOURCE.to_string());
        } else {
            let lighthouse_repo = root.join(PINNED_LIGHTHOUSE_REPO);
            if lighthouse_repo.join("cli").join("index.js").is_file() {
                args.push("--lighthouse-repo".to_string());
                args.push(lighthouse_repo.display().to_string());
                detected_from.push(PINNED_LIGHTHOUSE_REPO.to_string());
            }
        }
    }

    DxToolPlan {
        id: format!("web-audit-{}", target.id),
        target: DxToolTarget::Audit,
        executable: "dx-check-web-audit".to_string(),
        args,
        cwd: root.to_path_buf(),
        detected_from,
        parser: "web-audit-json".to_string(),
    }
}

fn push_lighthouse_runtime_args(args: &mut Vec<String>, runtime: &DxWebLighthouseRuntime) {
    args.push("--lighthouse-command".to_string());
    args.push(runtime.command.display().to_string());
    args.push("--lighthouse-command-cwd".to_string());
    args.push(runtime.cwd.display().to_string());
    for arg in &runtime.args {
        args.push("--lighthouse-command-arg".to_string());
        args.push(arg.clone());
    }
}
