pub(crate) fn should_skip_generated_or_dependency_dir(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.starts_with("cmake-build-") || normalized.starts_with("bazel-") {
        return true;
    }

    matches!(
        normalized.as_str(),
        ".conan"
            | ".cache"
            | ".dx"
            | ".git"
            | ".next"
            | ".turbo"
            | "_deps"
            | "build"
            | "cmakefiles"
            | "coverage"
            | "dist"
            | "external"
            | "node_modules"
            | "out"
            | "receipts"
            | "target"
            | "third_party"
            | "vcpkg_installed"
            | "vendor"
    )
}
