use std::fs;
use std::path::Path;

pub(super) fn is_component_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    (lower.ends_with(".tsx") || lower.ends_with(".jsx"))
        && (lower.contains("component")
            || lower.contains("/components/")
            || file_stem_is_pascal_case(path))
}

pub(super) fn component_has_boundary_leak(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    SERVER_ONLY_MODULES
        .iter()
        .any(|module| source_imports_module(&content, module))
}

pub(super) fn component_lacks_quality_affordance(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    !(content.contains("className")
        || content.contains("interface ")
        || content.contains("React.ComponentProps")
        || content_contains_props_type(&content))
}

fn file_stem_is_pascal_case(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next() else {
        return false;
    };
    let Some((stem, _extension)) = file_name.rsplit_once('.') else {
        return false;
    };
    let mut chars = stem.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        && stem.chars().any(|character| character.is_ascii_lowercase())
}

const SERVER_ONLY_MODULES: [&str; 8] = [
    "fs",
    "path",
    "child_process",
    "net",
    "tls",
    "http",
    "https",
    "process",
];

fn source_imports_module(content: &str, module: &str) -> bool {
    [
        format!("from '{module}'"),
        format!("from \"{module}\""),
        format!("from 'node:{module}'"),
        format!("from \"node:{module}\""),
        format!("require('{module}')"),
        format!("require(\"{module}\")"),
        format!("require('node:{module}')"),
        format!("require(\"node:{module}\")"),
    ]
    .iter()
    .any(|pattern| content.contains(pattern))
}

fn content_contains_props_type(content: &str) -> bool {
    content
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| token.ends_with("Props") && token.len() > "Props".len())
}
