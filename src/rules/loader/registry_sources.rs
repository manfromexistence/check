use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::Result;
use serializer::SerializerOutput;

use crate::model::{DxDiagnostic, DxMeasurementKind, DxRulePackSummary, DxSeverity};
use crate::registry::{
    RulePackLock, RulePackLockEntry, RulePackTrustDecision, rule_pack_lock_from_document,
    verify_cached_pack,
};
use crate::rule_pack::{has_rule_pack_marker, pack_id_from_document};

use super::artifacts::{read_source_document, rule_pack_load_failed};
use super::summaries::{rejected_locked_summary, unlisted_local_rule_pack_summary};
use super::types::{
    LockedRulePackSources, RulePackLoadOptions, RulePackSource, RulePackSourceTrust,
};

pub(super) fn locked_sources(
    root: &Path,
    check_dir: &Path,
    serializer: &SerializerOutput,
    options: RulePackLoadOptions,
    packs: &mut Vec<DxRulePackSummary>,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Result<LockedRulePackSources> {
    let Some(lock) = read_rule_pack_lock(check_dir, serializer, options.allow_writes, diagnostics)?
    else {
        return Ok(LockedRulePackSources::default());
    };
    let strict = options.strict_rule_packs || lock.strict;
    let mut sources = Vec::new();

    for entry in lock.entries {
        let Some(cache_path) = entry.cache_path.as_deref() else {
            reject_lock_entry(
                packs,
                diagnostics,
                &entry,
                None,
                "lock entry is missing a cache path",
            );
            continue;
        };
        let cache_source = match resolve_lock_cache_source(root, cache_path) {
            Ok(cache_source) => cache_source,
            Err(reason) => {
                reject_lock_entry(packs, diagnostics, &entry, None, &reason);
                continue;
            }
        };
        match verify_cached_pack(&cache_source, &entry, strict) {
            RulePackTrustDecision::Accepted { signature_status } => {
                if !entry.signed {
                    diagnostics.push(rule_pack_registry_unsigned(&cache_source, &entry));
                }
                sources.push(RulePackSource {
                    path: cache_source,
                    trust: RulePackSourceTrust::Locked {
                        entry: Box::new(entry),
                        signature_status,
                    },
                });
            }
            RulePackTrustDecision::Rejected {
                reason,
                signature_status,
            } => {
                packs.push(rejected_locked_summary(
                    &entry,
                    Some(cache_source.clone()),
                    signature_status,
                ));
                diagnostics.push(rule_pack_registry_rejected(
                    Some(&cache_source),
                    &entry,
                    &reason,
                ));
            }
        }
    }

    Ok(LockedRulePackSources {
        sources,
        strict_local_sources: strict,
        lock_present: true,
    })
}

pub(super) fn local_sources_for_lock_scope(
    paths: Vec<PathBuf>,
    strict_local_sources: bool,
    lock_present: bool,
    locked_paths: &[PathBuf],
    packs: &mut Vec<DxRulePackSummary>,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Result<Vec<RulePackSource>> {
    let mut sources = Vec::new();

    for path in paths {
        if locked_paths.iter().any(|locked_path| locked_path == &path) {
            continue;
        }
        if lock_present && path.file_name().and_then(|name| name.to_str()) == Some("dx-default.sr")
        {
            continue;
        }
        if strict_local_sources {
            match read_source_document(&path) {
                Ok(document) if has_rule_pack_marker(&document) => {
                    packs.push(unlisted_local_rule_pack_summary(&path, &document));
                    diagnostics.push(unlisted_local_rule_pack_rejected(&path, &document));
                    continue;
                }
                Ok(_) | Err(_) => {}
            }
        }
        sources.push(RulePackSource {
            path,
            trust: RulePackSourceTrust::Local,
        });
    }

    Ok(sources)
}

pub(super) fn is_registry_lock_failure(diagnostic: &DxDiagnostic) -> bool {
    diagnostic.id == "rule-pack-registry-rejected"
        || (diagnostic.id == "rule-pack-load-failed"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("rule-pack-lock.sr")))
}

fn read_rule_pack_lock(
    check_dir: &Path,
    serializer: &SerializerOutput,
    allow_writes: bool,
    diagnostics: &mut Vec<DxDiagnostic>,
) -> Result<Option<RulePackLock>> {
    let source = check_dir.join("rule-pack-lock.sr");
    if !source.exists() {
        return Ok(None);
    }

    if allow_writes {
        let _ = serializer.process_file(&source);
    }

    match read_source_document(&source)
        .and_then(|document| rule_pack_lock_from_document(&document, &source))
    {
        Ok(lock) => Ok(Some(lock)),
        Err(error) => {
            diagnostics.push(rule_pack_load_failed(&source, error));
            Ok(None)
        }
    }
}

fn reject_lock_entry(
    packs: &mut Vec<DxRulePackSummary>,
    diagnostics: &mut Vec<DxDiagnostic>,
    entry: &RulePackLockEntry,
    source: Option<&Path>,
    reason: &str,
) {
    packs.push(rejected_locked_summary(
        entry,
        source.map(Path::to_path_buf),
        None,
    ));
    diagnostics.push(rule_pack_registry_rejected(source, entry, reason));
}

fn project_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value.trim());
    if value.trim().is_empty() || path.is_absolute() {
        return None;
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn resolve_lock_cache_source(root: &Path, value: &str) -> std::result::Result<PathBuf, String> {
    let cache_path = project_relative_path(value)
        .ok_or_else(|| "lock cache path must stay project-relative".to_string())?;

    if !is_rule_pack_cache_path(&cache_path) {
        return Err(
            "lock cache path must point to a serializer .sr file inside .dx/check/cache"
                .to_string(),
        );
    }
    validate_cache_anchors(root)?;

    let cache_source = root.join(&cache_path);
    let canonical_root = fs::canonicalize(root).map_err(|_| {
        "lock cache path could not be validated because the project root could not be resolved"
            .to_string()
    })?;
    let canonical_cache_dir = fs::canonicalize(root.join(".dx").join("check").join("cache"))
        .map_err(|_| {
            "lock cache path could not be validated because .dx/check/cache could not be resolved"
                .to_string()
        })?;
    let canonical_cache_source = fs::canonicalize(&cache_source).map_err(|_| {
        "lock cache path must resolve to a readable .sr file inside .dx/check/cache".to_string()
    })?;

    if !canonical_cache_dir.starts_with(&canonical_root)
        || !canonical_cache_source.starts_with(&canonical_root)
        || !canonical_cache_source.starts_with(&canonical_cache_dir)
    {
        return Err("lock cache path must resolve inside .dx/check/cache".to_string());
    }

    Ok(canonical_cache_source)
}

fn validate_cache_anchors(root: &Path) -> std::result::Result<(), String> {
    for (label, anchor) in [
        (".dx", root.join(".dx")),
        (".dx/check", root.join(".dx").join("check")),
        (
            ".dx/check/cache",
            root.join(".dx").join("check").join("cache"),
        ),
    ] {
        if cache_anchor_is_link(&anchor) {
            return Err(format!(
                "lock cache path must use real {label} directories, not linked cache anchors"
            ));
        }
    }

    Ok(())
}

fn cache_anchor_is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata)
    })
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

fn is_rule_pack_cache_path(path: &Path) -> bool {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sr"))
    {
        return false;
    }

    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .take(3)
        .collect::<Vec<_>>();

    components.len() == 3
        && components[0].eq_ignore_ascii_case(".dx")
        && components[1].eq_ignore_ascii_case("check")
        && components[2].eq_ignore_ascii_case("cache")
}

fn unlisted_local_rule_pack_rejected(
    source: &Path,
    document: &serializer::DxDocument,
) -> DxDiagnostic {
    let id = pack_id_from_document(document, source);
    DxDiagnostic {
        id: "rule-pack-registry-rejected".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Local rule pack `{id}` was rejected: strict rule-pack lock is active, but this pack is not listed in .dx/check/rule-pack-lock.sr"
        ),
        next_action:
            "Add the pack to the Forge/R2 rule-pack lock, move it out of .dx/check, or disable strict rule-pack mode for local experimentation."
                .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn rule_pack_registry_rejected(
    source: Option<&Path>,
    entry: &RulePackLockEntry,
    reason: &str,
) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-registry-rejected".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Failure,
        file: source.map(|source| source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule pack `{}` from {} was rejected: {reason}",
            entry.id, entry.source
        ),
        next_action: "Refresh the Forge/R2 cache from a signed pack that matches the lockfile hash, id, and version."
            .to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}

fn rule_pack_registry_unsigned(source: &Path, entry: &RulePackLockEntry) -> DxDiagnostic {
    DxDiagnostic {
        id: "rule-pack-registry-unsigned".to_string(),
        source: "dx-check-rule-pack".to_string(),
        severity: DxSeverity::Warning,
        file: Some(source.display().to_string()),
        line: None,
        column: None,
        message: format!(
            "Rule pack `{}` from {} is unsigned; non-strict mode accepts it only after lockfile hash, id, and version verification.",
            entry.id, entry.source
        ),
        next_action: "Publish a signed pack before enabling strict rule-pack mode.".to_string(),
        measurement: DxMeasurementKind::Measured,
    }
}
