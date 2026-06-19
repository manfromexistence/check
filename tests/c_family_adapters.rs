use std::fs;

use tempfile::tempdir;

use dx_check_engine::adapters::plan_tools;
use dx_check_engine::model::{DxToolPlan, DxToolTarget};

#[test]
fn c_family_plans_non_mutating_format_lint_build_and_test_tools() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("runtime.c"),
        "int runtime(void) { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();
    fs::write(
        root.path().join(".clang-tidy"),
        "Checks: bugprone-*,performance-*\n",
    )
    .unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        r#"[
  { "directory": ".", "command": "clang++ -c src/main.cpp", "file": "src/main.cpp" },
  { "directory": ".", "arguments": ["clang", "-c", "src/runtime.c"], "file": "src/runtime.c" }
]
"#,
    )
    .unwrap();
    fs::create_dir_all(root.path().join("build").join("debug")).unwrap();
    fs::write(
        root.path()
            .join("build")
            .join("debug")
            .join("CTestTestfile.cmake"),
        "# CTest generated file\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.25)\nproject(demo C CXX)\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "configurePresets": [{ "name": "debug", "generator": "Ninja", "binaryDir": "build/debug" }],
  "buildPresets": [{ "name": "debug", "configurePreset": "debug" }],
  "testPresets": [{ "name": "unit", "configurePreset": "debug" }]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(
        root.path(),
        &[
            DxToolTarget::Format,
            DxToolTarget::Lint,
            DxToolTarget::Typecheck,
            DxToolTarget::Test,
        ],
    );

    let format = plans
        .iter()
        .find(|plan| plan.id == "cpp-clang-format-check")
        .expect("clang-format plan");
    assert_eq!(format.executable, "clang-format");
    assert_eq!(format.parser, "clang-format");
    assert!(format.args.iter().any(|arg| arg == "--dry-run"));
    assert!(format.args.iter().any(|arg| arg == "--Werror"));
    assert!(format.args.iter().any(|arg| arg == "src/main.cpp"));
    assert!(format.args.iter().any(|arg| arg == "src/runtime.c"));

    let tidy = plans
        .iter()
        .find(|plan| plan.id == "cpp-clang-tidy")
        .expect("clang-tidy plan");
    assert_eq!(tidy.executable, "clang-tidy");
    assert_eq!(tidy.parser, "clang-tidy");
    assert!(tidy.args.windows(2).any(|pair| pair == ["-p", "."]));
    assert!(tidy.args.iter().any(|arg| arg == "--warnings-as-errors=*"));
    assert!(tidy.args.iter().any(|arg| arg == "src/main.cpp"));
    assert!(
        tidy.detected_from
            .contains(&"compile_commands.json".to_string())
    );
    assert!(tidy.detected_from.contains(&".clang-tidy".to_string()));

    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck plan");
    assert_eq!(cppcheck.executable, "cppcheck");
    assert_eq!(cppcheck.parser, "cppcheck-xml");
    assert!(cppcheck.args.iter().any(|arg| arg == "--xml"));
    assert!(
        cppcheck
            .args
            .iter()
            .any(|arg| arg == "--project=compile_commands.json")
    );

    let clangd = plans
        .iter()
        .find(|plan| plan.id == "cpp-clangd-check")
        .expect("clangd check plan");
    assert_eq!(clangd.executable, "clangd");
    assert_eq!(clangd.parser, "clangd");
    assert!(
        clangd
            .args
            .iter()
            .any(|arg| arg.starts_with("--check=src/main.cpp"))
    );
    assert!(
        clangd
            .args
            .iter()
            .any(|arg| arg == "--compile-commands-dir=.")
    );

    let cmake_build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    assert_eq!(cmake_build.executable, "cmake");
    assert_eq!(cmake_build.parser, "cxx-compiler");
    assert_eq!(cmake_build.target, DxToolTarget::Typecheck);
    assert_eq!(
        cmake_build.args,
        ["--build", "--preset", "debug", "--parallel", "1"]
    );
    assert!(
        cmake_build
            .detected_from
            .contains(&"CMakePresets.json".to_string())
    );

    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");
    assert_eq!(test.executable, "ctest");
    assert_eq!(test.parser, "ctest");
    assert_eq!(
        test.args,
        [
            "--preset",
            "unit",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
    assert!(
        test.detected_from
            .contains(&"CMakePresets.json".to_string())
    );
}

#[test]
fn c_family_test_falls_back_to_ctest_build_directory_without_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("build")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("build").join("CTestTestfile.cmake"),
        format!(
            "# CTest generated file\n# Source directory: {}\n",
            root.path().display()
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Test]);

    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest build directory fallback plan");
    assert_eq!(test.executable, "ctest");
    assert_eq!(test.parser, "ctest");
    assert_eq!(
        test.args,
        [
            "--test-dir",
            "build",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
    assert!(
        test.detected_from
            .contains(&"build/CTestTestfile.cmake".to_string())
    );
}

#[test]
fn c_family_typecheck_uses_configured_cmake_build_directory_without_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("build")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.25)\nproject(demo CXX)\n",
    )
    .unwrap();
    fs::write(
        root.path().join("build").join("CMakeCache.txt"),
        format!(
            "# configured CMake build directory\nCMAKE_HOME_DIRECTORY:INTERNAL={}\n",
            root.path().display()
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);

    let cmake_build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build directory plan");
    assert_eq!(cmake_build.executable, "cmake");
    assert_eq!(cmake_build.parser, "cxx-compiler");
    assert_eq!(cmake_build.args, ["--build", "build", "--parallel", "1"]);
    assert!(
        cmake_build
            .detected_from
            .contains(&"build/CMakeCache.txt".to_string())
    );
}

#[test]
fn c_family_lint_skips_generated_build_tree_sources() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("build").join("generated")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path()
            .join("build")
            .join("generated")
            .join("generated.cpp"),
        "int generated() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);
    let joined_args = plans
        .iter()
        .flat_map(|plan| plan.args.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(joined_args.contains("src/main.cpp"));
    assert!(!joined_args.contains("build/generated/generated.cpp"));
}

#[test]
fn c_family_file_discovery_skips_generated_dirs_case_insensitively() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    for relative in [
        "BUILD/generated.cpp",
        "TARGET/native.cpp",
        "Node_Modules/pkg/native.cpp",
        "DIST/app.cpp",
        ".CACHE/generated.cpp",
        "Coverage/report.cpp",
        "OUT/build/gen.cpp",
        "CMakeFiles/rule.cpp",
        "_DEPS/fmt-src/fmt.cpp",
        "Vcpkg_Installed/x64/foo.cpp",
        ".Conan/cache.cpp",
        "Third_Party/lib.cpp",
        "VENDOR/lib.cpp",
        "EXTERNAL/lib.cpp",
        "CMake-Build-Debug/generated.cpp",
        "BAZEL-out/gen.cpp",
    ] {
        write_c_family_source(root.path(), relative);
    }
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);
    let joined_args = joined_plan_args(&plans);

    assert!(joined_args.contains("src/main.cpp"));
    for excluded in [
        "BUILD/generated.cpp",
        "TARGET/native.cpp",
        "Node_Modules/pkg/native.cpp",
        "DIST/app.cpp",
        ".CACHE/generated.cpp",
        "Coverage/report.cpp",
        "OUT/build/gen.cpp",
        "CMakeFiles/rule.cpp",
        "_DEPS/fmt-src/fmt.cpp",
        "Vcpkg_Installed/x64/foo.cpp",
        ".Conan/cache.cpp",
        "Third_Party/lib.cpp",
        "VENDOR/lib.cpp",
        "EXTERNAL/lib.cpp",
        "CMake-Build-Debug/generated.cpp",
        "BAZEL-out/gen.cpp",
    ] {
        assert!(
            !joined_args.contains(excluded),
            "generated/dependency directory case variants must be excluded from C/C++ tool args: {excluded}\n{joined_args}"
        );
    }
}

#[test]
fn c_family_file_discovery_keeps_project_dirs_that_only_contain_reserved_words() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();
    for relative in [
        "src/build_tools/manual.cpp",
        "src/vendor_tools/manual.cpp",
        "src/external_api/manual.cpp",
        "src/third_party_notes/manual.cpp",
    ] {
        write_c_family_source(root.path(), relative);
    }

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);
    let joined_args = joined_plan_args(&plans);

    for included in [
        "src/build_tools/manual.cpp",
        "src/vendor_tools/manual.cpp",
        "src/external_api/manual.cpp",
        "src/third_party_notes/manual.cpp",
    ] {
        assert!(
            joined_args.contains(included),
            "project-owned directories must not be skipped by reserved-word substrings: {included}\n{joined_args}"
        );
    }
}

#[test]
fn c_family_file_discovery_skips_dx_cache_roots_case_insensitively() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    for relative in [
        ".DX/Serializer/generated.cpp",
        ".dx/check/cache/remote.cpp",
        ".DX/DCP/Cache/schema.cpp",
    ] {
        write_c_family_source(root.path(), relative);
    }
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);
    let joined_args = joined_plan_args(&plans);

    assert!(joined_args.contains("src/main.cpp"));
    for excluded in [
        ".DX/Serializer/generated.cpp",
        ".dx/check/cache/remote.cpp",
        ".DX/DCP/Cache/schema.cpp",
    ] {
        assert!(
            !joined_args.contains(excluded),
            "DX cache roots must be excluded from C/C++ tool args: {excluded}\n{joined_args}"
        );
    }
}

#[test]
fn c_family_adapter_ignores_generated_source_for_tool_args_and_compile_db_coverage() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("include")).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("schema.pb.cc"),
        "int generated_schema() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("include").join("schema.pb.h"),
        "#pragma once\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("manual.cpp"),
        "// @generated by a local codegen step\nint manual() { return 2; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        r#"[
  { "directory": ".", "command": "clang++ -c src/main.cpp", "file": "src/main.cpp" },
  { "directory": ".", "command": "clang++ -c src/schema.pb.cc", "file": "src/schema.pb.cc" },
  { "directory": ".", "command": "clang++ -c src/manual.cpp", "file": "src/manual.cpp" }
]
"#,
    )
    .unwrap();

    let plans = plan_tools(
        root.path(),
        &[
            DxToolTarget::Format,
            DxToolTarget::Lint,
            DxToolTarget::Typecheck,
        ],
    );
    let joined_args = plans
        .iter()
        .flat_map(|plan| plan.args.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(joined_args.contains("src/main.cpp"));
    for generated in ["src/schema.pb.cc", "include/schema.pb.h", "src/manual.cpp"] {
        assert!(
            !joined_args.contains(generated),
            "generated C/C++ source must not be passed to tool plans: {generated}\n{joined_args}"
        );
    }
    assert!(
        plans
            .iter()
            .any(|plan| plan.id == "cpp-clang-tidy"
                && plan.args.iter().any(|arg| arg == "src/main.cpp")),
        "clang-tidy should still run for covered hand-authored translation units"
    );
    assert!(
        plans
            .iter()
            .any(|plan| plan.id == "cpp-clangd-check" && plan.args[0].contains("src/main.cpp")),
        "clangd should still check covered hand-authored translation units"
    );
    assert!(
        plans
            .iter()
            .filter(|plan| plan.id == "cpp-cppcheck")
            .all(|plan| !plan.args.iter().any(|arg| arg.starts_with("--project="))),
        "cppcheck must not consume compile databases that include generated sources"
    );
}

#[test]
fn c_family_file_discovery_skips_linked_directories_that_escape_project_root() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        outside.path().join("escape.cpp"),
        "int escape() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

    if create_directory_link(
        outside.path(),
        &root.path().join("src").join("linked-outside"),
    )
    .is_err()
    {
        return;
    }

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);
    let joined_args = plans
        .iter()
        .flat_map(|plan| plan.args.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    assert!(joined_args.contains("src/main.cpp"));
    assert!(
        !joined_args.contains("linked-outside/escape.cpp"),
        "C/C++ adapters must not pass files discovered through linked directories"
    );
}

#[test]
fn c_family_blocks_empty_or_stale_compile_database_for_context_tools() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(root.path().join("compile_commands.json"), "[]\n").unwrap();

    let empty_database_plans =
        plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);
    assert_context_tools_blocked(&empty_database_plans, "compile_commands.json");
    let cppcheck = empty_database_plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck fallback");
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project="))
    );

    fs::write(
        root.path().join("compile_commands.json"),
        r#"[{ "directory": ".", "command": "clang++ -c stale.cpp", "file": "stale.cpp" }]"#,
    )
    .unwrap();

    let stale_database_plans =
        plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);
    assert_context_tools_blocked(&stale_database_plans, "compile_commands.json");
}

#[test]
fn c_family_blocks_compile_database_entries_with_malformed_arguments() {
    let cases = [
        (
            "null-argument",
            r#"[{ "directory": ".", "arguments": [null], "file": "src/main.cpp" }]"#,
        ),
        (
            "blank-argument",
            r#"[{ "directory": ".", "arguments": [" "], "file": "src/main.cpp" }]"#,
        ),
    ];

    for (name, database) in cases {
        let root = tempdir().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("src").join("main.cpp"),
            "int main() { return 0; }\n",
        )
        .unwrap();
        fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
        fs::write(root.path().join("compile_commands.json"), database).unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);

        assert_context_tools_blocked(&plans, "compile_commands.json");
        assert_source_cppcheck_fallback(&plans, name);
    }
}

#[test]
fn c_family_blocks_compile_database_outside_project_suffix_matches() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        r#"[{
  "directory": "C:/outside/project",
  "command": "clang++ -c C:/outside/project/src/main.cpp",
  "file": "C:/outside/project/src/main.cpp"
}]
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);

    assert_context_tools_blocked(&plans, "compile_commands.json");
    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck source fallback");
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project=")),
        "cppcheck must not trust an outside-project compile database"
    );
}

#[test]
fn c_family_accepts_superset_compile_database_for_project_translation_units() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_path = outside.path().to_string_lossy().replace('\\', "/");
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        format!(
            r#"[{{
  "directory": ".",
  "command": "clang++ -c src/main.cpp",
  "file": "src/main.cpp"
}}, {{
  "directory": ".",
  "command": "clang++ -c build/generated/generated.cpp",
  "file": "build/generated/generated.cpp"
}}, {{
  "directory": ".",
  "command": "clang++ -c vendor/dependency.cpp",
  "file": "vendor/dependency.cpp"
}}, {{
  "directory": "{}",
  "command": "clang++ -c outside.cpp",
  "file": "outside.cpp"
}}]
"#,
            outside_path
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);
    let tidy = plans
        .iter()
        .find(|plan| plan.id == "cpp-clang-tidy")
        .expect("clang-tidy should use covered project entries from a superset database");
    assert!(tidy.args.iter().any(|arg| arg == "src/main.cpp"));
    assert!(
        tidy.args
            .iter()
            .all(|arg| !arg.contains("generated") && !arg.contains("dependency"))
    );
    assert!(
        plans
            .iter()
            .any(|plan| plan.id == "cpp-clangd-check" && plan.args[0].contains("src/main.cpp")),
        "clangd should check covered project translation units"
    );
    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck fallback");
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project=")),
        "cppcheck must not consume a superset compile database directly"
    );
    assert!(cppcheck.args.iter().any(|arg| arg == "src/main.cpp"));
    assert!(
        cppcheck
            .args
            .iter()
            .all(|arg| !arg.contains("generated") && !arg.contains("dependency"))
    );
}

#[test]
fn c_family_option_looking_file_args_are_normalized() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("--help.cpp"), "int main() { return 0; }\n").unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        r#"[{ "directory": ".", "command": "clang++ -c -- --help.cpp", "file": "--help.cpp" }]"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Format, DxToolTarget::Lint]);

    for id in ["cpp-clang-format-check", "cpp-clang-tidy"] {
        let plan = plans
            .iter()
            .find(|plan| plan.id == id)
            .unwrap_or_else(|| panic!("{id} plan"));
        assert!(
            plan.args.iter().any(|arg| arg == "./--help.cpp"),
            "{id} should normalize option-looking file paths"
        );
        assert!(
            !plan.args.iter().any(|arg| arg == "--help.cpp"),
            "{id} must not pass option-looking source paths as bare arguments"
        );
    }

    let cppcheck_root = tempdir().unwrap();
    fs::write(
        cppcheck_root.path().join("--help.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    let cppcheck_plans = plan_tools(cppcheck_root.path(), &[DxToolTarget::Lint]);
    let cppcheck = cppcheck_plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck source fallback");
    assert!(
        cppcheck.args.iter().any(|arg| arg == "./--help.cpp"),
        "cppcheck should normalize option-looking file paths"
    );
    assert!(
        !cppcheck.args.iter().any(|arg| arg == "--help.cpp"),
        "cppcheck must not pass option-looking source paths as bare arguments"
    );
}

#[test]
fn c_family_blocks_partial_compile_database_coverage() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("covered.cpp"),
        "int covered() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("uncovered.cpp"),
        "int uncovered() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("compile_commands.json"),
        r#"[{
  "directory": ".",
  "command": "clang++ -c src/covered.cpp",
  "file": "src/covered.cpp"
}]
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint, DxToolTarget::Typecheck]);

    assert_context_tools_blocked(&plans, "compile_commands.json");
    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck source fallback");
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project=")),
        "cppcheck must not trust partial compile databases as full-project evidence"
    );
}

#[test]
fn c_family_lint_can_use_cppcheck_without_compile_database() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("driver.c"),
        "int driver(void) { return 0; }\n",
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Lint]);

    assert!(
        !plans.iter().any(|plan| plan.id == "cpp-clang-tidy"),
        "clang-tidy requires compile_commands.json so it has the same parse context as the compiler"
    );
    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .expect("cppcheck can run from source files alone");
    assert!(cppcheck.args.iter().any(|arg| arg == "src/driver.c"));
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project=")),
        "cppcheck should not claim a compile database that is missing"
    );
}

fn joined_plan_args(plans: &[dx_check_engine::model::DxToolPlan]) -> String {
    plans
        .iter()
        .flat_map(|plan| plan.args.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_context_tools_blocked(plans: &[DxToolPlan], source: &str) {
    for (id, target) in [
        ("cpp-clang-tidy", DxToolTarget::Lint),
        ("cpp-clangd-check", DxToolTarget::Typecheck),
    ] {
        let plan = plans
            .iter()
            .find(|plan| plan.id == id)
            .unwrap_or_else(|| panic!("{id} blocked plan"));
        assert_eq!(plan.target, target, "{id}");
        assert_eq!(plan.executable, "dx-check-blocked", "{id}");
        assert_eq!(plan.parser, "blocked", "{id}");
        assert!(
            plan.detected_from.iter().any(|detected| detected == source),
            "{id} must name blocked compile database source: {:?}",
            plan.detected_from
        );
        assert!(
            plan.args
                .iter()
                .any(|arg| arg.contains(source) && arg.contains("compile database")),
            "{id} must explain blocked compile database: {:?}",
            plan.args
        );
    }
}

fn assert_source_cppcheck_fallback(plans: &[DxToolPlan], name: &str) {
    let cppcheck = plans
        .iter()
        .find(|plan| plan.id == "cpp-cppcheck")
        .unwrap_or_else(|| panic!("{name} cppcheck fallback"));
    assert!(
        !cppcheck
            .args
            .iter()
            .any(|arg| arg.starts_with("--project=")),
        "{name} must keep cppcheck on source-file fallback"
    );
}

fn write_c_family_source(root: &std::path::Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "int generated_dependency(void) { return 0; }\n").unwrap();
}

#[cfg(unix)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
