use std::{fs, io, path::Path};

use dx_check_engine::{DxCheckEngineOptions, DxScoreStatus, DxSeverity, analyze_project};
use serializer::{SerializerOutput, SerializerOutputConfig};
use tempfile::tempdir;

mod support;

use support::rule_pack::{write_cached_rule_pack, write_rule_pack_lock};

#[test]
fn rule_pack_lock_sr_is_registry_metadata_not_a_non_rule_source() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    fs::write(
        root.path()
            .join(".dx")
            .join("check")
            .join("rule-pack-lock.sr"),
        r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=true registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
)
"#,
    )
    .unwrap();

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "rule-pack-skipped-non-rule-source"),
        "registry lock serializer sources must not be treated as skipped scoring rule packs"
    );
    assert!(report.rule_packs.iter().any(|pack| {
        pack.id == "dx-check-default" && pack.lock_status.as_deref() == Some("built-in")
    }));
}

#[test]
fn rule_pack_lock_generates_serializer_machine_without_scoring() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".dx").join("check")).unwrap();
    write_rule_pack_lock(root.path(), "remote-check", false, "a".repeat(64));

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            allow_writes: true,
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert!(
        root.path()
            .join(".dx")
            .join("serializer")
            .join("check-rule-pack-lock.machine")
            .is_file(),
        "rule-pack-lock.sr should generate a serializer .machine artifact"
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id == "rule-pack-skipped-non-rule-source"),
        "registry lock serializer sources must not be treated as skipped scoring rule packs"
    );
}

#[test]
fn rule_pack_lock_machine_cache_cannot_override_source_lock() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let lock_path = root
        .path()
        .join(".dx")
        .join("check")
        .join("rule-pack-lock.sr");
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let serializer = SerializerOutput::with_config(
        SerializerOutputConfig::new()
            .with_output_dir(root.path().join(".dx").join("serializer"))
            .with_llm(false)
            .with_machine(true),
    );
    let machine_path = serializer
        .process_file(&lock_path)
        .expect("generate conflicting lock machine")
        .paths
        .machine;
    let conflicting_machine = fs::read(&machine_path).expect("conflicting lock machine bytes");

    fs::write(
        &lock_path,
        r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=true registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
)
"#,
    )
    .unwrap();
    fs::write(&machine_path, conflicting_machine).expect("fresh conflicting machine cache");

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            allow_writes: false,
            strict_rule_packs: false,
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "generated .machine lock caches must not override .dx/check/rule-pack-lock.sr"
    );
    assert!(
        !report
            .rule_packs
            .iter()
            .any(|pack| pack.id == "remote-check"),
        "lock entries absent from source rule-pack-lock.sr must not be accepted from a generated cache"
    );
}

#[test]
fn rule_pack_lock_rejects_duplicate_id_version_rows() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} false forge/remote-check
remote-check 1 r2://forge/check/remote-check-copy .dx/check/cache/remote-check.sr {hash} false forge/remote-check-copy
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("duplicate")
            && diagnostic.message.contains("remote-check")
            && diagnostic.message.contains("1")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "duplicate locked rows must not score cached packs"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_malformed_signed_cell() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} maybe forge/remote-check
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("signed")
            && diagnostic.message.contains("row 1")
            && diagnostic.message.contains("true")
            && diagnostic.message.contains("false")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "malformed signed cells must not be treated as unsigned accepted locks"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_duplicate_cache_paths() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} false forge/remote-check
remote-check-extra 1 r2://forge/check/remote-check-extra .dx/check/cache/remote-check.sr {hash} false forge/remote-check-extra
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("duplicate")
            && diagnostic.message.contains("cache")
            && diagnostic
                .message
                .contains(".dx/check/cache/remote-check.sr")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "duplicate lock cache paths must not score cached packs"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_duplicate_normalized_cache_paths() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} false forge/remote-check
remote-check-extra 1 r2://forge/check/remote-check-extra .dx/check/./cache/remote-check.sr {hash} false forge/remote-check-extra
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("duplicate")
            && diagnostic.message.contains("cache")
            && diagnostic
                .message
                .contains(".dx/check/cache/remote-check.sr")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "lexically different cache paths that normalize together must not score cached packs"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_duplicate_case_variant_cache_paths() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} false forge/remote-check
remote-check-extra 1 r2://forge/check/remote-check-extra .DX/check/cache/REMOTE-CHECK.SR {hash} false forge/remote-check-extra
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("duplicate")
            && diagnostic.message.contains("cache")
            && diagnostic
                .message
                .contains(".dx/check/cache/remote-check.sr")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "case variants of the same cache path must not score cached packs"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_malformed_strict_value() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=maybe registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.sr {hash} false forge/remote-check
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-load-failed"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains("rule_pack_lock.strict")
            && diagnostic.message.contains("true")
            && diagnostic.message.contains("false")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "malformed strict values must not be treated as non-strict accepted locks"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_cache_path_outside_dx_check_cache() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::copy(
        root.path()
            .join(".dx")
            .join("check")
            .join("cache")
            .join("remote-check.sr"),
        root.path().join("src").join("remote-check.sr"),
    )
    .unwrap();
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check src/remote-check.sr {hash} false forge/remote-check
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains(".dx/check/cache")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "hash-matched packs outside the registry cache must not score"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_non_serializer_cache_file_extension() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    fs::copy(
        root.path()
            .join(".dx")
            .join("check")
            .join("cache")
            .join("remote-check.sr"),
        root.path()
            .join(".dx")
            .join("check")
            .join("cache")
            .join("remote-check.txt"),
    )
    .unwrap();
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/remote-check.txt {hash} false forge/remote-check
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains(".sr")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "non-.sr registry cache paths must not score even when their content hashes match"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_cache_file_symlink_escape() {
    let root = tempdir().unwrap();
    let external = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let cache_file = root
        .path()
        .join(".dx")
        .join("check")
        .join("cache")
        .join("remote-check.sr");
    let external_pack = external.path().join("remote-check.sr");
    fs::copy(&cache_file, &external_pack).unwrap();
    fs::remove_file(&cache_file).unwrap();
    if create_file_symlink(&external_pack, &cache_file).is_err() {
        return;
    }
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic
                .message
                .contains("resolve inside .dx/check/cache")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "symlinked registry cache files must not score packs outside the project cache"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_cache_directory_link_escape() {
    let root = tempdir().unwrap();
    let external = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let cache_file = root
        .path()
        .join(".dx")
        .join("check")
        .join("cache")
        .join("remote-check.sr");
    let external_pack = external.path().join("remote-check.sr");
    fs::copy(&cache_file, &external_pack).unwrap();
    let linked_cache_dir = root
        .path()
        .join(".dx")
        .join("check")
        .join("cache")
        .join("linked");
    if create_dir_link(external.path(), &linked_cache_dir).is_err() {
        return;
    }
    write_lock_source(
        root.path(),
        &format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
remote-check 1 r2://forge/check/remote-check .dx/check/cache/linked/remote-check.sr {hash} false forge/remote-check
)
"#
        ),
    );

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic
                .message
                .contains("resolve inside .dx/check/cache")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "linked registry cache directories must not score packs outside the project cache"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_cache_root_link_to_project_source() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let cache_dir = root.path().join(".dx").join("check").join("cache");
    let cache_file = cache_dir.join("remote-check.sr");
    let source_pack = root.path().join("src").join("remote-check.sr");
    fs::copy(&cache_file, &source_pack).unwrap();
    fs::remove_file(&cache_file).unwrap();
    fs::remove_dir(&cache_dir).unwrap();
    if create_dir_link(root.path().join("src").as_path(), &cache_dir).is_err() {
        return;
    }
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains(".dx/check/cache")
            && diagnostic.message.contains("linked")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "a linked cache root must not redefine the trusted registry cache boundary"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[test]
fn rule_pack_lock_rejects_check_anchor_link_to_project_source() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let check_dir = root.path().join(".dx").join("check");
    let cache_file = check_dir.join("cache").join("remote-check.sr");
    let linked_check = root.path().join("src").join("linked-check");
    fs::create_dir_all(linked_check.join("cache")).unwrap();
    fs::copy(
        &cache_file,
        linked_check.join("cache").join("remote-check.sr"),
    )
    .unwrap();
    fs::remove_dir_all(&check_dir).unwrap();
    if create_dir_link(&linked_check, &check_dir).is_err() {
        return;
    }
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains(".dx/check")
            && diagnostic.message.contains("linked")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "a linked .dx/check anchor must not redefine the trusted registry cache boundary"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[cfg(windows)]
#[test]
fn rule_pack_lock_rejects_cache_root_windows_junction_to_project_source() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn example() {}\n".repeat(450));
    let cache_dir = root.path().join(".dx").join("check").join("cache");
    let cache_file = cache_dir.join("remote-check.sr");
    let source_pack = root.path().join("src").join("remote-check.sr");
    fs::copy(&cache_file, &source_pack).unwrap();
    fs::remove_file(&cache_file).unwrap();
    fs::remove_dir(&cache_dir).unwrap();
    if create_directory_junction(root.path().join("src").as_path(), &cache_dir).is_err() {
        return;
    }
    write_rule_pack_lock(root.path(), "remote-check", false, hash);

    let report = analyze_project(root.path(), read_only_options()).unwrap();

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.id == "rule-pack-registry-rejected"
            && diagnostic.severity == DxSeverity::Failure
            && diagnostic.message.contains(".dx/check/cache")
            && diagnostic.message.contains("linked")
    }));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "cache-line-budget"),
        "a junction cache root must not redefine the trusted registry cache boundary"
    );
    assert_eq!(report.score.status, DxScoreStatus::Blocked);
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(unix)]
fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_dir_link(target: &Path, link: &Path) -> io::Result<()> {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => Ok(()),
        Err(first_error) => {
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .status()?;
            if status.success() {
                Ok(())
            } else {
                Err(first_error)
            }
        }
    }
}

#[cfg(windows)]
fn create_directory_junction(target: &Path, link: &Path) -> io::Result<()> {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("mklink /J failed"))
    }
}

fn write_lock_source(root: &Path, source: &str) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    fs::write(
        root.join(".dx").join("check").join("rule-pack-lock.sr"),
        source,
    )
    .unwrap();
}

fn read_only_options() -> DxCheckEngineOptions {
    DxCheckEngineOptions {
        allow_writes: false,
        ..DxCheckEngineOptions::default()
    }
}
