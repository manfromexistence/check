use std::path::Path;
use std::{fs, io::Read};

pub(super) fn rust_file_has_application_unwrap(relative_path: &str, path: &Path) -> bool {
    if is_test_relative_path(relative_path) {
        return false;
    }

    fs::read_to_string(path)
        .map(|content| rust_application_code_has_unwrap(&content))
        .unwrap_or(false)
}

pub(super) fn source_contains_insecure_default(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|content| source_code_has_insecure_default(&content))
        .unwrap_or(false)
}

pub(crate) fn generated_source_leak(relative_path: &str, path: &Path) -> bool {
    generated_source_filename(relative_path)
        || (generated_marker_scan_allowed(relative_path) && source_contains_generated_marker(path))
}

fn is_test_relative_path(relative_path: &str) -> bool {
    relative_path.starts_with("tests/")
        || relative_path.contains("/tests/")
        || relative_path.ends_with("/tests.rs")
        || relative_path.ends_with("_test.rs")
        || relative_path.ends_with("_tests.rs")
}

fn rust_application_code_has_unwrap(content: &str) -> bool {
    let application_code = rust_without_cfg_test_modules(content);
    rust_code_has_unwrap_call(&application_code)
}

fn rust_without_cfg_test_modules(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut pending_cfg_test = false;
    let mut skip_depth = None::<i32>;

    for line in content.lines() {
        if let Some(depth) = skip_depth.as_mut() {
            *depth += rust_brace_delta(line);
            if *depth <= 0 {
                skip_depth = None;
            }
            continue;
        }

        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test && trimmed.starts_with("mod ") && trimmed.contains('{') {
            let depth = rust_brace_delta(line);
            if depth > 0 {
                skip_depth = Some(depth);
            }
            pending_cfg_test = false;
            continue;
        }

        if pending_cfg_test && !(trimmed.is_empty() || trimmed.starts_with("#[")) {
            pending_cfg_test = false;
        }

        output.push_str(line);
        output.push('\n');
    }

    output
}

fn rust_brace_delta(line: &str) -> i32 {
    line.chars().fold(0, |depth, character| match character {
        '{' => depth + 1,
        '}' => depth - 1,
        _ => depth,
    })
}

fn rust_code_has_unwrap_call(content: &str) -> bool {
    let mut index = 0;
    let bytes = content.as_bytes();
    let mut state = RustScanState::Code;

    while index < bytes.len() {
        match state {
            RustScanState::Code => {
                let rest = &bytes[index..];
                if rest.starts_with(b".unwrap()") || rest.starts_with(b".expect(") {
                    return true;
                }
                if rest.starts_with(b"//") {
                    state = RustScanState::LineComment;
                    index += 2;
                    continue;
                }
                if rest.starts_with(b"/*") {
                    state = RustScanState::BlockComment;
                    index += 2;
                    continue;
                }
                if bytes[index] == b'"' {
                    state = RustScanState::String;
                }
            }
            RustScanState::String => match bytes[index] {
                b'\\' => index += 1,
                b'"' => state = RustScanState::Code,
                _ => {}
            },
            RustScanState::LineComment => {
                if bytes[index] == b'\n' {
                    state = RustScanState::Code;
                }
            }
            RustScanState::BlockComment => {
                if bytes[index..].starts_with(b"*/") {
                    state = RustScanState::Code;
                    index += 1;
                }
            }
        }

        index += 1;
    }

    false
}

#[derive(Debug, Clone, Copy)]
enum RustScanState {
    Code,
    String,
    LineComment,
    BlockComment,
}

fn source_code_has_insecure_default(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut index = 0;
    let mut state = SourceScanState::Code;

    while index < bytes.len() {
        match state {
            SourceScanState::Code => {
                let rest = &bytes[index..];
                if rest.starts_with(b"dangerouslysetinnerhtml") {
                    return true;
                }
                if insecure_assignment_starts_at(bytes, index, b"password")
                    || insecure_assignment_starts_at(bytes, index, b"secret")
                    || insecure_assignment_starts_at(bytes, index, b"api_key")
                {
                    return true;
                }
                if rest.starts_with(b"//") {
                    state = SourceScanState::LineComment;
                    index += 2;
                    continue;
                }
                if rest.starts_with(b"/*") {
                    state = SourceScanState::BlockComment;
                    index += 2;
                    continue;
                }
                match bytes[index] {
                    b'"' => state = SourceScanState::String(b'"'),
                    b'\'' => state = SourceScanState::String(b'\''),
                    _ => {}
                }
            }
            SourceScanState::String(quote) => match bytes[index] {
                b'\\' => index += 1,
                value if value == quote => state = SourceScanState::Code,
                _ => {}
            },
            SourceScanState::LineComment => {
                if bytes[index] == b'\n' {
                    state = SourceScanState::Code;
                }
            }
            SourceScanState::BlockComment => {
                if bytes[index..].starts_with(b"*/") {
                    state = SourceScanState::Code;
                    index += 1;
                }
            }
        }

        index += 1;
    }

    false
}

fn insecure_assignment_starts_at(bytes: &[u8], index: usize, name: &[u8]) -> bool {
    if !bytes[index..].starts_with(name) {
        return false;
    }
    if index > 0 && is_identifier_byte(bytes[index - 1]) {
        return false;
    }
    let mut cursor = index + name.len();
    if cursor < bytes.len() && is_identifier_byte(bytes[cursor]) {
        return false;
    }

    cursor = skip_ascii_whitespace(bytes, cursor);
    if cursor >= bytes.len() || bytes[cursor] != b'=' {
        return false;
    }
    cursor = skip_ascii_whitespace(bytes, cursor + 1);
    cursor < bytes.len() && matches!(bytes[cursor], b'"' | b'\'')
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[derive(Debug, Clone, Copy)]
enum SourceScanState {
    Code,
    String(u8),
    LineComment,
    BlockComment,
}

fn generated_source_filename(relative_path: &str) -> bool {
    let lower = relative_path.to_ascii_lowercase();
    GENERATED_SOURCE_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

const GENERATED_SOURCE_SUFFIXES: [&str; 39] = [
    ".generated.ts",
    ".generated.tsx",
    ".generated.js",
    ".generated.jsx",
    ".generated.rs",
    ".generated.c",
    ".generated.cc",
    ".generated.cpp",
    ".generated.cxx",
    ".generated.h",
    ".generated.hh",
    ".generated.hpp",
    ".generated.hxx",
    ".gen.ts",
    ".gen.tsx",
    ".gen.js",
    ".gen.jsx",
    ".gen.rs",
    ".gen.c",
    ".gen.cc",
    ".gen.cpp",
    ".gen.cxx",
    ".gen.h",
    ".gen.hh",
    ".gen.hpp",
    ".gen.hxx",
    ".pb.go",
    ".pb.ts",
    ".pb.js",
    ".pb.c",
    ".pb.cc",
    ".pb.cpp",
    ".pb.cxx",
    ".pb.cu",
    ".pb.cuh",
    ".pb.h",
    ".pb.hh",
    ".pb.hpp",
    ".pb.hxx",
];

fn generated_marker_scan_allowed(relative_path: &str) -> bool {
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "c"
            | "cc"
            | "cpp"
            | "cxx"
            | "h"
            | "hh"
            | "hpp"
            | "hxx"
    )
}

fn source_contains_generated_marker(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut reader = file.take(GENERATED_MARKER_SCAN_LIMIT);
    let mut bytes = Vec::new();
    if reader.read_to_end(&mut bytes).is_err() {
        return false;
    }
    let lower = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    lower.lines().take(20).any(header_has_generated_marker)
}

fn header_has_generated_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !is_comment_like_header_line(trimmed) {
        return false;
    }

    trimmed.contains("@generated")
        || trimmed.contains("do not edit")
        || trimmed.contains("code generated")
}

fn is_comment_like_header_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with("#")
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.starts_with("<!--")
}

const GENERATED_MARKER_SCAN_LIMIT: u64 = 64 * 1024;

#[cfg(test)]
mod tests {
    use super::{
        is_test_relative_path, rust_code_has_unwrap_call, source_code_has_insecure_default,
    };

    #[test]
    fn rust_test_module_paths_are_not_treated_as_application_source() {
        assert!(is_test_relative_path("tests/integration.rs"));
        assert!(is_test_relative_path("src/output/tests.rs"));
        assert!(is_test_relative_path("src/parser/snapshot_test.rs"));
        assert!(is_test_relative_path("src/parser/snapshot_tests.rs"));
        assert!(!is_test_relative_path("src/parser/test_support.rs"));
        assert!(!is_test_relative_path("src/parser/contest.rs"));
    }

    #[test]
    fn rust_unwrap_scan_handles_leading_bom_without_panicking() {
        assert!(rust_code_has_unwrap_call(
            "\u{feff}pub fn load(value: Option<u8>) -> u8 { value.unwrap() }\n"
        ));
    }

    #[test]
    fn insecure_default_scan_handles_leading_bom_without_panicking() {
        assert!(source_code_has_insecure_default(
            "\u{feff}export const password = \"demo\";\n"
        ));
    }
}
