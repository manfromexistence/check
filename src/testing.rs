use std::fs;
use std::path::Path;

use crate::languages::{is_c_source, is_cpp_source};
use crate::model::DxTestInventory;
use crate::path_filters::should_skip_generated_or_dependency_dir;
use crate::rules::source_scan::generated_source_leak;

pub fn discover_tests(root: &Path) -> DxTestInventory {
    let mut inventory = DxTestInventory::default();
    visit(root, &mut |path| {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if generated_source_leak(&relative, path) {
            return;
        }
        let content = fs::read_to_string(path).unwrap_or_default();

        match path.extension().and_then(|extension| extension.to_str()) {
            Some("rs") if content.contains("#[test]") => {
                inventory.rust_tests += content.matches("#[test]").count();
            }
            Some("ts" | "tsx" | "js" | "jsx")
                if relative.contains(".test.") || relative.contains(".spec.") =>
            {
                inventory.js_tests += content.matches("test(").count().max(1);
            }
            Some("py") if relative.starts_with("test_") || content.contains("def test_") => {
                inventory.python_tests += content.matches("def test_").count().max(1);
            }
            Some("go") if relative.ends_with("_test.go") => {
                inventory.go_tests += content.matches("func Test").count().max(1);
            }
            Some(_) if is_c_source(path) && c_family_test_file(&relative, &content) => {
                inventory.c_tests += c_family_test_count(&content);
            }
            Some(_) if is_cpp_source(path) && c_family_test_file(&relative, &content) => {
                inventory.cpp_tests += c_family_test_count(&content);
            }
            _ => {}
        }
    });

    inventory
}

fn c_family_test_file(relative: &str, content: &str) -> bool {
    let lower = relative.to_ascii_lowercase();
    lower.contains("_test.")
        || lower.contains(".test.")
        || lower.contains("_spec.")
        || lower.contains(".spec.")
        || content.contains("TEST(")
        || content.contains("TEST_F(")
        || content.contains("TEST_P(")
        || content.contains("TYPED_TEST(")
        || content.contains("TEST_CASE(")
        || content.contains("SCENARIO(")
}

fn c_family_test_count(content: &str) -> usize {
    [
        "TEST(",
        "TEST_F(",
        "TEST_P(",
        "TYPED_TEST(",
        "TEST_CASE(",
        "SCENARIO(",
    ]
    .iter()
    .map(|marker| content.matches(marker).count())
    .sum::<usize>()
    .max(1)
}

fn visit(dir: &Path, callback: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if should_skip_test_dir(&name) {
                continue;
            }
            visit(&path, callback);
        } else if path.is_file() {
            callback(&path);
        }
    }
}

fn should_skip_test_dir(name: &str) -> bool {
    should_skip_generated_or_dependency_dir(name)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::testing::discover_tests;

    #[test]
    fn discovers_common_test_shapes_without_running_them() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::write(
            temp.path().join("src").join("lib.rs"),
            "#[test]\nfn works() {}\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("widget.test.ts"),
            "test('works', () => {})",
        )
        .unwrap();
        fs::write(
            temp.path().join("test_demo.py"),
            "def test_demo():\n    pass\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("thing_test.go"),
            "func TestThing(t *testing.T) {}\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("runtime_test.c"),
            "TEST(Runtime, Starts) {}\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("parser_test.cpp"),
            "TEST_CASE(\"parser round trip\") {}\n",
        )
        .unwrap();

        let inventory = discover_tests(temp.path());

        assert_eq!(inventory.rust_tests, 1);
        assert_eq!(inventory.js_tests, 1);
        assert_eq!(inventory.python_tests, 1);
        assert_eq!(inventory.go_tests, 1);
        assert_eq!(inventory.c_tests, 1);
        assert_eq!(inventory.cpp_tests, 1);
    }
}
