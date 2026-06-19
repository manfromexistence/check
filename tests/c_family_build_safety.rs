use std::fs;

use dx_check_engine::adapters::plan_tools;
use dx_check_engine::model::{DxToolPlan, DxToolTarget};
use tempfile::tempdir;

#[test]
fn c_family_build_discovery_skips_linked_directories_that_escape_project_root() {
    let root = tempdir().unwrap();
    let outside_build = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src").join("main.cpp"),
        "int main() { return 0; }\n",
    )
    .unwrap();
    fs::write(root.path().join(".clang-tidy"), "Checks: bugprone-*\n").unwrap();
    fs::write(
        root.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.25)\nproject(demo CXX)\n",
    )
    .unwrap();
    fs::write(
        outside_build.path().join("compile_commands.json"),
        r#"[{ "directory": ".", "command": "clang++ -c src/main.cpp", "file": "src/main.cpp" }]"#,
    )
    .unwrap();
    fs::write(
        outside_build.path().join("CMakeCache.txt"),
        "# outside CMake cache\n",
    )
    .unwrap();
    fs::write(
        outside_build.path().join("CTestTestfile.cmake"),
        "# outside CTest generated file\n",
    )
    .unwrap();

    if create_directory_link(outside_build.path(), &root.path().join("build")).is_err() {
        return;
    }

    let plans = plan_tools(
        root.path(),
        &[
            DxToolTarget::Lint,
            DxToolTarget::Typecheck,
            DxToolTarget::Test,
        ],
    );

    assert!(
        !plans.iter().any(|plan| plan.id == "cpp-clang-tidy"),
        "linked outside build directory must not enable clang-tidy"
    );
    assert!(
        !plans.iter().any(|plan| plan.id == "cpp-clangd-check"),
        "linked outside build directory must not enable clangd checks"
    );
    assert!(
        !plans.iter().any(|plan| plan.id == "cpp-cmake-build"),
        "linked outside build directory must not enable cmake build"
    );
    assert!(
        !plans.iter().any(|plan| plan.id == "cpp-ctest"),
        "linked outside build directory must not enable ctest"
    );
}

#[test]
fn c_family_cmake_presets_skip_hidden_build_and_test_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "configurePresets": [
    { "name": "base", "hidden": true },
    { "name": "debug", "generator": "Ninja", "binaryDir": "build/debug" }
  ],
  "buildPresets": [
    { "name": "base-build", "configurePreset": "base", "hidden": true },
    { "name": "debug-build", "configurePreset": "debug" }
  ],
  "testPresets": [
    { "name": "base-test", "configurePreset": "base", "hidden": true },
    { "name": "debug-test", "configurePreset": "debug" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "debug-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "debug-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_skip_false_condition_build_and_test_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "disabled-build", "condition": false },
    { "name": "disabled-const-build", "condition": { "type": "const", "value": false } },
    { "name": "debug-build" }
  ],
  "testPresets": [
    { "name": "disabled-test", "condition": false },
    { "name": "disabled-const-test", "condition": { "type": "const", "value": false } },
    { "name": "debug-test" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "debug-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "debug-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_skip_false_expression_condition_build_and_test_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "disabled-equals-build", "condition": { "type": "equals", "lhs": "Debug", "rhs": "Release" } },
    { "name": "disabled-not-equals-build", "condition": { "type": "notEquals", "lhs": "Debug", "rhs": "Debug" } },
    { "name": "disabled-in-list-build", "condition": { "type": "inList", "string": "Debug", "list": ["Release", "RelWithDebInfo"] } },
    { "name": "disabled-not-in-list-build", "condition": { "type": "notInList", "string": "Debug", "list": ["Debug", "Release"] } },
    { "name": "disabled-any-build", "condition": { "type": "anyOf", "conditions": [
      { "type": "const", "value": false },
      { "type": "equals", "lhs": "Debug", "rhs": "Release" }
    ] } },
    { "name": "disabled-all-build", "condition": { "type": "allOf", "conditions": [
      { "type": "const", "value": true },
      { "type": "equals", "lhs": "Debug", "rhs": "Release" }
    ] } },
    { "name": "disabled-not-build", "condition": { "type": "not", "condition": { "type": "const", "value": true } } },
    { "name": "debug-build" }
  ],
  "testPresets": [
    { "name": "disabled-equals-test", "condition": { "type": "equals", "lhs": "Debug", "rhs": "Release" } },
    { "name": "disabled-not-equals-test", "condition": { "type": "notEquals", "lhs": "Debug", "rhs": "Debug" } },
    { "name": "disabled-in-list-test", "condition": { "type": "inList", "string": "Debug", "list": ["Release", "RelWithDebInfo"] } },
    { "name": "disabled-not-in-list-test", "condition": { "type": "notInList", "string": "Debug", "list": ["Debug", "Release"] } },
    { "name": "disabled-any-test", "condition": { "type": "anyOf", "conditions": [
      { "type": "const", "value": false },
      { "type": "equals", "lhs": "Debug", "rhs": "Release" }
    ] } },
    { "name": "disabled-all-test", "condition": { "type": "allOf", "conditions": [
      { "type": "const", "value": true },
      { "type": "equals", "lhs": "Debug", "rhs": "Release" }
    ] } },
    { "name": "disabled-not-test", "condition": { "type": "not", "condition": { "type": "const", "value": true } } },
    { "name": "debug-test" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "debug-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "debug-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_evaluate_host_system_name_condition_macros() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        format!(
            r#"{{
  "version": 6,
  "buildPresets": [
    {{ "name": "host-build", "condition": {{ "type": "equals", "lhs": "${{hostSystemName}}", "rhs": "{host_system}" }} }},
    {{ "name": "fallback-build" }}
  ],
  "testPresets": [
    {{ "name": "host-test", "condition": {{ "type": "equals", "lhs": "${{hostSystemName}}", "rhs": "{host_system}" }} }},
    {{ "name": "fallback-test" }}
  ]
}}
"#,
            host_system = cmake_host_system_name_for_test()
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "host-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "host-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_evaluate_regex_condition_objects() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "regex-build", "condition": { "type": "matches", "string": "Debug", "regex": "^Deb.*$" } },
    { "name": "fallback-build" }
  ],
  "testPresets": [
    { "name": "regex-test", "condition": { "type": "notMatches", "string": "Debug", "regex": "Release" } },
    { "name": "fallback-test" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "regex-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "regex-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_reject_malformed_condition_objects_without_fallback() {
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
        "# cache\n",
    )
    .unwrap();
    fs::write(
        root.path().join("build").join("CTestTestfile.cmake"),
        "# ctest\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "malformed-build", "condition": { "type": "equals", "lhs": "Debug" } }
  ],
  "testPresets": [
    { "name": "malformed-test", "condition": { "type": "anyOf", "conditions": [null] } }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);

    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);
    let test = blocked_plan(&plans, "cpp-ctest", DxToolTarget::Test);

    assert_blocked_detected_from(build, "CMakePresets.json");
    assert_blocked_detected_from(test, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "buildPresets");
    assert_blocked_reason_mentions(test, "CMakePresets.json");
    assert_blocked_reason_mentions(test, "testPresets");
}

#[test]
fn c_family_blocks_stale_cmake_cache_build_directory_without_presets() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
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
            "# stale CMake cache\nCMAKE_HOME_DIRECTORY:INTERNAL={}\n",
            outside.path().display()
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);

    assert_blocked_detected_from(build, "build/CMakeCache.txt");
    assert_blocked_reason_mentions(build, "CMake cache");
    assert_blocked_reason_mentions(build, "source directory");
}

#[test]
fn c_family_blocks_stale_ctest_build_directory_without_presets() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
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
            outside.path().display()
        ),
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Test]);
    let test = blocked_plan(&plans, "cpp-ctest", DxToolTarget::Test);

    assert_blocked_detected_from(test, "build/CTestTestfile.cmake");
    assert_blocked_reason_mentions(test, "CTest");
    assert_blocked_reason_mentions(test, "source directory");
}

#[test]
fn c_family_cmake_presets_skip_inherited_false_condition_build_and_test_presets() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "disabled-build-base", "hidden": true, "condition": false },
    { "name": "disabled-build-child", "inherits": "disabled-build-base" },
    { "name": "debug-build" }
  ],
  "testPresets": [
    { "name": "disabled-test-base", "hidden": true, "condition": false },
    { "name": "disabled-test-child", "inherits": ["disabled-test-base"] },
    { "name": "debug-test" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = plans
        .iter()
        .find(|plan| plan.id == "cpp-cmake-build")
        .expect("CMake build plan");
    let test = plans
        .iter()
        .find(|plan| plan.id == "cpp-ctest")
        .expect("CTest plan");

    assert_eq!(
        build.args,
        ["--build", "--preset", "debug-build", "--parallel", "1"]
    );
    assert_eq!(
        test.args,
        [
            "--preset",
            "debug-test",
            "--output-on-failure",
            "--no-tests=error",
            "--timeout",
            "120"
        ]
    );
}

#[test]
fn c_family_cmake_presets_reject_duplicate_build_and_test_names_without_fallback() {
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
        "# cache\n",
    )
    .unwrap();
    fs::write(
        root.path().join("build").join("CTestTestfile.cmake"),
        "# ctest\n",
    )
    .unwrap();
    fs::write(
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "debug-build" },
    { "name": "debug-build" }
  ],
  "testPresets": [
    { "name": "debug-test" },
    { "name": "debug-test" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);

    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);
    let test = blocked_plan(&plans, "cpp-ctest", DxToolTarget::Test);

    assert_blocked_detected_from(build, "CMakePresets.json");
    assert_blocked_detected_from(test, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "buildPresets");
    assert_blocked_reason_mentions(test, "CMakePresets.json");
    assert_blocked_reason_mentions(test, "testPresets");
}

#[test]
fn c_family_blocks_cmake_build_presets_with_install_package_or_clean_actions() {
    for (name, preset, expected_fragment) in [
        (
            "install-target",
            r#"{ "name": "install-build", "targets": ["install"] }"#,
            "install",
        ),
        (
            "install-strip-target",
            r#"{ "name": "install-strip-build", "targets": ["install/strip"] }"#,
            "install/strip",
        ),
        (
            "package-target",
            r#"{ "name": "package-build", "targets": "package" }"#,
            "package",
        ),
        (
            "package-source-target",
            r#"{ "name": "package-source-build", "targets": "package_source" }"#,
            "package_source",
        ),
        (
            "list-install-components-target",
            r#"{ "name": "list-install-components-build", "targets": "list_install_components" }"#,
            "list_install_components",
        ),
        (
            "clean-target",
            r#"{ "name": "clean-build", "targets": ["all", "clean"] }"#,
            "clean",
        ),
        (
            "clean-first",
            r#"{ "name": "clean-first-build", "cleanFirst": true }"#,
            "cleanFirst",
        ),
        (
            "native-tool-options",
            r#"{ "name": "native-tool-build", "nativeToolOptions": ["install"] }"#,
            "nativeToolOptions",
        ),
    ] {
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
            format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", root.path().display()),
        )
        .unwrap();
        fs::write(
            root.path().join("CMakePresets.json"),
            format!(
                r#"{{
  "version": 6,
  "buildPresets": [{preset}]
}}
"#
            ),
        )
        .unwrap();

        let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
        let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);

        assert_blocked_detected_from(build, "CMakePresets.json");
        assert_blocked_reason_mentions(build, "buildPresets");
        assert_blocked_reason_mentions(build, expected_fragment);
        assert_eq!(build.executable, "dx-check-blocked", "{name}");
    }
}

#[test]
fn c_family_blocks_cmake_build_presets_inheriting_unsafe_build_actions() {
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
        format!("CMAKE_HOME_DIRECTORY:INTERNAL={}\n", root.path().display()),
    )
    .unwrap();
    fs::write(
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "buildPresets": [
    { "name": "install-base", "hidden": true, "targets": ["install"] },
    { "name": "developer-build", "inherits": "install-base" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);

    assert_blocked_detected_from(build, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "buildPresets");
    assert_blocked_reason_mentions(build, "install");
}

#[test]
fn c_family_blocks_cmake_presets_with_configure_preset_binary_dir_outside_project() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "configurePresets": [
    { "name": "outside", "generator": "Ninja", "binaryDir": "../outside-build" }
  ],
  "buildPresets": [
    { "name": "outside-build", "configurePreset": "outside" }
  ],
  "testPresets": [
    { "name": "outside-test", "configurePreset": "outside" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck, DxToolTarget::Test]);
    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);
    let test = blocked_plan(&plans, "cpp-ctest", DxToolTarget::Test);

    assert_blocked_detected_from(build, "CMakePresets.json");
    assert_blocked_detected_from(test, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "configurePresets");
    assert_blocked_reason_mentions(test, "configurePresets");
    assert_blocked_reason_mentions(build, "binaryDir");
    assert_blocked_reason_mentions(test, "binaryDir");
}

#[test]
fn c_family_blocks_cmake_configure_preset_binary_dir_environment_macros() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
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
        root.path().join("CMakePresets.json"),
        r#"{
  "version": 6,
  "configurePresets": [
    { "name": "environment", "generator": "Ninja", "binaryDir": "$env{TEMP}/dx-check-build" }
  ],
  "buildPresets": [
    { "name": "environment-build", "configurePreset": "environment" }
  ]
}
"#,
    )
    .unwrap();

    let plans = plan_tools(root.path(), &[DxToolTarget::Typecheck]);
    let build = blocked_plan(&plans, "cpp-cmake-build", DxToolTarget::Typecheck);

    assert_blocked_detected_from(build, "CMakePresets.json");
    assert_blocked_reason_mentions(build, "configurePresets");
    assert_blocked_reason_mentions(build, "unsupported CMake macro");
}

#[cfg(unix)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

fn cmake_host_system_name_for_test() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Darwin"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "freebsd") {
        "FreeBSD"
    } else if cfg!(target_os = "openbsd") {
        "OpenBSD"
    } else if cfg!(target_os = "netbsd") {
        "NetBSD"
    } else if cfg!(target_os = "dragonfly") {
        "DragonFly"
    } else {
        std::env::consts::OS
    }
}

fn blocked_plan<'a>(plans: &'a [DxToolPlan], id: &str, target: DxToolTarget) -> &'a DxToolPlan {
    let plan = plans
        .iter()
        .find(|plan| plan.id == id)
        .unwrap_or_else(|| panic!("{id} blocked plan"));
    assert_eq!(plan.target, target);
    assert_eq!(plan.executable, "dx-check-blocked");
    assert_eq!(plan.parser, "blocked");
    plan
}

fn assert_blocked_detected_from(plan: &DxToolPlan, source: &str) {
    assert!(
        plan.detected_from.iter().any(|detected| detected == source),
        "{source} missing from blocked plan sources: {:?}",
        plan.detected_from
    );
}

fn assert_blocked_reason_mentions(plan: &DxToolPlan, fragment: &str) {
    assert!(
        plan.args.iter().any(|arg| arg.contains(fragment)),
        "{fragment} missing from blocked reason: {:?}",
        plan.args
    );
}
