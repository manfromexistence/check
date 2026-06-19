use crate::model::{DxMeasurementKind, DxRulePackStatus, DxSeverity, DxToolTarget};

pub(super) fn severity_label(value: DxSeverity) -> String {
    match value {
        DxSeverity::Info => "info",
        DxSeverity::Warning => "warning",
        DxSeverity::Failure => "failure",
    }
    .to_string()
}

pub(super) fn measurement_label(value: DxMeasurementKind) -> String {
    match value {
        DxMeasurementKind::Measured => "measured",
        DxMeasurementKind::Imported => "imported",
        DxMeasurementKind::Estimated => "estimated",
        DxMeasurementKind::Skipped => "skipped",
    }
    .to_string()
}

pub(super) fn tool_target_label(value: DxToolTarget) -> String {
    match value {
        DxToolTarget::Lint => "lint",
        DxToolTarget::Format => "format",
        DxToolTarget::Typecheck => "typecheck",
        DxToolTarget::Test => "test",
        DxToolTarget::Audit => "audit",
    }
    .to_string()
}

pub(super) fn rule_pack_status_label(value: &DxRulePackStatus) -> String {
    match value {
        DxRulePackStatus::BuiltIn => "built-in",
        DxRulePackStatus::MachineFresh => "machine-fresh",
        DxRulePackStatus::MachineGenerated => "machine-generated",
        DxRulePackStatus::SourceOnly => "source-only",
        DxRulePackStatus::Invalid => "invalid",
    }
    .to_string()
}

pub(super) fn score_status_label(value: crate::model::DxScoreStatus) -> String {
    match value {
        crate::model::DxScoreStatus::Ready => "ready",
        crate::model::DxScoreStatus::Warning => "warning",
        crate::model::DxScoreStatus::Blocked => "blocked",
    }
    .to_string()
}
