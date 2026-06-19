use std::fs;

use dx_check_engine::{
    DxCheckEngineOptions, DxRulePackStatus, DxScoreStatus, DxSeverity, analyze_project,
};
use tempfile::tempdir;

mod support;

use support::rule_pack::{
    write_cached_rule_pack, write_rule_pack_lock, write_signed_rule_pack_lock,
};

#[test]
fn non_strict_unsigned_locked_cache_scores_with_warning() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn one() {}\n".repeat(6));
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), non_strict_options()).unwrap();

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "a hash-matched unsigned cached pack should score in non-strict mode"
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-unsigned"
            && diagnostic.severity == DxSeverity::Warning
            && diagnostic.message.contains("remote-check")
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "remote-check"
            && pack.registry_source.as_deref() == Some("r2://forge/check/remote-check")
            && pack.provenance.as_deref() == Some("forge/remote-check")
            && pack.lock_status.as_deref() == Some("locked-unsigned")
            && pack.signed == Some(false)
            && pack.rule_count == 1
    }));
}

#[test]
fn strict_unsigned_locked_cache_is_rejected_not_scored() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), strict_options()).unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "strict mode must not score rejected registry packs"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "source-file-line-count"),
        "strict rejection must not silently fall back to built-in scoring"
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("remote-check")
            && diagnostic.message.contains("signed")
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "remote-check"
            && pack.status == DxRulePackStatus::Invalid
            && pack.lock_status.as_deref() == Some("rejected")
            && pack.signed == Some(false)
            && pack.rule_count == 0
    }));
    assert_eq!(report.score.finding_weight_total, 0);
    assert_eq!(report.score.score, 500);
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn strict_signed_locked_cache_without_signature_is_rejected_not_scored() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_rule_pack_lock(root.path(), "remote-check", true, hash);

    let report = analyze_project(root.path(), strict_options()).unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "strict mode must not score signed locks without verifiable signature proof"
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("remote-check")
            && diagnostic.message.contains("signature")
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "remote-check"
            && pack.status == DxRulePackStatus::Invalid
            && pack.lock_status.as_deref() == Some("rejected")
            && pack.signed == Some(true)
            && pack.rule_count == 0
    }));
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn strict_signed_locked_cache_with_valid_signature_scores() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_signed_rule_pack_lock(root.path(), "remote-check", hash);

    let report = analyze_project(root.path(), strict_options()).unwrap();

    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "strict mode should score a hash-matched locked pack with a valid signature proof"
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "rule-pack-registry-rejected"),
        "{:?}",
        report.diagnostics
    );
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "remote-check"
            && pack.lock_status.as_deref() == Some("locked-signed")
            && pack.signed == Some(true)
            && pack.signer.as_deref() == Some("forge-test-key")
            && pack.signature_status.as_deref() == Some("verified")
            && pack.rule_count == 1
    }));
}

#[test]
fn hash_rejected_locked_cache_blocks_builtin_fallback_even_with_non_rule_sr() {
    let root = tempdir().unwrap();
    write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_rule_pack_lock(root.path(), "remote-check", true, "a".repeat(64));
    fs::write(
        root.path().join(".dx").join("check").join("doctor.sr"),
        r#"
tool="dx doctor"
command="run"
passed=true
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.id == "rule-pack-registry-rejected"
                && diagnostic.severity == DxSeverity::Failure
                && diagnostic.message.contains("hash")
        }),
        "{:?}",
        report.diagnostics
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "hash-rejected registry packs must not score"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "source-file-line-count"),
        "a rejected registry pack must suppress built-in fallback"
    );
}

#[test]
fn strict_rule_pack_lock_blocks_unlisted_local_rule_pack() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn one() {}\n".repeat(6));
    write_signed_rule_pack_lock(root.path(), "remote-check", hash);
    let lock_path = root
        .path()
        .join(".dx")
        .join("check")
        .join("rule-pack-lock.sr");
    let strict_lock = fs::read_to_string(&lock_path)
        .unwrap()
        .replace("strict=false", "strict=true");
    fs::write(&lock_path, strict_lock).unwrap();
    fs::write(
        root.path().join(".dx").join("check").join("doctor.sr"),
        r#"
tool="dx doctor"
command="run"
passed=true
"#,
    )
    .unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("unlisted-local.sr"),
        r#"
rule_pack(id=unlisted-local version=1 title=UnlistedLocal kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
unlisted-local-line-budget structure failure 50 line_count max 1 docs/check/unlisted.md local
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "remote-check"
            && pack.lock_status.as_deref() == Some("locked-signed")
            && pack.signature_status.as_deref() == Some("verified")
    }));
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "unlisted-local"
            && pack.status == DxRulePackStatus::Invalid
            && pack.lock_status.as_deref() == Some("unlisted-local")
            && pack.rule_count == 0
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "unlisted-local-line-budget"),
        "strict lock scope must prevent unlisted local rule packs from scoring"
    );
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("unlisted-local.sr"))
            && diagnostic.message.contains("strict rule-pack lock")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-skipped-non-rule-source"
            && diagnostic
                .file
                .as_deref()
                .is_some_and(|file| file.ends_with("doctor.sr"))
    }));
}

fn read_only_options() -> DxCheckEngineOptions {
    DxCheckEngineOptions {
        allow_writes: false,
        ..DxCheckEngineOptions::default()
    }
}

fn strict_options() -> DxCheckEngineOptions {
    DxCheckEngineOptions {
        allow_writes: false,
        strict_rule_packs: true,
        ..DxCheckEngineOptions::default()
    }
}

fn non_strict_options() -> DxCheckEngineOptions {
    DxCheckEngineOptions {
        allow_writes: false,
        strict_rule_packs: false,
        ..DxCheckEngineOptions::default()
    }
}
