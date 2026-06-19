use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::languages::{is_c_family_source_or_header, is_c_family_translation_unit};
use crate::model::{DxToolPlan, DxToolTarget};
use crate::path_filters::should_skip_generated_or_dependency_dir;
use crate::rules::source_scan::generated_source_leak;

const FILE_CHUNK_SIZE: usize = 100;

pub(super) fn plans(root: &Path, targets: &[DxToolTarget]) -> Vec<DxToolPlan> {
    let files = c_family_files(root);
    if files.is_empty() {
        return Vec::new();
    }

    let translation_units = files
        .iter()
        .filter(|file| is_c_family_translation_unit(&file.path))
        .cloned()
        .collect::<Vec<_>>();
    let format_configs = clang_format_configs(root, &files);
    let clang_tidy_config = clang_tidy_config(root, &files).is_some();
    let compile_database = compile_database(root, &translation_units);
    let usable_compile_database = compile_database
        .as_ref()
        .ok()
        .and_then(std::option::Option::as_ref);
    let mut plans = Vec::new();

    for target in targets {
        match target {
            DxToolTarget::Format if !format_configs.is_empty() => {
                plans.extend(clang_format_plans(root, &files, &format_configs));
            }
            DxToolTarget::Lint => {
                if clang_tidy_config {
                    match &compile_database {
                        Ok(Some(database)) => {
                            plans.extend(clang_tidy_plans(root, &translation_units, database));
                        }
                        Err(error) => plans.push(blocked_compile_database_plan(
                            root,
                            "cpp-clang-tidy",
                            DxToolTarget::Lint,
                            error,
                            true,
                        )),
                        Ok(None) => {}
                    }
                }
                plans.extend(cppcheck_plans(root, &files, usable_compile_database));
            }
            DxToolTarget::Typecheck => {
                match &compile_database {
                    Ok(Some(database)) => {
                        plans.extend(clangd_check_plans(root, &translation_units, database));
                    }
                    Err(error) => plans.push(blocked_compile_database_plan(
                        root,
                        "cpp-clangd-check",
                        DxToolTarget::Typecheck,
                        error,
                        false,
                    )),
                    Ok(None) => {}
                }
                if let Some(plan) = cmake_build_plan(root) {
                    plans.push(plan);
                }
            }
            DxToolTarget::Test => {
                if let Some(plan) = ctest_plan(root) {
                    plans.push(plan);
                }
            }
            _ => {}
        }
    }

    plans
}

fn clang_format_plans(root: &Path, files: &[CFamilyFile], configs: &[String]) -> Vec<DxToolPlan> {
    files
        .chunks(FILE_CHUNK_SIZE)
        .enumerate()
        .map(|(index, chunk)| {
            let mut args = vec![
                "--dry-run".to_string(),
                "--Werror".to_string(),
                "--style=file".to_string(),
            ];
            args.extend(chunk.iter().map(|file| tool_file_arg(&file.relative)));
            DxToolPlan {
                id: chunked_id("cpp-clang-format-check", index),
                target: DxToolTarget::Format,
                executable: "clang-format".to_string(),
                args,
                cwd: root.to_path_buf(),
                detected_from: configs.to_vec(),
                parser: "clang-format".to_string(),
            }
        })
        .collect()
}

fn clang_tidy_plans(
    root: &Path,
    files: &[CFamilyFile],
    database: &CompileDatabase,
) -> Vec<DxToolPlan> {
    files
        .chunks(FILE_CHUNK_SIZE)
        .enumerate()
        .map(|(index, chunk)| {
            let mut args = vec![
                "-p".to_string(),
                database.directory.clone(),
                "--quiet".to_string(),
                "--warnings-as-errors=*".to_string(),
            ];
            args.extend(chunk.iter().map(|file| tool_file_arg(&file.relative)));
            DxToolPlan {
                id: chunked_id("cpp-clang-tidy", index),
                target: DxToolTarget::Lint,
                executable: "clang-tidy".to_string(),
                args,
                cwd: root.to_path_buf(),
                detected_from: vec![database.file.clone(), ".clang-tidy".to_string()],
                parser: "clang-tidy".to_string(),
            }
        })
        .collect()
}

fn clangd_check_plans(
    root: &Path,
    files: &[CFamilyFile],
    database: &CompileDatabase,
) -> Vec<DxToolPlan> {
    files
        .iter()
        .enumerate()
        .map(|(index, file)| DxToolPlan {
            id: chunked_id("cpp-clangd-check", index),
            target: DxToolTarget::Typecheck,
            executable: "clangd".to_string(),
            args: vec![
                format!("--check={}", tool_file_arg(&file.relative)),
                format!("--compile-commands-dir={}", database.directory),
                "--pretty=false".to_string(),
            ],
            cwd: root.to_path_buf(),
            detected_from: vec![database.file.clone()],
            parser: "clangd".to_string(),
        })
        .collect()
}

fn cppcheck_plans(
    root: &Path,
    files: &[CFamilyFile],
    database: Option<&CompileDatabase>,
) -> Vec<DxToolPlan> {
    let base_args = vec![
        "--enable=warning,style,performance,portability".to_string(),
        "--error-exitcode=1".to_string(),
        "--xml".to_string(),
        "--xml-version=2".to_string(),
        "--quiet".to_string(),
    ];

    if let Some(database) = database
        && database.project_only
    {
        let mut args = base_args;
        args.push(format!("--project={}", database.file));
        return vec![DxToolPlan {
            id: "cpp-cppcheck".to_string(),
            target: DxToolTarget::Lint,
            executable: "cppcheck".to_string(),
            args,
            cwd: root.to_path_buf(),
            detected_from: vec![database.file.clone()],
            parser: "cppcheck-xml".to_string(),
        }];
    }

    files
        .chunks(FILE_CHUNK_SIZE)
        .enumerate()
        .map(|(index, chunk)| {
            let mut args = base_args.clone();
            args.extend(chunk.iter().map(|file| tool_file_arg(&file.relative)));
            DxToolPlan {
                id: chunked_id("cpp-cppcheck", index),
                target: DxToolTarget::Lint,
                executable: "cppcheck".to_string(),
                args,
                cwd: root.to_path_buf(),
                detected_from: chunk.iter().map(|file| file.relative.clone()).collect(),
                parser: "cppcheck-xml".to_string(),
            }
        })
        .collect()
}

fn ctest_plan(root: &Path) -> Option<DxToolPlan> {
    match cmake_test_preset(root) {
        Ok(Some((source, preset))) => {
            return Some(DxToolPlan {
                id: "cpp-ctest".to_string(),
                target: DxToolTarget::Test,
                executable: "ctest".to_string(),
                args: vec![
                    "--preset".to_string(),
                    preset,
                    "--output-on-failure".to_string(),
                    "--no-tests=error".to_string(),
                    "--timeout".to_string(),
                    "120".to_string(),
                ],
                cwd: root.to_path_buf(),
                detected_from: vec![source],
                parser: "ctest".to_string(),
            });
        }
        Err(error) => {
            return Some(blocked_cmake_plan(
                root,
                "cpp-ctest",
                DxToolTarget::Test,
                error,
            ));
        }
        Ok(None) => {}
    }

    match ctest_build_dir(root) {
        Ok(Some((test_dir, test_file))) => Some(DxToolPlan {
            id: "cpp-ctest".to_string(),
            target: DxToolTarget::Test,
            executable: "ctest".to_string(),
            args: vec![
                "--test-dir".to_string(),
                test_dir,
                "--output-on-failure".to_string(),
                "--no-tests=error".to_string(),
                "--timeout".to_string(),
                "120".to_string(),
            ],
            cwd: root.to_path_buf(),
            detected_from: vec![test_file],
            parser: "ctest".to_string(),
        }),
        Err(error) => Some(blocked_cmake_fallback_plan(
            root,
            "cpp-ctest",
            DxToolTarget::Test,
            error,
        )),
        Ok(None) => None,
    }
}

fn cmake_build_plan(root: &Path) -> Option<DxToolPlan> {
    if !root.join("CMakeLists.txt").is_file() {
        return None;
    }

    match cmake_build_preset(root) {
        Ok(Some((source, preset))) => {
            return Some(DxToolPlan {
                id: "cpp-cmake-build".to_string(),
                target: DxToolTarget::Typecheck,
                executable: "cmake".to_string(),
                args: vec![
                    "--build".to_string(),
                    "--preset".to_string(),
                    preset.to_string(),
                    "--parallel".to_string(),
                    "1".to_string(),
                ],
                cwd: root.to_path_buf(),
                detected_from: vec![source],
                parser: "cxx-compiler".to_string(),
            });
        }
        Err(error) => {
            return Some(blocked_cmake_plan(
                root,
                "cpp-cmake-build",
                DxToolTarget::Typecheck,
                error,
            ));
        }
        Ok(None) => {}
    }

    match cmake_configured_build_dir(root) {
        Ok(Some((directory, cache))) => Some(DxToolPlan {
            id: "cpp-cmake-build".to_string(),
            target: DxToolTarget::Typecheck,
            executable: "cmake".to_string(),
            args: vec![
                "--build".to_string(),
                directory,
                "--parallel".to_string(),
                "1".to_string(),
            ],
            cwd: root.to_path_buf(),
            detected_from: vec![cache],
            parser: "cxx-compiler".to_string(),
        }),
        Err(error) => Some(blocked_cmake_fallback_plan(
            root,
            "cpp-cmake-build",
            DxToolTarget::Typecheck,
            error,
        )),
        Ok(None) => None,
    }
}

type CMakePresetSelection = Result<Option<(String, String)>, CMakePresetError>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CMakePresetError {
    source: String,
    section: String,
    detail: Option<String>,
}

impl CMakePresetError {
    fn new(source: impl Into<String>, section: &str) -> Self {
        Self {
            source: source.into(),
            section: section.to_string(),
            detail: None,
        }
    }

    fn with_detail(source: impl Into<String>, section: &str, detail: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            section: section.to_string(),
            detail: Some(detail.into()),
        }
    }

    fn reason(&self) -> String {
        let mut reason = format!(
            "{} {} could not be used safely; fix invalid, duplicate, or unsupported CMake preset metadata before running check",
            self.source, self.section
        );
        if let Some(detail) = &self.detail {
            reason.push_str(": ");
            reason.push_str(detail);
        }
        reason
    }
}

fn blocked_cmake_plan(
    root: &Path,
    id: &str,
    target: DxToolTarget,
    error: CMakePresetError,
) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "dx-check-blocked".to_string(),
        args: vec![error.reason()],
        cwd: root.to_path_buf(),
        detected_from: vec![error.source],
        parser: "blocked".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CMakeFallbackError {
    file: String,
    source: CMakeFallbackSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CMakeFallbackSource {
    Cache,
    CTest,
}

impl CMakeFallbackError {
    fn new(file: String, source: CMakeFallbackSource) -> Self {
        Self { file, source }
    }

    fn reason(&self) -> String {
        format!(
            "{} {} source directory does not match this project; regenerate the CMake build directory before running check",
            self.file,
            self.source.label()
        )
    }
}

impl CMakeFallbackSource {
    fn label(self) -> &'static str {
        match self {
            Self::Cache => "CMake cache",
            Self::CTest => "CTest",
        }
    }
}

fn blocked_cmake_fallback_plan(
    root: &Path,
    id: &str,
    target: DxToolTarget,
    error: CMakeFallbackError,
) -> DxToolPlan {
    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "dx-check-blocked".to_string(),
        args: vec![error.reason()],
        cwd: root.to_path_buf(),
        detected_from: vec![error.file],
        parser: "blocked".to_string(),
    }
}

fn cmake_build_preset(root: &Path) -> CMakePresetSelection {
    cmake_preset(root, "buildPresets")
}

fn cmake_test_preset(root: &Path) -> CMakePresetSelection {
    cmake_preset(root, "testPresets")
}

fn cmake_preset(root: &Path, section: &str) -> CMakePresetSelection {
    let mut names = BTreeSet::new();
    let mut presets_by_name = BTreeMap::new();
    let mut candidates = Vec::new();
    let mut documents = Vec::new();

    for name in ["CMakePresets.json", "CMakeUserPresets.json"] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(body) = fs::read_to_string(&path) else {
            return Err(CMakePresetError::new(name, section));
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
            return Err(CMakePresetError::new(name, section));
        };
        documents.push((name.to_string(), value));
    }

    for (name, value) in &documents {
        for preset_name in
            cmake_read_named_preset_section(name, value, section, &mut names, &mut presets_by_name)?
        {
            candidates.push((name.clone(), preset_name));
        }
    }

    if candidates.is_empty() {
        return Ok(None);
    }

    let mut configure_names = BTreeSet::new();
    let mut configure_presets_by_name = BTreeMap::new();
    for (name, value) in &documents {
        cmake_read_named_preset_section(
            name,
            value,
            "configurePresets",
            &mut configure_names,
            &mut configure_presets_by_name,
        )?;
    }

    for (source, preset_name) in candidates {
        let Some(preset) = presets_by_name.get(&preset_name) else {
            return Err(CMakePresetError::new(source, section));
        };
        if cmake_preset_is_runnable(preset, &presets_by_name, true, &mut BTreeSet::new())
            .map_err(|()| CMakePresetError::new(source.clone(), section))?
        {
            if section == "buildPresets" {
                cmake_build_preset_action_is_safe(preset, &presets_by_name, &mut BTreeSet::new())
                    .map_err(|detail| {
                        CMakePresetError::with_detail(source.clone(), section, detail)
                    })?;
            }
            cmake_preset_configure_reference_is_safe(
                root,
                preset,
                &presets_by_name,
                &configure_presets_by_name,
            )
            .map_err(|detail| {
                CMakePresetError::with_detail(source.clone(), "configurePresets", detail)
            })?;
            return Ok(Some((source, preset_name)));
        }
    }

    Ok(None)
}

fn cmake_read_named_preset_section(
    source: &str,
    document: &serde_json::Value,
    section: &str,
    names: &mut BTreeSet<String>,
    presets_by_name: &mut BTreeMap<String, serde_json::Value>,
) -> Result<Vec<String>, CMakePresetError> {
    let Some(section_value) = document.get(section) else {
        return Ok(Vec::new());
    };
    let Some(presets) = section_value.as_array() else {
        return Err(CMakePresetError::new(source, section));
    };

    let mut ordered_names = Vec::new();
    for preset in presets {
        let Some(preset_name) = cmake_preset_name(preset) else {
            return Err(CMakePresetError::new(source, section));
        };
        if !names.insert(preset_name.to_string()) {
            return Err(CMakePresetError::new(source, section));
        }
        presets_by_name.insert(preset_name.to_string(), preset.clone());
        ordered_names.push(preset_name.to_string());
    }

    Ok(ordered_names)
}

fn cmake_preset_configure_reference_is_safe(
    root: &Path,
    preset: &serde_json::Value,
    presets_by_name: &BTreeMap<String, serde_json::Value>,
    configure_presets_by_name: &BTreeMap<String, serde_json::Value>,
) -> Result<(), String> {
    let Some(configure_preset_name) =
        cmake_inherited_string_field(preset, presets_by_name, "configurePreset")?
    else {
        return Ok(());
    };
    let Some(configure_preset) = configure_presets_by_name.get(&configure_preset_name) else {
        return Err(format!(
            "configurePreset `{configure_preset_name}` is missing from configurePresets"
        ));
    };
    if !cmake_preset_is_runnable(
        configure_preset,
        configure_presets_by_name,
        false,
        &mut BTreeSet::new(),
    )
    .map_err(|()| format!("configurePreset `{configure_preset_name}` has unsupported metadata"))?
    {
        return Err(format!(
            "configurePreset `{configure_preset_name}` is disabled by condition"
        ));
    }
    cmake_configure_binary_dir_is_safe(
        root,
        &configure_preset_name,
        configure_preset,
        configure_presets_by_name,
        &mut BTreeSet::new(),
    )
}

fn cmake_inherited_string_field(
    preset: &serde_json::Value,
    presets_by_name: &BTreeMap<String, serde_json::Value>,
    field: &str,
) -> Result<Option<String>, String> {
    cmake_inherited_string_field_inner(preset, presets_by_name, field, &mut BTreeSet::new())
}

fn cmake_inherited_string_field_inner(
    preset: &serde_json::Value,
    presets_by_name: &BTreeMap<String, serde_json::Value>,
    field: &str,
    visiting: &mut BTreeSet<String>,
) -> Result<Option<String>, String> {
    if let Some(value) = preset.get(field) {
        return value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| format!("{field} must be a non-empty string"));
    }

    let Some(inherits) = cmake_preset_inherits(preset) else {
        return Err("inherits must be a string or string array".to_string());
    };
    for inherited in inherits {
        if !visiting.insert(inherited.to_string()) {
            return Err("preset inheritance contains a cycle".to_string());
        }
        let Some(parent) = presets_by_name.get(inherited) else {
            return Err(format!("preset inherits missing preset `{inherited}`"));
        };
        if let Some(value) =
            cmake_inherited_string_field_inner(parent, presets_by_name, field, visiting)?
        {
            visiting.remove(inherited);
            return Ok(Some(value));
        }
        visiting.remove(inherited);
    }

    Ok(None)
}

fn cmake_configure_binary_dir_is_safe(
    root: &Path,
    configure_preset_name: &str,
    configure_preset: &serde_json::Value,
    configure_presets_by_name: &BTreeMap<String, serde_json::Value>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), String> {
    if let Some(binary_dir) = configure_preset.get("binaryDir") {
        let binary_dir = binary_dir
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "configurePreset binaryDir must be a non-empty string".to_string())?;
        return validate_cmake_configure_binary_dir(root, configure_preset_name, binary_dir);
    }

    let Some(inherits) = cmake_preset_inherits(configure_preset) else {
        return Err("configurePreset inherits must be a string or string array".to_string());
    };
    for inherited in inherits {
        if !visiting.insert(inherited.to_string()) {
            return Err("configurePreset inheritance contains a cycle".to_string());
        }
        let Some(parent) = configure_presets_by_name.get(inherited) else {
            return Err(format!(
                "configurePreset inherits missing preset `{inherited}`"
            ));
        };
        cmake_configure_binary_dir_is_safe(
            root,
            inherited,
            parent,
            configure_presets_by_name,
            visiting,
        )?;
        visiting.remove(inherited);
    }

    Ok(())
}

fn validate_cmake_configure_binary_dir(
    root: &Path,
    configure_preset_name: &str,
    binary_dir: &str,
) -> Result<(), String> {
    let expanded = cmake_expand_binary_dir_macros(root, configure_preset_name, binary_dir)?;
    let path = Path::new(&expanded);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if project_relative_path(root, &path).is_none() {
        return Err(format!(
            "configurePreset `{configure_preset_name}` binaryDir must resolve inside the project"
        ));
    }

    Ok(())
}

fn cmake_expand_binary_dir_macros(
    root: &Path,
    configure_preset_name: &str,
    value: &str,
) -> Result<String, String> {
    let source_dir = normalize_path(&root.to_string_lossy());
    let source_parent_dir = root
        .parent()
        .map(|parent| normalize_path(&parent.to_string_lossy()))
        .unwrap_or_else(|| source_dir.clone());
    let source_dir_name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let expanded = value
        .replace("${sourceDir}", &source_dir)
        .replace("${sourceParentDir}", &source_parent_dir)
        .replace("${sourceDirName}", &source_dir_name)
        .replace("${presetName}", configure_preset_name)
        .replace("${hostSystemName}", cmake_host_system_name());

    let lower = expanded.to_ascii_lowercase();
    if expanded.contains("${") || lower.contains("$env{") || lower.contains("$penv{") {
        return Err("configurePreset binaryDir contains an unsupported CMake macro".to_string());
    }

    Ok(expanded)
}

fn cmake_build_preset_action_is_safe(
    preset: &serde_json::Value,
    presets_by_name: &BTreeMap<String, serde_json::Value>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), String> {
    if let Some(clean_first) = preset.get("cleanFirst") {
        match clean_first.as_bool() {
            Some(true) => {
                return Err("build preset cleanFirst would clean build artifacts".to_string());
            }
            Some(false) => {}
            None => return Err("build preset cleanFirst must be a boolean".to_string()),
        }
    }
    if let Some(targets) = preset.get("targets") {
        validate_cmake_build_targets(targets)?;
    }
    if let Some(native_options) = preset.get("nativeToolOptions") {
        let options = native_options
            .as_array()
            .ok_or_else(|| "build preset nativeToolOptions must be an array".to_string())?;
        if !options.is_empty() {
            return Err(
                "build preset nativeToolOptions can change native build-tool behavior".to_string(),
            );
        }
    }

    let Some(inherits) = cmake_preset_inherits(preset) else {
        return Err("build preset inherits must be a string or string array".to_string());
    };
    for inherited in inherits {
        if !visiting.insert(inherited.to_string()) {
            return Err("build preset inheritance contains a cycle".to_string());
        }
        let Some(parent) = presets_by_name.get(inherited) else {
            return Err(format!(
                "build preset inherits missing preset `{inherited}`"
            ));
        };
        cmake_build_preset_action_is_safe(parent, presets_by_name, visiting)?;
        visiting.remove(inherited);
    }

    Ok(())
}

fn validate_cmake_build_targets(targets: &serde_json::Value) -> Result<(), String> {
    match targets {
        serde_json::Value::String(target) => validate_cmake_build_target(target),
        serde_json::Value::Array(targets) => {
            for target in targets {
                let target = target
                    .as_str()
                    .ok_or_else(|| "build preset targets must be strings".to_string())?;
                validate_cmake_build_target(target)?;
            }
            Ok(())
        }
        _ => Err("build preset targets must be a string or string array".to_string()),
    }
}

fn validate_cmake_build_target(target: &str) -> Result<(), String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("build preset targets must not be empty".to_string());
    }
    if cmake_build_target_is_generated_mutation(target) {
        return Err(format!("build preset target `{target}` is not check-safe"));
    }

    Ok(())
}

fn cmake_build_target_is_generated_mutation(target: &str) -> bool {
    let target = target.to_ascii_lowercase();
    target == "install"
        || target.starts_with("install/")
        || target == "package"
        || target == "package_source"
        || target == "list_install_components"
        || target == "clean"
}

fn cmake_preset_name(preset: &serde_json::Value) -> Option<&str> {
    preset
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn cmake_preset_is_runnable(
    preset: &serde_json::Value,
    presets_by_name: &BTreeMap<String, serde_json::Value>,
    check_hidden: bool,
    visiting: &mut BTreeSet<String>,
) -> Result<bool, ()> {
    if check_hidden
        && preset
            .get("hidden")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        return Ok(false);
    }
    if !cmake_preset_condition_is_enabled(preset.get("condition"))? {
        return Ok(false);
    }

    let Some(inherits) = cmake_preset_inherits(preset) else {
        return Err(());
    };
    for inherited in inherits {
        if !visiting.insert(inherited.to_string()) {
            return Err(());
        }
        let Some(parent) = presets_by_name.get(inherited) else {
            return Err(());
        };
        if !cmake_preset_is_runnable(parent, presets_by_name, false, visiting)? {
            return Ok(false);
        }
        visiting.remove(inherited);
    }

    Ok(true)
}

fn cmake_preset_inherits(preset: &serde_json::Value) -> Option<Vec<&str>> {
    match preset.get("inherits") {
        None => Some(Vec::new()),
        Some(serde_json::Value::String(name)) => {
            let name = name.trim();
            (!name.is_empty()).then_some(vec![name])
        }
        Some(serde_json::Value::Array(names)) => names
            .iter()
            .map(|name| name.as_str().map(str::trim).filter(|name| !name.is_empty()))
            .collect(),
        Some(_) => None,
    }
}

fn cmake_preset_condition_is_enabled(condition: Option<&serde_json::Value>) -> Result<bool, ()> {
    match condition {
        None => Ok(true),
        Some(value) => cmake_condition_value_is_enabled(value, true),
    }
}

fn cmake_condition_value_is_enabled(
    condition: &serde_json::Value,
    allow_null: bool,
) -> Result<bool, ()> {
    match condition {
        serde_json::Value::Bool(enabled) => Ok(*enabled),
        serde_json::Value::Null if allow_null => Ok(true),
        serde_json::Value::Null => Err(()),
        serde_json::Value::Object(object) => cmake_condition_object_is_enabled(object),
        _ => Err(()),
    }
}

fn cmake_condition_object_is_enabled(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<bool, ()> {
    match object.get("type").and_then(serde_json::Value::as_str) {
        Some("const") => object
            .get("value")
            .and_then(serde_json::Value::as_bool)
            .ok_or(()),
        Some("equals") => {
            let (lhs, rhs) = cmake_condition_string_pair(object, "lhs", "rhs").ok_or(())?;
            Ok(lhs == rhs)
        }
        Some("notEquals") => {
            let (lhs, rhs) = cmake_condition_string_pair(object, "lhs", "rhs").ok_or(())?;
            Ok(lhs != rhs)
        }
        Some("inList") => {
            let (needle, values) = cmake_condition_string_list(object).ok_or(())?;
            Ok(values.contains(&needle))
        }
        Some("notInList") => {
            let (needle, values) = cmake_condition_string_list(object).ok_or(())?;
            Ok(!values.contains(&needle))
        }
        Some("matches") => {
            let (value, pattern) =
                cmake_condition_string_pair(object, "string", "regex").ok_or(())?;
            let regex = regex::Regex::new(&pattern).map_err(|_| ())?;
            Ok(regex.is_match(&value))
        }
        Some("notMatches") => {
            let (value, pattern) =
                cmake_condition_string_pair(object, "string", "regex").ok_or(())?;
            let regex = regex::Regex::new(&pattern).map_err(|_| ())?;
            Ok(!regex.is_match(&value))
        }
        Some("anyOf") => {
            for condition in cmake_condition_children(object).ok_or(())? {
                if cmake_condition_value_is_enabled(condition, false)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Some("allOf") => {
            for condition in cmake_condition_children(object).ok_or(())? {
                if !cmake_condition_value_is_enabled(condition, false)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Some("not") => Ok(!cmake_condition_value_is_enabled(
            object.get("condition").ok_or(())?,
            false,
        )?),
        _ => Err(()),
    }
}

fn cmake_condition_string_pair(
    object: &serde_json::Map<String, serde_json::Value>,
    lhs_key: &str,
    rhs_key: &str,
) -> Option<(String, String)> {
    Some((
        cmake_condition_string(object, lhs_key)?,
        cmake_condition_string(object, rhs_key)?,
    ))
}

fn cmake_condition_string_list(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<(String, Vec<String>)> {
    let needle = cmake_condition_string(object, "string")?;
    let values = object
        .get("list")?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(cmake_expand_condition_macros))
        .collect::<Option<Vec<_>>>()?;
    Some((needle, values))
}

fn cmake_condition_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    object.get(key)?.as_str().map(cmake_expand_condition_macros)
}

fn cmake_expand_condition_macros(value: &str) -> String {
    value.replace("${hostSystemName}", cmake_host_system_name())
}

fn cmake_host_system_name() -> &'static str {
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

fn cmake_condition_children(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<&Vec<serde_json::Value>> {
    object.get("conditions")?.as_array()
}

fn cmake_configured_build_dir(root: &Path) -> Result<Option<(String, String)>, CMakeFallbackError> {
    for directory in known_build_directories(root) {
        let cache = directory.join("CMakeCache.txt");
        if !cache.is_file() {
            continue;
        }
        let cache_file = relative_path(root, &cache);
        if cmake_cache_source_dir(&cache)
            .is_some_and(|source| generated_source_matches_root(root, &source))
        {
            return Ok(Some((relative_path(root, &directory), cache_file)));
        }
        return Err(CMakeFallbackError::new(
            cache_file,
            CMakeFallbackSource::Cache,
        ));
    }

    Ok(None)
}

fn chunked_id(base: &str, index: usize) -> String {
    if index == 0 {
        base.to_string()
    } else {
        format!("{base}-{:03}", index + 1)
    }
}

#[derive(Debug, Clone)]
struct CFamilyFile {
    path: PathBuf,
    relative: String,
}

fn c_family_files(root: &Path) -> Vec<CFamilyFile> {
    let Ok(canonical_root) = fs::canonicalize(root) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    visit(root, &canonical_root, &mut |path| {
        if is_c_family_source_or_header(path) {
            let relative = relative_path(root, path);
            if generated_source_leak(&relative, path) {
                return;
            }
            files.push(CFamilyFile {
                path: path.to_path_buf(),
                relative,
            });
        }
    });
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    files
}

fn clang_format_configs(root: &Path, files: &[CFamilyFile]) -> Vec<String> {
    let mut configs = Vec::new();
    for name in [".clang-format", "_clang-format"] {
        if root.join(name).is_file() {
            configs.push(name.to_string());
        }
    }

    for file in files {
        if let Some(parent) = file.path.parent() {
            for name in [".clang-format", "_clang-format"] {
                let path = parent.join(name);
                if path.is_file() {
                    let relative = relative_path(root, &path);
                    if !configs.contains(&relative) {
                        configs.push(relative);
                    }
                }
            }
        }
    }
    configs
}

fn clang_tidy_config(root: &Path, files: &[CFamilyFile]) -> Option<String> {
    if root.join(".clang-tidy").is_file() {
        return Some(".clang-tidy".to_string());
    }

    for file in files {
        let Some(parent) = file.path.parent() else {
            continue;
        };
        let path = parent.join(".clang-tidy");
        if path.is_file() {
            return Some(relative_path(root, &path));
        }
    }

    None
}

#[derive(Debug, Clone)]
struct CompileDatabase {
    file: String,
    directory: String,
    project_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompileDatabaseError {
    file: String,
}

impl CompileDatabaseError {
    fn reason(&self) -> String {
        format!(
            "{} compile database could not be used safely; regenerate it so every hand-authored translation unit has valid in-project compiler context",
            self.file
        )
    }
}

fn blocked_compile_database_plan(
    root: &Path,
    id: &str,
    target: DxToolTarget,
    error: &CompileDatabaseError,
    include_tidy_config: bool,
) -> DxToolPlan {
    let mut detected_from = vec![error.file.clone()];
    if include_tidy_config {
        detected_from.push(".clang-tidy".to_string());
    }

    DxToolPlan {
        id: id.to_string(),
        target,
        executable: "dx-check-blocked".to_string(),
        args: vec![error.reason()],
        cwd: root.to_path_buf(),
        detected_from,
        parser: "blocked".to_string(),
    }
}

fn compile_database(
    root: &Path,
    translation_units: &[CFamilyFile],
) -> Result<Option<CompileDatabase>, CompileDatabaseError> {
    if translation_units.is_empty() {
        return Ok(None);
    }

    let mut first_error = None;
    for path in compile_database_candidates(root) {
        if !path.is_file() {
            continue;
        }
        match compile_database_coverage(root, &path, translation_units) {
            Some(coverage) => {
                let file = relative_path(root, &path);
                let directory = path
                    .parent()
                    .map(|parent| relative_path(root, parent))
                    .filter(|relative| !relative.is_empty())
                    .unwrap_or_else(|| ".".to_string());
                return Ok(Some(CompileDatabase {
                    file,
                    directory,
                    project_only: coverage.project_only,
                }));
            }
            None => {
                first_error.get_or_insert_with(|| CompileDatabaseError {
                    file: relative_path(root, &path),
                });
            }
        }
    }

    if let Some(error) = first_error {
        Err(error)
    } else {
        Ok(None)
    }
}

fn compile_database_candidates(root: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![root.join("compile_commands.json")];
    for directory in known_build_directories(root) {
        candidates.push(directory.join("compile_commands.json"));
    }
    candidates
}

#[derive(Debug, Clone, Copy)]
struct CompileDatabaseCoverage {
    project_only: bool,
}

fn compile_database_coverage(
    root: &Path,
    path: &Path,
    translation_units: &[CFamilyFile],
) -> Option<CompileDatabaseCoverage> {
    if translation_units.is_empty() {
        return None;
    }

    let Ok(body) = fs::read_to_string(path) else {
        return None;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
        return None;
    };
    let entries = value.as_array()?;
    if entries.is_empty() {
        return None;
    }

    let mut covered = BTreeSet::new();
    let translation_unit_paths = translation_units
        .iter()
        .map(|unit| unit.relative.as_str())
        .collect::<BTreeSet<_>>();
    let mut project_only = true;
    for entry in entries {
        let file = entry
            .get("file")
            .and_then(serde_json::Value::as_str)
            .filter(|file| !file.trim().is_empty())?;
        let directory = entry.get("directory").and_then(serde_json::Value::as_str);
        if !compile_entry_has_invocation(entry) {
            return None;
        }

        let Some(relative) = compile_entry_project_relative(root, directory, file) else {
            project_only = false;
            continue;
        };
        if !translation_unit_paths.contains(relative.as_str()) {
            project_only = false;
            continue;
        }
        covered.insert(relative);
    }

    if translation_units
        .iter()
        .all(|unit| covered.contains(&unit.relative))
    {
        Some(CompileDatabaseCoverage { project_only })
    } else {
        None
    }
}

fn compile_entry_has_invocation(entry: &serde_json::Value) -> bool {
    if entry
        .get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|command| !command.trim().is_empty())
    {
        return true;
    }

    entry
        .get("arguments")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|arguments| {
            !arguments.is_empty()
                && arguments.iter().all(|argument| {
                    argument
                        .as_str()
                        .is_some_and(|value| !value.trim().is_empty())
                })
        })
}

fn compile_entry_project_relative(
    root: &Path,
    entry_directory: Option<&str>,
    entry_file: &str,
) -> Option<String> {
    let entry_path = Path::new(entry_file);
    let absolute = if entry_path.is_absolute() {
        entry_path.to_path_buf()
    } else {
        let directory = entry_directory
            .map(str::trim)
            .filter(|directory| !directory.is_empty())
            .unwrap_or(".");
        let directory_path = Path::new(directory);
        let base = if directory_path.is_absolute() {
            directory_path.to_path_buf()
        } else {
            root.join(directory_path)
        };
        base.join(entry_path)
    };
    project_relative_path(root, &absolute)
}

fn ctest_build_dir(root: &Path) -> Result<Option<(String, String)>, CMakeFallbackError> {
    for directory in known_build_directories(root) {
        let test_file = directory.join("CTestTestfile.cmake");
        if !test_file.is_file() {
            continue;
        }
        let test_file_relative = relative_path(root, &test_file);
        if ctest_source_dir(&test_file)
            .is_some_and(|source| generated_source_matches_root(root, &source))
        {
            return Ok(Some((relative_path(root, &directory), test_file_relative)));
        }
        return Err(CMakeFallbackError::new(
            test_file_relative,
            CMakeFallbackSource::CTest,
        ));
    }

    Ok(None)
}

fn cmake_cache_source_dir(cache: &Path) -> Option<PathBuf> {
    let body = fs::read_to_string(cache).ok()?;
    body.lines()
        .filter_map(|line| {
            let metadata = line.trim().strip_prefix("CMAKE_HOME_DIRECTORY:")?;
            let (_, source) = metadata.split_once('=')?;
            non_empty_generated_source(source)
        })
        .next()
}

fn ctest_source_dir(test_file: &Path) -> Option<PathBuf> {
    let body = fs::read_to_string(test_file).ok()?;
    body.lines()
        .filter_map(|line| {
            let source = line.trim().strip_prefix("# Source directory:")?;
            non_empty_generated_source(source)
        })
        .next()
}

fn non_empty_generated_source(source: &str) -> Option<PathBuf> {
    let source = source.trim().trim_matches('"');
    (!source.is_empty()).then(|| PathBuf::from(source))
}

fn generated_source_matches_root(root: &Path, source: &Path) -> bool {
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        root.join(source)
    };
    project_relative_path(root, &source).as_deref() == Some(".")
}

fn known_build_directories(root: &Path) -> Vec<PathBuf> {
    let Ok(canonical_root) = fs::canonicalize(root) else {
        return Vec::new();
    };
    let mut directories = Vec::new();
    let build = root.join("build");
    if build_dir_is_inside_project(&canonical_root, &build) {
        push_build_directory(&mut directories, &canonical_root, &build);
    } else {
        let build = root.join("Build");
        if build_dir_is_inside_project(&canonical_root, &build) {
            push_build_directory(&mut directories, &canonical_root, &build);
        }
    }

    let out_build = root.join("out").join("build");
    if build_dir_is_inside_project(&canonical_root, &out_build) {
        push_build_directory(&mut directories, &canonical_root, &out_build);
    }

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("cmake-build-") && path_resolves_inside(&canonical_root, &path) {
                directories.push(path);
            }
        }
    }

    directories.sort();
    directories.dedup();
    directories
}

fn push_build_directory(directories: &mut Vec<PathBuf>, canonical_root: &Path, directory: &Path) {
    directories.push(directory.to_path_buf());
    directories.extend(child_directories(directory, canonical_root));
}

fn build_dir_is_inside_project(canonical_root: &Path, directory: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(directory) else {
        return false;
    };
    !metadata.file_type().is_symlink()
        && metadata.is_dir()
        && path_resolves_inside(canonical_root, directory)
}

fn child_directories(directory: &Path, canonical_root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                return None;
            };
            (file_type.is_dir()
                && !file_type.is_symlink()
                && path_resolves_inside(canonical_root, &path))
            .then_some(path)
        })
        .collect()
}

fn visit(dir: &Path, canonical_root: &Path, callback: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if file_type.is_dir() {
            if should_skip_dir(&name) {
                continue;
            }
            if !path_resolves_inside(canonical_root, &path) {
                continue;
            }
            visit(&path, canonical_root, callback);
        } else if file_type.is_file() && path_resolves_inside(canonical_root, &path) {
            callback(&path);
        }
    }
}

fn path_resolves_inside(canonical_root: &Path, path: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|canonical| canonical.starts_with(canonical_root))
}

fn should_skip_dir(name: &str) -> bool {
    should_skip_generated_or_dependency_dir(name)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn tool_file_arg(relative: &str) -> String {
    if relative.starts_with('-') || relative.starts_with('@') {
        format!("./{relative}")
    } else {
        relative.to_string()
    }
}

fn project_relative_path(root: &Path, path: &Path) -> Option<String> {
    let root = normalize_path(&root.to_string_lossy());
    let path = normalize_path(&path.to_string_lossy());
    let root_cmp = root.to_ascii_lowercase();
    let path_cmp = path.to_ascii_lowercase();
    if path_cmp == root_cmp {
        return Some(".".to_string());
    }
    let prefix = format!("{root_cmp}/");
    if !path_cmp.starts_with(&prefix) {
        return None;
    }
    Some(path[root.len() + 1..].to_string())
}

fn normalize_path(value: &str) -> String {
    let value = value.trim().replace('\\', "/");
    let (prefix, rest) = if value.as_bytes().get(1) == Some(&b':') {
        (&value[..2], &value[2..])
    } else {
        ("", value.as_str())
    };
    let leading_slash = rest.starts_with('/');
    let mut parts = Vec::new();
    for part in rest.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
        } else {
            parts.push(part);
        }
    }

    let mut normalized = String::new();
    if !prefix.is_empty() {
        normalized.push_str(prefix);
        if leading_slash || !parts.is_empty() {
            normalized.push('/');
        }
    } else if leading_slash {
        normalized.push('/');
    }
    normalized.push_str(&parts.join("/"));
    if normalized.is_empty() {
        ".".to_string()
    } else {
        normalized
    }
}
