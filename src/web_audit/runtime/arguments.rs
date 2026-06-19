use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serializer::DxDocument;

use crate::model::DxDiagnostic;

use super::super::{column, config_diagnostic, row_diagnostic, row_label, u64_cell};
use super::runtime_cell;

const DX_JS_LIGHTHOUSE_RUNTIME_ARGS: [&str; 2] = ["js", "lighthouse"];

pub(super) fn parse_lighthouse_runtime_args(
    document: &DxDocument,
    source: &Path,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> BTreeMap<String, Vec<String>> {
    let Some(section) = document.section_by_name("web_lighthouse_runtime_args") else {
        return BTreeMap::new();
    };
    let Some(runtime_index) = column(section, "runtime_id", source, diagnostics) else {
        return BTreeMap::new();
    };
    let Some(position_index) = column(section, "position", source, diagnostics) else {
        return BTreeMap::new();
    };
    let Some(arg_index) = column(section, "arg", source, diagnostics) else {
        return BTreeMap::new();
    };

    let mut by_runtime = BTreeMap::<String, Vec<(u64, String)>>::new();
    let mut positions = BTreeSet::<(String, u64)>::new();
    let mut invalid_runtime_ids = BTreeSet::<String>::new();
    for (row_index, row) in section.rows.iter().enumerate() {
        let row_label = row_label(row_index);
        let Some(runtime_id) = runtime_cell(
            row.get(runtime_index),
            source,
            &row_label,
            "runtime_id",
            diagnostics,
        ) else {
            continue;
        };
        let Some(position) = row.get(position_index).and_then(u64_cell) else {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-arg-invalid-position",
                source,
                &row_label,
                "DX JS Lighthouse runtime arg position must be a non-negative integer",
                "Use stable zero-based positions in web_lighthouse_runtime_args.",
            ));
            invalid_runtime_ids.insert(runtime_id);
            continue;
        };
        if !positions.insert((runtime_id.clone(), position)) {
            diagnostics.push(row_diagnostic(
                "web-lighthouse-runtime-arg-duplicate-position",
                source,
                &row_label,
                format!(
                    "DX JS Lighthouse runtime `{runtime_id}` has duplicate arg position `{position}`"
                ),
                "Give every runtime arg a unique position.",
            ));
            invalid_runtime_ids.insert(runtime_id);
            continue;
        }
        let Some(arg) = runtime_cell(row.get(arg_index), source, &row_label, "arg", diagnostics)
        else {
            invalid_runtime_ids.insert(runtime_id);
            continue;
        };
        by_runtime
            .entry(runtime_id)
            .or_default()
            .push((position, arg));
    }

    by_runtime
        .into_iter()
        .filter_map(|(runtime_id, mut args)| {
            if invalid_runtime_ids.contains(&runtime_id) {
                return None;
            }
            args.sort_by_key(|(position, _)| *position);
            if !runtime_arg_positions_are_dense(&args) {
                diagnostics.push(config_diagnostic(
                    "web-lighthouse-runtime-arg-position-gap",
                    source,
                    format!(
                        "DX JS Lighthouse runtime `{runtime_id}` args must use contiguous zero-based positions"
                    ),
                    "Regenerate web_lighthouse_runtime_args with positions 0, 1, 2... and no gaps.",
                ));
                return None;
            }

            Some((
                runtime_id,
                args.into_iter().map(|(_, arg)| arg).collect::<Vec<_>>(),
            ))
        })
        .collect()
}

pub(super) fn dx_js_lighthouse_args_are_valid(args: &[String]) -> bool {
    args.iter()
        .map(String::as_str)
        .eq(DX_JS_LIGHTHOUSE_RUNTIME_ARGS)
}

fn runtime_arg_positions_are_dense(args: &[(u64, String)]) -> bool {
    args.iter()
        .enumerate()
        .all(|(expected, (position, _))| *position == expected as u64)
}

pub(super) fn format_runtime_args(args: &[String]) -> String {
    if args.is_empty() {
        "<none>".to_string()
    } else {
        args.join(" ")
    }
}
