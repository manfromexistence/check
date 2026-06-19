use crate::model::{DxDiagnostic, DxMeasurementKind, DxSeverity};

pub(super) fn combined_lossy(stdout: &[u8], stderr: &[u8]) -> String {
    let mut output = String::from_utf8_lossy(stdout).into_owned();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(&String::from_utf8_lossy(stderr));
    output
}

pub(super) fn invalid_runner_output(source: &str, reason: &str, output: &str) -> DxDiagnostic {
    let excerpt = preview(output);
    DxDiagnostic {
        id: format!("{source}:runner-output-invalid"),
        source: source.to_string(),
        severity: DxSeverity::Failure,
        file: None,
        line: None,
        column: None,
        message: format!("{reason}. Excerpt: {excerpt}"),
        next_action:
            "Fix the adapter command so it emits the promised machine-readable output, then rerun dx check."
                .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

pub(super) fn to_u32(value: u64) -> Option<u32> {
    u32::try_from(value).ok()
}

fn preview(output: &str) -> String {
    let mut preview = output
        .chars()
        .flat_map(|character| match character {
            '\r' | '\n' | '\t' => Some(' '),
            character if character.is_control() => None,
            character => Some(character),
        })
        .collect::<String>();
    const MAX_PREVIEW_CHARS: usize = 240;
    if preview.chars().count() > MAX_PREVIEW_CHARS {
        preview = preview.chars().take(MAX_PREVIEW_CHARS).collect();
        preview.push_str("...");
    }
    preview
}
