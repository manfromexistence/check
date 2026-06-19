use std::path::PathBuf;

use dx_check_engine::diagnostics::parse_tool_output;
use dx_check_engine::model::{DxSeverity, DxToolPlan, DxToolTarget};

fn plan(id: &str, parser: &str, target: DxToolTarget) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "tool".to_string(),
        args: Vec::new(),
        cwd: PathBuf::from("G:\\demo"),
        detected_from: vec!["fixture".to_string()],
        parser: parser.to_string(),
    }
}

#[test]
fn parses_clang_and_gcc_style_c_family_diagnostics_with_windows_paths() {
    let plan = plan("cpp-clang-tidy", "clang-tidy", DxToolTarget::Lint);
    let stderr = r#"C:\work\demo\src\main.cpp:12:5: error: no matching function for call to 'run' [clang-diagnostic-error]
src/runtime.c:7:3: warning: unused variable 'count' [-Wunused-variable]
"#;

    let diagnostics = parse_tool_output(&plan, b"", stderr.as_bytes());

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "cpp-clang-tidy:clang-diagnostic-error");
    assert_eq!(diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(
        diagnostics[0].file.as_deref(),
        Some(r#"C:\work\demo\src\main.cpp"#)
    );
    assert_eq!(diagnostics[0].line, Some(12));
    assert_eq!(diagnostics[0].column, Some(5));
    assert!(diagnostics[0].message.contains("no matching function"));

    assert_eq!(diagnostics[1].id, "cpp-clang-tidy:-Wunused-variable");
    assert_eq!(diagnostics[1].severity, DxSeverity::Warning);
    assert_eq!(diagnostics[1].file.as_deref(), Some("src/runtime.c"));
}

#[test]
fn parses_msvc_style_c_family_diagnostics() {
    let plan = plan("cpp-cmake-build", "cpp-compiler", DxToolTarget::Typecheck);
    let stderr = r#"src\main.cpp(22,17): warning C4244: conversion from 'double' to 'int', possible loss of data
C:\work\demo\src\parser.c(31,9): error C2143: syntax error: missing ';' before '}'
"#;

    let diagnostics = parse_tool_output(&plan, b"", stderr.as_bytes());

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "cpp-cmake-build:C4244");
    assert_eq!(diagnostics[0].severity, DxSeverity::Warning);
    assert_eq!(diagnostics[0].file.as_deref(), Some(r#"src\main.cpp"#));
    assert_eq!(diagnostics[0].line, Some(22));
    assert_eq!(diagnostics[0].column, Some(17));
    assert!(diagnostics[0].message.contains("conversion"));

    assert_eq!(diagnostics[1].id, "cpp-cmake-build:C2143");
    assert_eq!(diagnostics[1].severity, DxSeverity::Failure);
    assert_eq!(
        diagnostics[1].file.as_deref(),
        Some(r#"C:\work\demo\src\parser.c"#)
    );
}

#[test]
fn parses_clang_format_dry_run_violations() {
    let plan = plan("cpp-clang-format", "clang-format", DxToolTarget::Format);
    let stderr =
        b"src/main.cpp:1:1: error: code should be clang-formatted [-Wclang-format-violations]\n";

    let diagnostics = parse_tool_output(&plan, b"", stderr);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].id,
        "cpp-clang-format:-Wclang-format-violations"
    );
    assert_eq!(diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert!(diagnostics[0].next_action.contains("clang-format"));
}

#[test]
fn parses_clangd_check_diagnostics() {
    let plan = plan("cpp-clangd-check", "clangd", DxToolTarget::Typecheck);
    let stderr = r#"I[09:22:01.000] Testing on source file G:\demo\src\main.cpp
E[09:22:01.010] [undeclared_var_use] src/main.cpp:7:13: use of undeclared identifier 'value'
W[09:22:01.020] [unused-includes] src/main.cpp:2:1: included header vector is not used directly
"#;

    let diagnostics = parse_tool_output(&plan, b"", stderr.as_bytes());

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "cpp-clangd-check:undeclared_var_use");
    assert_eq!(diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert_eq!(diagnostics[0].line, Some(7));
    assert_eq!(diagnostics[0].column, Some(13));
    assert!(diagnostics[0].message.contains("undeclared identifier"));
    assert_eq!(diagnostics[1].id, "cpp-clangd-check:unused-includes");
    assert_eq!(diagnostics[1].severity, DxSeverity::Warning);
}

#[test]
fn parses_cppcheck_xml_diagnostics() {
    let plan = plan("cpp-cppcheck", "cppcheck-xml", DxToolTarget::Lint);
    let stderr = br#"<?xml version="1.0" encoding="UTF-8"?>
<results version="2">
  <errors>
    <error id="uninitvar" severity="error" msg="Uninitialized variable: value">
      <location file="src/main.cpp" line="9" column="13"/>
    </error>
    <error id="unusedFunction" severity="style" msg="The function helper is never used">
      <location file="src/helper.c" line="4"/>
    </error>
  </errors>
</results>
"#;

    let diagnostics = parse_tool_output(&plan, b"", stderr);

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "cpp-cppcheck:uninitvar");
    assert_eq!(diagnostics[0].severity, DxSeverity::Failure);
    assert_eq!(diagnostics[0].file.as_deref(), Some("src/main.cpp"));
    assert_eq!(diagnostics[0].line, Some(9));
    assert_eq!(diagnostics[0].column, Some(13));

    assert_eq!(diagnostics[1].id, "cpp-cppcheck:unusedFunction");
    assert_eq!(diagnostics[1].severity, DxSeverity::Warning);
    assert_eq!(diagnostics[1].file.as_deref(), Some("src/helper.c"));
}

#[test]
fn parses_ctest_failed_test_names() {
    let plan = plan("cpp-ctest", "ctest", DxToolTarget::Test);
    let stdout = b"The following tests FAILED:\n\t  1 - parser_round_trip (Failed)\n\t  2 - runtime_contract (Timeout)\nErrors while running CTest\n";

    let diagnostics = parse_tool_output(&plan, stdout, b"");

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "cpp-ctest:test-failed");
    assert_eq!(diagnostics[0].severity, DxSeverity::Failure);
    assert!(diagnostics[0].message.contains("parser_round_trip"));
    assert!(diagnostics[1].message.contains("runtime_contract"));
}
