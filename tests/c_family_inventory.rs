use std::fs;

use tempfile::tempdir;

use dx_check_engine::model::DxToolTarget;
use dx_check_engine::{DxCheckEngineOptions, analyze_project};

#[test]
fn analyze_project_reports_c_family_sources_tests_and_adapter_plans() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("include")).unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::create_dir_all(root.path().join("tests")).unwrap();
    fs::write(
        root.path().join("include").join("library.hpp"),
        "#pragma once\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("library.cpp"),
        "int answer() { return 42; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("runtime.c"),
        "int runtime(void) { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tests").join("library_test.cpp"),
        "TEST(Library, Answers) { EXPECT_EQ(answer(), 42); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("tests").join("runtime_test.c"),
        "TEST_CASE(\"runtime starts\") { CHECK(runtime() == 0); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.25)\nproject(demo C CXX)\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            run_targets: vec![DxToolTarget::Format, DxToolTarget::Lint],
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert!(
        report
            .checked_paths
            .iter()
            .any(|path| path == "include/library.hpp")
    );
    assert!(
        report
            .checked_paths
            .iter()
            .any(|path| path == "src/library.cpp")
    );
    assert!(
        report
            .checked_paths
            .iter()
            .any(|path| path == "src/runtime.c")
    );
    assert!(
        report
            .checked_paths
            .iter()
            .any(|path| path == "CMakeLists.txt")
    );
    assert_eq!(report.test_inventory.c_tests, 1);
    assert_eq!(report.test_inventory.cpp_tests, 1);
    assert!(
        report
            .adapter_plans
            .iter()
            .any(|plan| plan.id == "cpp-clang-format-check")
    );
    assert!(
        report
            .adapter_plans
            .iter()
            .any(|plan| plan.id == "cpp-cppcheck")
    );
    assert_eq!(report.score.max_score, 500);
}

#[test]
fn c_family_test_inventory_skips_generated_dirs_and_dx_cache_roots_case_insensitively() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("tests")).unwrap();
    fs::write(
        root.path().join("tests").join("real_test.cpp"),
        "TEST(ProjectOwned, Runs) {}\n",
    )
    .unwrap();
    for relative in [
        "BUILD/generated_test.cpp",
        "Node_Modules/pkg/native_test.cpp",
        ".DX/check/cache/cache_test.cpp",
    ] {
        write_cpp_test(root.path(), relative);
    }

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert_eq!(
        report.test_inventory.cpp_tests, 1,
        "generated, dependency, and DX cache C++ tests must not satisfy project-owned readiness"
    );
}

#[test]
fn analyze_project_skips_case_variant_generated_dependency_dirs_for_c_family_checked_paths() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("manual.cpp"),
        "int manual(void) { return 0; }\n",
    )
    .unwrap();
    for relative in [
        "BUILD/generated.cpp",
        "CMake-Build-Debug/generated.cpp",
        "BAZEL-out/generated.cpp",
        "_DEPS/fmt-src/fmt.cpp",
        "Vcpkg_Installed/x64/foo.cpp",
        "Node_Modules/pkg/native.cpp",
        "Third_Party/lib.cpp",
        ".DX/Check/Cache/cache.cpp",
        "examples/template/.DX/Check/Cache/cache.cpp",
    ] {
        write_cpp_source(root.path(), relative);
    }

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        report
            .checked_paths
            .iter()
            .any(|path| path == "src/manual.cpp"),
        "project-owned C++ source should remain checked"
    );
    for excluded in [
        "BUILD/generated.cpp",
        "CMake-Build-Debug/generated.cpp",
        "BAZEL-out/generated.cpp",
        "_DEPS/fmt-src/fmt.cpp",
        "Vcpkg_Installed/x64/foo.cpp",
        "Node_Modules/pkg/native.cpp",
        "Third_Party/lib.cpp",
        ".DX/Check/Cache/cache.cpp",
        "examples/template/.DX/Check/Cache/cache.cpp",
    ] {
        assert!(
            !report.checked_paths.iter().any(|path| path == excluded),
            "case-variant generated/dependency C++ files must not enter checked_paths: {excluded:?}"
        );
    }
}

#[test]
fn generated_c_family_sources_are_scored_as_generated_source() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("schema.pb.cc"),
        "// generated protobuf\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("schema.pb.h"),
        "// generated protobuf\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("schema.pb.cxx"),
        "int generated_schema_translation_unit() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("schema.pb.hxx"),
        "#pragma once\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src").join("manual.cpp"),
        "// @generated by a local codegen step\nint manual() { return 0; }\n",
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();
    let generated_files = report
        .findings
        .iter()
        .filter(|finding| finding.id == "generated-source-leak")
        .filter_map(|finding| finding.file.as_deref())
        .collect::<Vec<_>>();

    assert!(generated_files.contains(&"src/schema.pb.cc"));
    assert!(generated_files.contains(&"src/schema.pb.h"));
    assert!(generated_files.contains(&"src/schema.pb.cxx"));
    assert!(generated_files.contains(&"src/schema.pb.hxx"));
    assert!(generated_files.contains(&"src/manual.cpp"));
}

#[test]
fn analyze_project_includes_modern_cpp_file_shapes() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    for file in [
        "math.cppm",
        "graphics.ixx",
        "detail.ipp",
        "template.tpp",
        "kernel.cu",
        "kernel.cuh",
        "bridge.mm",
    ] {
        fs::write(
            root.path().join("src").join(file),
            "// modern C++ file shape\n",
        )
        .unwrap();
    }

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    for file in [
        "src/math.cppm",
        "src/graphics.ixx",
        "src/detail.ipp",
        "src/template.tpp",
        "src/kernel.cu",
        "src/kernel.cuh",
        "src/bridge.mm",
    ] {
        assert!(
            report.checked_paths.iter().any(|path| path == file),
            "expected {file} in checked paths"
        );
    }
}

fn write_cpp_test(root: &std::path::Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "TEST(GeneratedOrVendored, ShouldNotCount) {}\n").unwrap();
}

fn write_cpp_source(root: &std::path::Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "int generated_or_dependency(void) { return 0; }\n").unwrap();
}
