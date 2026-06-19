use std::path::Path;

use crate::model::{DxToolPlan, DxToolTarget};

pub(super) fn python_plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    let has_pyproject = root.join("pyproject.toml").is_file();
    let has_ruff = root.join("ruff.toml").is_file() || pyproject_has_tool(root, "ruff");
    let has_black = pyproject_has_tool(root, "black");
    if !(has_pyproject || has_ruff) {
        return Vec::new();
    }
    let detected_from = if has_pyproject {
        "pyproject.toml"
    } else {
        "ruff.toml"
    };

    let mut plans = Vec::new();
    for target in targets {
        let (id, executable, args, parser): (&str, &str, Vec<&str>, &str) = match target {
            DxToolTarget::Lint if has_ruff => (
                "ruff-check",
                "ruff",
                vec!["check", "--output-format=json", "."],
                "ruff-json",
            ),
            DxToolTarget::Lint => continue,
            DxToolTarget::Format if has_black => (
                "black-format-check",
                "black",
                vec!["--check", "--diff", "."],
                "black",
            ),
            DxToolTarget::Format if has_ruff => (
                "ruff-format-check",
                "ruff",
                vec!["format", "--check", "."],
                "ruff-format",
            ),
            DxToolTarget::Format => continue,
            DxToolTarget::Test => ("pytest", "python", vec!["-m", "pytest"], "pytest"),
            DxToolTarget::Typecheck => continue,
            DxToolTarget::Audit => continue,
        };
        plans.push(DxToolPlan {
            id: id.to_string(),
            target: *target,
            executable: executable.to_string(),
            args: args.into_iter().map(ToOwned::to_owned).collect(),
            cwd: root.to_path_buf(),
            detected_from: vec![detected_from.to_string()],
            parser: parser.to_string(),
        });
    }
    plans
}

pub(super) fn go_plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    if !root.join("go.mod").is_file() {
        return Vec::new();
    }

    let mut plans = Vec::new();
    for target in targets {
        match target {
            DxToolTarget::Format => plans.push(DxToolPlan {
                id: "gofmt-check".to_string(),
                target: *target,
                executable: "gofmt".to_string(),
                args: vec!["-l".to_string(), ".".to_string()],
                cwd: root.to_path_buf(),
                detected_from: vec!["go.mod".to_string()],
                parser: "gofmt-list".to_string(),
            }),
            DxToolTarget::Test => plans.push(DxToolPlan {
                id: "go-test".to_string(),
                target: *target,
                executable: "go".to_string(),
                args: vec!["test".to_string(), "./...".to_string()],
                cwd: root.to_path_buf(),
                detected_from: vec!["go.mod".to_string()],
                parser: "go-test".to_string(),
            }),
            DxToolTarget::Lint => plans.push(DxToolPlan {
                id: "go-vet".to_string(),
                target: *target,
                executable: "go".to_string(),
                args: vec!["vet".to_string(), "./...".to_string()],
                cwd: root.to_path_buf(),
                detected_from: vec!["go.mod".to_string()],
                parser: "go-vet".to_string(),
            }),
            DxToolTarget::Typecheck | DxToolTarget::Audit => {}
        }
    }
    plans
}

fn pyproject_has_tool(root: &Path, tool: &str) -> bool {
    let Ok(body) = std::fs::read_to_string(root.join("pyproject.toml")) else {
        return false;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&body) else {
        return false;
    };
    value
        .get("tool")
        .and_then(|tool_table| tool_table.get(tool))
        .is_some()
}
