use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::languages::{is_c_family_source_or_header, is_cmake_file};
use crate::path_filters::should_skip_generated_or_dependency_dir;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub relative_path: String,
    pub bytes: u64,
    pub line_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectInventory {
    pub root: PathBuf,
    pub files: Vec<SourceFile>,
    pub contains_node_modules: bool,
}

pub fn scan_project(root: &Path) -> Result<ProjectInventory> {
    let mut inventory = ProjectInventory {
        root: root.to_path_buf(),
        ..ProjectInventory::default()
    };
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > 10 {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_dir() {
                if name.eq_ignore_ascii_case("node_modules") {
                    inventory.contains_node_modules = true;
                    continue;
                }
                if should_skip_dir(&name) {
                    continue;
                }
                if is_approved_generated_cache_artifact(root, &path) {
                    continue;
                }
                stack.push((path, depth + 1));
                continue;
            }

            if !path.is_file() || !is_source_or_config_file(&path) {
                continue;
            }
            if is_approved_generated_cache_artifact(root, &path) {
                continue;
            }

            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let line_count = fs::read_to_string(&path)
                .map(|content| content.lines().count())
                .unwrap_or(0);
            inventory.files.push(SourceFile {
                relative_path: relative_path(root, &path),
                path,
                bytes: metadata.len(),
                line_count,
            });
        }
    }

    inventory
        .files
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(inventory)
}

fn should_skip_dir(name: &str) -> bool {
    should_skip_generated_or_dependency_dir(name)
}

fn is_source_or_config_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name == "dx" {
        return true;
    }
    if is_c_family_source_or_header(path) || is_cmake_file(path) {
        return true;
    }

    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some(
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "md"
                | "mdx"
                | "toml"
                | "yaml"
                | "yml"
                | "json"
                | "sr"
                | "machine"
        )
    )
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_approved_generated_cache_artifact(root: &Path, path: &Path) -> bool {
    let relative = relative_path(root, path);
    is_approved_machine_cache_relative_path(&relative)
}

pub(crate) fn is_approved_machine_cache_relative_path(relative_path: &str) -> bool {
    let normalized = relative_path.to_ascii_lowercase();
    [
        ".dx/serializer",
        ".dx/check",
        ".dx/cli",
        ".dx/www",
        ".dx/icon",
        ".dx/media",
        ".dx/dcp/cache",
    ]
    .iter()
    .any(|root| is_relative_path_at_or_below(&normalized, root))
}

fn is_relative_path_at_or_below(normalized: &str, root: &str) -> bool {
    normalized == root
        || normalized.starts_with(&format!("{root}/"))
        || normalized.ends_with(&format!("/{root}"))
        || normalized.contains(&format!("/{root}/"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::inventory::scan_project;

    #[test]
    fn scan_project_ignores_generated_dx_receipts() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".dx").join("receipts").join("check")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();
        fs::write(
            temp.path()
                .join(".dx")
                .join("receipts")
                .join("check")
                .join("check-latest.json"),
            "{}\n",
        )
        .unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert!(
            inventory
                .files
                .iter()
                .any(|file| file.relative_path == "src/lib.rs")
        );
        assert!(
            !inventory
                .files
                .iter()
                .any(|file| file.relative_path.contains(".dx/receipts"))
        );
    }

    #[test]
    fn scan_project_ignores_generated_serializer_machine_artifacts() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".dx").join("serializer")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src").join("generated.machine"), "cache").unwrap();
        fs::write(
            temp.path()
                .join(".dx")
                .join("serializer")
                .join("check-launch-report.machine"),
            "generated cache\n",
        )
        .unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert!(
            inventory
                .files
                .iter()
                .any(|file| file.relative_path == "src/generated.machine")
        );
        assert!(
            !inventory
                .files
                .iter()
                .any(|file| file.relative_path.starts_with(".dx/serializer/")),
            "generated serializer caches should not be source-owned inventory"
        );
    }

    #[test]
    fn scan_project_ignores_approved_project_machine_cache_roots() {
        let temp = tempdir().unwrap();
        let approved_cache_paths: &[&[&str]] = &[
            &[".dx", "check", "check-report-latest.machine"],
            &[".dx", "cli", "status-latest.machine"],
            &[".dx", "www", "forge-package-status.machine"],
            &[".dx", "icon", "machine", "v1", "catalog.machine"],
            &[".dx", "media", "machine", "v1", "provider-catalog.machine"],
            &[".dx", "dcp", "cache", "schema-registry.machine"],
        ];

        for relative in approved_cache_paths {
            let path = relative
                .iter()
                .fold(temp.path().to_path_buf(), |path, segment| {
                    path.join(segment)
                });
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "generated cache\n").unwrap();
        }
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(temp.path().join("src").join("generated.machine"), "cache").unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert_eq!(inventory.files.len(), 1);
        assert_eq!(inventory.files[0].relative_path, "src/generated.machine");
    }

    #[test]
    fn scan_project_treats_serializer_cache_path_case_insensitively() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".DX").join("Serializer")).unwrap();
        fs::write(
            temp.path()
                .join(".DX")
                .join("Serializer")
                .join("check-launch-report.machine"),
            "generated cache\n",
        )
        .unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert!(
            inventory.files.is_empty(),
            "serializer cache casing should not make generated artifacts source-owned"
        );
    }

    #[test]
    fn scan_project_treats_node_modules_case_insensitively() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("Node_Modules").join("pkg")).unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(
            temp.path()
                .join("Node_Modules")
                .join("pkg")
                .join("native.cpp"),
            "int dependency(void) { return 0; }\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("src").join("main.cpp"),
            "int main() { return 0; }\n",
        )
        .unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert!(
            inventory.contains_node_modules,
            "case variants of node_modules must still be reported as dependency evidence"
        );
        assert!(
            inventory
                .files
                .iter()
                .any(|file| file.relative_path == "src/main.cpp")
        );
        assert!(
            !inventory
                .files
                .iter()
                .any(|file| file.relative_path.contains("Node_Modules")),
            "case variants of node_modules must not be inventoried as source-owned files"
        );
    }

    #[test]
    fn scan_project_ignores_nested_generated_serializer_machine_artifacts() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(
            temp.path()
                .join("examples")
                .join("template")
                .join(".dx")
                .join("serializer"),
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("examples").join("template").join("src")).unwrap();
        fs::write(
            temp.path()
                .join("examples")
                .join("template")
                .join(".dx")
                .join("serializer")
                .join("dx.machine"),
            "generated cache\n",
        )
        .unwrap();
        fs::write(
            temp.path()
                .join("examples")
                .join("template")
                .join("src")
                .join("generated.machine"),
            "cache",
        )
        .unwrap();

        let inventory = scan_project(temp.path()).unwrap();

        assert!(
            inventory
                .files
                .iter()
                .any(|file| { file.relative_path == "examples/template/src/generated.machine" })
        );
        assert!(
            !inventory
                .files
                .iter()
                .any(|file| file.relative_path.contains("/.dx/serializer/")),
            "nested serializer caches should not be source-owned inventory"
        );
    }
}
