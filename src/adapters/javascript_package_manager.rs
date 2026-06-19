use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PackageManagerSelectionError {
    pub(super) reason: String,
    pub(super) detected_from: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PackageManager {
    pub(super) executable: &'static str,
    pub(super) lockfile: Option<&'static str>,
}

impl PackageManager {
    pub(super) fn executable_name(self) -> String {
        executable_name(self.executable)
    }

    pub(super) fn args_for_script(self, script: &str) -> Vec<String> {
        match self.executable {
            "npm" => vec![
                "run".to_string(),
                "--silent".to_string(),
                script.to_string(),
            ],
            _ => vec!["run".to_string(), script.to_string()],
        }
    }

    pub(super) fn args_for_biome(self, command: &str, paths: &[String]) -> Vec<String> {
        let mut args = match self.executable {
            "npm" => vec!["exec", "--", "biome"],
            "bun" => vec!["x", "biome"],
            _ => vec!["exec", "biome"],
        }
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

        args.extend([
            command.to_string(),
            "--reporter=json".to_string(),
            "--max-diagnostics=none".to_string(),
            "--colors=off".to_string(),
            "--no-errors-on-unmatched".to_string(),
        ]);
        if paths.is_empty() {
            args.push(".".to_string());
        } else {
            args.extend(paths.iter().cloned());
        }
        args
    }
}

pub(super) fn package_manager(root: &Path) -> Result<PackageManager, PackageManagerSelectionError> {
    let lockfiles = existing_lockfiles(root);

    if let Some(manager) = declared_package_manager(root)? {
        return Ok(manager);
    }

    let manager_count = lockfiles
        .iter()
        .map(|lockfile| lockfile.executable)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if manager_count > 1 {
        let names = lockfiles
            .iter()
            .map(|lockfile| lockfile.name)
            .collect::<Vec<_>>();
        return Err(PackageManagerSelectionError {
            reason: format!(
                "multiple JavaScript lockfiles were found without packageManager in package.json; add packageManager or remove stale lockfiles before dx-check selects a package manager: {}",
                names.join(", ")
            ),
            detected_from: std::iter::once("package.json".to_string())
                .chain(names.iter().map(|name| (*name).to_string()))
                .collect(),
        });
    }

    if let Some(lockfile) = lockfiles.first() {
        return Ok(PackageManager {
            executable: lockfile.executable,
            lockfile: Some(lockfile.name),
        });
    }

    Ok(PackageManager {
        executable: "npm",
        lockfile: None,
    })
}

fn declared_package_manager(
    root: &Path,
) -> Result<Option<PackageManager>, PackageManagerSelectionError> {
    let body = fs::read_to_string(root.join("package.json")).ok();
    let Some(body) = body else {
        return Ok(None);
    };
    let package = serde_json::from_str::<serde_json::Value>(&body).ok();
    let Some(package) = package else {
        return Ok(None);
    };
    let Some(package_manager) = package
        .get("packageManager")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
    else {
        return Ok(None);
    };
    let Some((name, version)) = package_manager.split_once('@') else {
        return Err(invalid_declared_package_manager(package_manager));
    };
    let name = name.trim();
    let version = version.trim();
    if name.is_empty() || version.is_empty() || version.chars().any(char::is_whitespace) {
        return Err(invalid_declared_package_manager(package_manager));
    }

    match name {
        "bun" => Ok(Some(PackageManager {
            executable: "bun",
            lockfile: first_existing_lockfile(root, &["bun.lock", "bun.lockb"]),
        })),
        "pnpm" => Ok(Some(PackageManager {
            executable: "pnpm",
            lockfile: first_existing_lockfile(root, &["pnpm-lock.yaml"]),
        })),
        "yarn" => Ok(Some(PackageManager {
            executable: "yarn",
            lockfile: first_existing_lockfile(root, &["yarn.lock"]),
        })),
        "npm" => Ok(Some(PackageManager {
            executable: "npm",
            lockfile: first_existing_lockfile(root, &["package-lock.json"]),
        })),
        _ => Err(PackageManagerSelectionError {
            reason: format!(
                "package.json declares unsupported packageManager `{package_manager}`; use npm, pnpm, yarn, or bun before dx-check selects JavaScript tools"
            ),
            detected_from: vec!["package.json".to_string()],
        }),
    }
}

#[derive(Debug, Clone, Copy)]
struct PackageManagerLockfile {
    executable: &'static str,
    name: &'static str,
}

fn existing_lockfiles(root: &Path) -> Vec<PackageManagerLockfile> {
    [
        PackageManagerLockfile {
            executable: "bun",
            name: "bun.lock",
        },
        PackageManagerLockfile {
            executable: "bun",
            name: "bun.lockb",
        },
        PackageManagerLockfile {
            executable: "pnpm",
            name: "pnpm-lock.yaml",
        },
        PackageManagerLockfile {
            executable: "yarn",
            name: "yarn.lock",
        },
        PackageManagerLockfile {
            executable: "npm",
            name: "package-lock.json",
        },
    ]
    .into_iter()
    .filter(|lockfile| root.join(lockfile.name).is_file())
    .collect()
}

fn first_existing_lockfile(root: &Path, names: &[&'static str]) -> Option<&'static str> {
    names.iter().copied().find(|name| root.join(name).is_file())
}

fn invalid_declared_package_manager(package_manager: &str) -> PackageManagerSelectionError {
    PackageManagerSelectionError {
        reason: format!(
            "package.json declares invalid packageManager `{package_manager}`; use the `<manager>@<version>` format before dx-check selects JavaScript tools"
        ),
        detected_from: vec!["package.json".to_string()],
    }
}

#[cfg(windows)]
fn executable_name(base: &str) -> String {
    if matches!(base, "npm" | "pnpm" | "yarn" | "bun") {
        format!("{base}.cmd")
    } else {
        base.to_string()
    }
}

#[cfg(not(windows))]
fn executable_name(base: &str) -> String {
    base.to_string()
}
