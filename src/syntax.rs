use std::fs;

use crate::inventory::{ProjectInventory, SourceFile};
use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

pub fn syntax_diagnostics(inventory: &ProjectInventory) -> Vec<DxDiagnostic> {
    inventory
        .files
        .iter()
        .filter_map(|file| match extension(file) {
            Some("json") => json_diagnostic(file),
            Some("toml") => toml_diagnostic(file),
            Some("yaml") | Some("yml") => yaml_diagnostic(file),
            _ => None,
        })
        .collect()
}

fn json_diagnostic(file: &SourceFile) -> Option<DxDiagnostic> {
    let content = fs::read_to_string(&file.path).ok()?;
    let error = serde_json::from_str::<serde_json::Value>(&content).err()?;
    Some(DxDiagnostic {
        id: "json-syntax-error".to_string(),
        source: "dx-check-syntax".to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.relative_path.clone()),
        line: to_u32(error.line()),
        column: to_u32(error.column()),
        message: format!("JSON syntax error: {error}"),
        next_action: "Fix the JSON syntax, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn toml_diagnostic(file: &SourceFile) -> Option<DxDiagnostic> {
    let content = fs::read_to_string(&file.path).ok()?;
    let error = toml::from_str::<toml::Value>(&content).err()?;
    let (line, column) = error
        .span()
        .map(|span| line_column_for_offset(&content, span.start))
        .unwrap_or((None, None));
    Some(DxDiagnostic {
        id: "toml-syntax-error".to_string(),
        source: "dx-check-syntax".to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.relative_path.clone()),
        line,
        column,
        message: format!("TOML syntax error: {error}"),
        next_action: "Fix the TOML syntax, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn yaml_diagnostic(file: &SourceFile) -> Option<DxDiagnostic> {
    let content = fs::read_to_string(&file.path).ok()?;
    let error = serde_yaml::from_str::<serde_yaml::Value>(&content).err()?;
    let location = error.location();
    Some(DxDiagnostic {
        id: "yaml-syntax-error".to_string(),
        source: "dx-check-syntax".to_string(),
        severity: DxSeverity::Failure,
        file: Some(file.relative_path.clone()),
        line: location
            .as_ref()
            .and_then(|location| to_u32(location.line())),
        column: location
            .as_ref()
            .and_then(|location| to_u32(location.column())),
        message: format!("YAML syntax error: {error}"),
        next_action: "Fix the YAML syntax, then rerun dx check.".to_string(),
        measurement: DxMeasurementKind::Measured,
    })
}

fn extension(file: &SourceFile) -> Option<&str> {
    file.path
        .extension()
        .and_then(|extension| extension.to_str())
}

fn line_column_for_offset(content: &str, offset: usize) -> (Option<u32>, Option<u32>) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, character) in content.char_indices() {
        if index >= offset {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (to_u32(line), to_u32(column))
}

fn to_u32(value: usize) -> Option<u32> {
    u32::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use crate::inventory::scan_project;
    use crate::model::{DxMeasurementKind, DxSeverity};
    use crate::syntax::syntax_diagnostics;

    #[test]
    fn reports_invalid_json_and_toml_syntax() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("package.json"), "{ \"scripts\": [ }").unwrap();
        fs::write(temp.path().join("dx.check.toml"), "[bucket_weights\n").unwrap();
        fs::write(temp.path().join("workflow.yaml"), "jobs:\n  build: [").unwrap();
        let inventory = scan_project(temp.path()).unwrap();

        let diagnostics = syntax_diagnostics(&inventory);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "json-syntax-error"
                && diagnostic.file.as_deref() == Some("package.json")
                && diagnostic.severity == DxSeverity::Failure
                && diagnostic.measurement == DxMeasurementKind::Measured
                && diagnostic.line.is_some()
                && diagnostic.column.is_some()
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "toml-syntax-error"
                && diagnostic.file.as_deref() == Some("dx.check.toml")
                && diagnostic.severity == DxSeverity::Failure
                && diagnostic.measurement == DxMeasurementKind::Measured
                && diagnostic.line.is_some()
                && diagnostic.column.is_some()
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "yaml-syntax-error"
                && diagnostic.file.as_deref() == Some("workflow.yaml")
                && diagnostic.severity == DxSeverity::Failure
                && diagnostic.measurement == DxMeasurementKind::Measured
                && diagnostic.line.is_some()
                && diagnostic.column.is_some()
        }));
    }
}
