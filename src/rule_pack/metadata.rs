use std::path::Path;

use serializer::{DxDocument, DxLlmValue};

const DX_CHECK_RULE_PACK_KIND: &str = "dx-check-rule-pack";

pub fn has_rule_pack_marker(document: &DxDocument) -> bool {
    let has_id = document
        .get_path("rule_pack.id")
        .and_then(DxLlmValue::as_str)
        .is_some_and(|id| !id.trim().is_empty());
    let has_kind = document
        .get_path("rule_pack.kind")
        .and_then(DxLlmValue::as_str)
        .is_some_and(|kind| kind == DX_CHECK_RULE_PACK_KIND);
    has_id && has_kind
}

pub fn pack_id_from_document(document: &DxDocument, source: &Path) -> String {
    document
        .get_path("rule_pack.id")
        .and_then(DxLlmValue::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("check-pack")
                .to_string()
        })
}

pub fn pack_version_from_document(document: &DxDocument) -> String {
    document
        .get_path("rule_pack.version")
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "1".to_string())
}
