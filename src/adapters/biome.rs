use std::fs;
use std::path::Path;

use crate::model::{DxToolPlan, DxToolTarget};
use serializer::{DxBiomeConfig, DxBiomeTarget, load_biome_config};

use super::javascript_package_manager::package_manager;

pub(super) fn plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    let package_json = root.join("package.json");
    if !package_json.is_file() {
        return Vec::new();
    }

    let Ok(body) = std::fs::read_to_string(&package_json) else {
        return Vec::new();
    };
    if serde_json::from_str::<serde_json::Value>(&body).is_err() {
        return Vec::new();
    }

    let dx_path = root.join("dx");
    let config = match load_biome_config(&dx_path) {
        Ok(Some(config)) => config,
        Ok(None) => return Vec::new(),
        Err(error) => {
            if dx_declares_biome(&dx_path) {
                return blocked_plans(
                    root,
                    targets,
                    &format!("invalid Biome dx config: {error}"),
                    &["dx"],
                );
            }
            return Vec::new();
        }
    };

    if !package_declares_biome(&body) {
        let detected_from = ["package.json".to_string(), "dx".to_string()];
        return blocked_configured_plans(
            root,
            targets,
            &config,
            "package.json must declare @biomejs/biome before dx-check runs Biome",
            &detected_from,
        );
    }

    let manager = match package_manager(root) {
        Ok(manager) => manager,
        Err(error) => {
            let detected_from = std::iter::once("dx".to_string())
                .chain(error.detected_from)
                .collect::<Vec<_>>();
            return blocked_configured_plans(root, targets, &config, &error.reason, &detected_from);
        }
    };
    let mut detected_from = vec!["package.json".to_string(), "dx".to_string()];
    if let Some(lockfile) = manager.lockfile {
        detected_from.push(lockfile.to_string());
    }

    let mut plans = Vec::new();
    for target in targets {
        match target {
            DxToolTarget::Lint => {
                let paths = config.paths_for(DxBiomeTarget::Lint);
                if paths.is_empty() {
                    continue;
                }
                if let Some(reason) = invalid_paths_reason(root, &paths) {
                    plans.push(blocked_plan(
                        "biome-lint",
                        DxToolTarget::Lint,
                        root,
                        &reason,
                        &["package.json", "dx"],
                    ));
                } else {
                    plans.push(plan(
                        "biome-lint",
                        DxToolTarget::Lint,
                        root,
                        &manager,
                        &detected_from,
                        "lint",
                        &paths,
                    ));
                }
            }
            DxToolTarget::Format => {
                let paths = config.paths_for(DxBiomeTarget::Format);
                if paths.is_empty() {
                    continue;
                }
                if let Some(reason) = invalid_paths_reason(root, &paths) {
                    plans.push(blocked_plan(
                        "biome-format",
                        DxToolTarget::Format,
                        root,
                        &reason,
                        &["package.json", "dx"],
                    ));
                } else {
                    plans.push(plan(
                        "biome-format",
                        DxToolTarget::Format,
                        root,
                        &manager,
                        &detected_from,
                        "format",
                        &paths,
                    ));
                }
            }
            DxToolTarget::Typecheck | DxToolTarget::Test | DxToolTarget::Audit => {}
        }
    }

    plans
}

const MAX_BIOME_INPUT_SCAN_FILES: usize = 4_000;

fn invalid_paths_reason(root: &Path, paths: &[String]) -> Option<String> {
    let option_like = paths
        .iter()
        .filter(|path| path.trim_start().starts_with('-'))
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>();
    if !option_like.is_empty() {
        return Some(format!(
            "Biome dx config path {} is invalid: path cannot look like a command-line option",
            option_like.join(", ")
        ));
    }

    let missing = paths
        .iter()
        .filter(|path| !root.join(path.as_str()).exists())
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        return Some(format!(
            "Biome dx config path {} does not exist; create it or remove the stale entry before dx-check runs Biome",
            missing.join(", ")
        ));
    }

    let Ok(canonical_root) = fs::canonicalize(root) else {
        return Some(
            "Biome dx config paths could not be validated because the project root could not be resolved"
                .to_string(),
        );
    };

    let outside = paths
        .iter()
        .filter(|path| !path_resolves_inside(&canonical_root, &root.join(path.as_str())))
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>();
    if !outside.is_empty() {
        return Some(format!(
            "Biome dx config path {} resolves outside the project; keep Biome paths inside the project root before dx-check runs Biome",
            outside.join(", ")
        ));
    }

    let reserved = paths
        .iter()
        .filter(|path| biome_reserved_path_component(path).is_some())
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>();
    if !reserved.is_empty() {
        return Some(format!(
            "Biome dx config path {} targets a generated/dependency directory; choose first-party source paths before dx-check runs Biome",
            reserved.join(", ")
        ));
    }

    let empty = paths
        .iter()
        .filter(|path| !path_contains_biome_inputs(&canonical_root, &root.join(path.as_str())))
        .map(|path| format!("`{path}`"))
        .collect::<Vec<_>>();
    if empty.is_empty() {
        None
    } else {
        Some(format!(
            "Biome dx config path {} contains no Biome-supported files; add supported JS/TS/JSON/CSS sources or remove the stale entry before dx-check runs Biome",
            empty.join(", ")
        ))
    }
}

fn path_contains_biome_inputs(canonical_root: &Path, path: &Path) -> bool {
    if path.is_file() {
        return path_resolves_inside(canonical_root, path) && is_biome_supported_file(path);
    }
    if !path.is_dir() {
        return false;
    }
    if skip_biome_input_dir(path) {
        return false;
    }

    let mut stack = vec![path.to_path_buf()];
    let mut scanned = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !skip_biome_input_dir(&child) && path_resolves_inside(canonical_root, &child) {
                    stack.push(child);
                }
                continue;
            }
            if file_type.is_file() && path_resolves_inside(canonical_root, &child) {
                scanned += 1;
                if is_biome_supported_file(&child) {
                    return true;
                }
                if scanned >= MAX_BIOME_INPUT_SCAN_FILES {
                    return false;
                }
            }
        }
    }
    false
}

fn path_resolves_inside(canonical_root: &Path, path: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|canonical| canonical.starts_with(canonical_root))
}

fn skip_biome_input_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    biome_reserved_dir_name(name).is_some()
}

fn biome_reserved_path_component(path: &str) -> Option<&'static str> {
    Path::new(path)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .find_map(biome_reserved_dir_name)
}

fn biome_reserved_dir_name(name: &str) -> Option<&'static str> {
    let normalized = name.to_ascii_lowercase();
    Some(match normalized.as_str() {
        ".dx" => ".dx",
        ".git" => ".git",
        "build" => "build",
        "dist" => "dist",
        "node_modules" => "node_modules",
        "out" => "out",
        "target" => "target",
        "third_party" => "third_party",
        "vendor" => "vendor",
        _ => return None,
    })
}

fn is_biome_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "astro"
                    | "cjs"
                    | "css"
                    | "cts"
                    | "gql"
                    | "graphql"
                    | "js"
                    | "json"
                    | "jsonc"
                    | "jsx"
                    | "mjs"
                    | "mts"
                    | "svelte"
                    | "ts"
                    | "tsx"
                    | "vue"
            )
        })
}

fn dx_declares_biome(path: &Path) -> bool {
    let Ok(source) = std::fs::read_to_string(path) else {
        return false;
    };
    source.lines().any(|line| {
        let line = line.trim_start();
        line == "biome"
            || line.starts_with("biome[")
            || line.starts_with("biome(")
            || line.starts_with("biome ")
    })
}

fn package_declares_biome(body: &str) -> bool {
    let Ok(package) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ]
    .iter()
    .any(|field| {
        package
            .get(field)
            .and_then(serde_json::Value::as_object)
            .is_some_and(|dependencies| dependencies.contains_key("@biomejs/biome"))
    })
}

fn blocked_plans(
    root: &Path,
    targets: &[DxToolTarget],
    reason: &str,
    detected_from: &[&str],
) -> Vec<DxToolPlan> {
    targets
        .iter()
        .filter_map(|target| match target {
            DxToolTarget::Lint => Some(blocked_plan(
                "biome-lint",
                DxToolTarget::Lint,
                root,
                reason,
                detected_from,
            )),
            DxToolTarget::Format => Some(blocked_plan(
                "biome-format",
                DxToolTarget::Format,
                root,
                reason,
                detected_from,
            )),
            DxToolTarget::Typecheck | DxToolTarget::Test | DxToolTarget::Audit => None,
        })
        .collect()
}

fn blocked_configured_plans(
    root: &Path,
    targets: &[DxToolTarget],
    config: &DxBiomeConfig,
    reason: &str,
    detected_from: &[String],
) -> Vec<DxToolPlan> {
    let mut plans = Vec::new();
    for target in targets {
        match target {
            DxToolTarget::Lint if config.is_enabled_for(DxBiomeTarget::Lint) => {
                plans.push(blocked_plan(
                    "biome-lint",
                    DxToolTarget::Lint,
                    root,
                    reason,
                    detected_from,
                ));
            }
            DxToolTarget::Format if config.is_enabled_for(DxBiomeTarget::Format) => {
                plans.push(blocked_plan(
                    "biome-format",
                    DxToolTarget::Format,
                    root,
                    reason,
                    detected_from,
                ));
            }
            _ => {}
        }
    }
    plans
}

fn blocked_plan<S: AsRef<str>>(
    id: &str,
    target: DxToolTarget,
    root: &Path,
    reason: &str,
    detected_from: &[S],
) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "dx-check-blocked".to_string(),
        args: vec![reason.to_string()],
        cwd: root.to_path_buf(),
        detected_from: detected_from
            .iter()
            .map(|source| source.as_ref().to_string())
            .collect(),
        parser: "blocked".to_string(),
    }
}

fn plan(
    id: &str,
    target: DxToolTarget,
    root: &Path,
    manager: &super::javascript_package_manager::PackageManager,
    detected_from: &[String],
    command: &str,
    paths: &[String],
) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: manager.executable_name(),
        args: manager.args_for_biome(command, paths),
        cwd: root.to_path_buf(),
        detected_from: detected_from.to_vec(),
        parser: "biome-json".to_string(),
    }
}
