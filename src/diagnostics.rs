use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity, DxToolPlan};

mod c_family;
mod parsers;

pub use parsers::parse_cargo_json_lines;

pub fn diagnostic(
    id: impl Into<String>,
    source: impl Into<String>,
    severity: DxSeverity,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> DxDiagnostic {
    DxDiagnostic {
        id: id.into(),
        source: source.into(),
        severity,
        file: None,
        line: None,
        column: None,
        message: message.into(),
        next_action: next_action.into(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub fn parse_tool_output(plan: &DxToolPlan, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    match plan.parser.as_str() {
        "cargo-json" => {
            let mut output = String::from_utf8_lossy(stdout).into_owned();
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&String::from_utf8_lossy(stderr));
            parsers::parse_cargo_json_lines(&plan.id, &output)
        }
        "rustfmt" => parsers::parse_rustfmt(&plan.id, stdout, stderr),
        "biome-json" => parsers::parse_biome_json(&plan.id, stdout, stderr),
        "ruff-json" => parsers::parse_ruff_json(&plan.id, stdout),
        "ruff-format" => parsers::parse_ruff_format(&plan.id, stdout, stderr),
        "black" => parsers::parse_black(&plan.id, stdout, stderr),
        "package-script" => parsers::parse_package_script(&plan.id, stdout, stderr),
        "pytest" => parsers::parse_pytest(&plan.id, stdout, stderr),
        "gofmt-list" => parsers::parse_gofmt_list(&plan.id, stdout),
        "go-vet" | "go-test" => parsers::parse_go_locations(&plan.id, stdout, stderr),
        "clang-format" => c_family::parse_clang_format(&plan.id, stdout, stderr),
        "clang-tidy" => c_family::parse_clang_tidy(&plan.id, stdout, stderr),
        "clangd" => c_family::parse_clangd(&plan.id, stdout, stderr),
        "cxx-compiler" | "cpp-compiler" => c_family::parse_cxx_compiler(&plan.id, stdout, stderr),
        "cppcheck-xml" => c_family::parse_cppcheck_xml(&plan.id, stdout, stderr),
        "ctest" => c_family::parse_ctest(&plan.id, stdout, stderr),
        "web-audit-json" => parsers::parse_web_audit_json(&plan.id, stdout, stderr),
        _ => parsers::parse_unknown_parser(&plan.id, &plan.parser, stdout, stderr),
    }
}
