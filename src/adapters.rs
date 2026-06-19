use std::path::Path;

pub use crate::model::{DxToolProcessOutput, DxToolRunResult, DxToolRunStatus};

use crate::model::{DxToolPlan, DxToolTarget};

mod biome;
mod c_family;
mod javascript;
mod javascript_package_manager;
mod runner;
mod scripted;
mod web_audit;

pub use runner::{
    blocked_adapter_plan_diagnostic, executable_is_blocked, run_tool_plan,
    run_tool_plan_with_executor,
};

pub fn plan_tools(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    let mut plans = Vec::new();

    if root.join("Cargo.toml").is_file() {
        plans.extend(targets.iter().filter_map(|target| rust_plan(root, *target)));
    }

    let biome_plans = biome::plans(root, targets);
    let biome_lint_format_targets = biome_plans
        .iter()
        .map(|plan| plan.target)
        .collect::<std::collections::BTreeSet<_>>();
    plans.extend(biome_plans);

    let javascript_targets = targets
        .iter()
        .copied()
        .filter(|target| !biome_lint_format_targets.contains(target))
        .collect::<Vec<_>>();
    plans.extend(javascript::package_manager_plans(root, &javascript_targets));
    plans.extend(scripted::python_plans(root, targets));
    plans.extend(scripted::go_plans(root, targets));
    plans.extend(c_family::plans(root, targets));
    plans.extend(web_audit::plans(root, targets));

    plans
}

fn rust_plan(root: &Path, target: DxToolTarget) -> Option<DxToolPlan> {
    match target {
        DxToolTarget::Format => Some(cargo_plan(
            "rustfmt-check",
            DxToolTarget::Format,
            root,
            ["fmt", "--all", "--", "--check"],
            "rustfmt",
        )),
        DxToolTarget::Lint => Some(cargo_plan(
            "rust-clippy",
            DxToolTarget::Lint,
            root,
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "-j",
                "1",
                "--message-format=json",
                "--",
                "-D",
                "warnings",
            ],
            "cargo-json",
        )),
        DxToolTarget::Test => Some(cargo_plan(
            "cargo-test",
            DxToolTarget::Test,
            root,
            ["test", "--workspace", "-j", "1", "--message-format=json"],
            "cargo-json",
        )),
        DxToolTarget::Typecheck => Some(cargo_plan(
            "cargo-check",
            DxToolTarget::Typecheck,
            root,
            ["check", "--workspace", "-j", "1", "--message-format=json"],
            "cargo-json",
        )),
        DxToolTarget::Audit => None,
    }
}

fn cargo_plan<const N: usize>(
    id: &str,
    target: DxToolTarget,
    root: &Path,
    args: [&str; N],
    parser: &str,
) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "cargo".to_string(),
        args: args.into_iter().map(ToOwned::to_owned).collect(),
        cwd: root.to_path_buf(),
        detected_from: vec!["Cargo.toml".to_string()],
        parser: parser.to_string(),
    }
}
