use std::path::Path;

pub(crate) fn is_c_family_source_or_header(path: &Path) -> bool {
    is_c_family_translation_unit(path) || is_c_family_header(path)
}

pub(crate) fn is_c_family_translation_unit(path: &Path) -> bool {
    matches!(
        normalized_extension(path).as_deref(),
        Some("c" | "cc" | "cpp" | "cxx" | "cppm" | "ixx" | "cu" | "mm")
    )
}

pub(crate) fn is_c_source(path: &Path) -> bool {
    normalized_extension(path).as_deref() == Some("c")
}

pub(crate) fn is_cpp_source(path: &Path) -> bool {
    matches!(
        normalized_extension(path).as_deref(),
        Some("cc" | "cpp" | "cxx" | "cppm" | "ixx" | "cu" | "mm")
    )
}

pub(crate) fn is_c_family_header(path: &Path) -> bool {
    matches!(
        normalized_extension(path).as_deref(),
        Some("h" | "hh" | "hpp" | "hxx" | "ipp" | "tpp" | "cuh")
    )
}

pub(crate) fn is_cmake_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == "CMakeLists.txt"
        || name == "CMakePresets.json"
        || name == "CMakeUserPresets.json"
        || normalized_extension(path).as_deref() == Some("cmake")
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}
