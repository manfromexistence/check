# DX Check Strict Local Rule Pack Scope Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make strict rule-pack locks reject unlisted local DX Check rule packs while still allowing non-rule serializer `.sr` artifacts in `.dx/check`.

**Architecture:** `G:\Dx\check` keeps rule-pack trust decisions inside the engine loader. When a strict lock is active from `rule-pack-lock.sr` or `strict_rule_packs`, local `.sr` sources are inspected before scoring: non-rule artifacts remain skipped info diagnostics, but marked `kind=dx-check-rule-pack` sources not listed in the lock are rejected and do not affect findings.

**Tech Stack:** Rust 2024, `dx-check-engine`, serializer `.sr` / `.machine`, existing DX diagnostics and score model.

---

### Task 1: Strict Local Rule Pack Scope

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`
- Modify: `G:\Dx\check\src\rules\loader.rs`

- [x] **Step 1: Write the failing test**

Add a focused integration test in `G:\Dx\check\tests\rule_pack_diagnostics.rs`:

```rust
#[test]
fn strict_rule_pack_lock_blocks_unlisted_local_rule_pack() {
    let root = tempdir().unwrap();
    let hash = write_cached_rule_pack(root.path(), "fn one() {}\n".repeat(6));
    write_rule_pack_lock(root.path(), "remote-check", true, hash);
    let lock_path = root.path().join(".dx").join("check").join("rule-pack-lock.sr");
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
        root.path().join(".dx").join("check").join("unlisted-local.sr"),
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

    let report = analyze_project(
        root.path(),
        DxCheckEngineOptions {
            allow_writes: false,
            ..DxCheckEngineOptions::default()
        },
    )
    .unwrap();

    assert!(
        report
            .rule_packs
            .iter()
            .any(|pack| pack.id == "remote-check"
                && pack.lock_status.as_deref() == Some("locked-signed"))
    );
    assert!(
        report
            .rule_packs
            .iter()
            .any(|pack| pack.id == "unlisted-local"
                && pack.status == DxRulePackStatus::Invalid
                && pack.lock_status.as_deref() == Some("unlisted-local")
                && pack.rule_count == 0)
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "unlisted-local-line-budget")
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
```

- [x] **Step 2: Run red test**

Run:

```powershell
cargo test -j 1 strict_rule_pack_lock_blocks_unlisted_local_rule_pack --test rule_pack_diagnostics -- --nocapture
```

Expected: fail because `unlisted-local-line-budget` is still loaded from `.dx/check/unlisted-local.sr`.

- [x] **Step 3: Implement minimal loader gating**

Change `locked_sources` to return whether a strict lock is active. When strict lock scope is active, inspect discovered local `.sr` files before appending them as `RulePackSourceTrust::Local`:

```rust
let discovered_sources = discover_sources(&check_dir)?;
sources.extend(local_sources_for_lock_scope(
    &discovered_sources,
    lock_scope.strict_local_sources,
    &mut packs,
    &mut diagnostics,
)?);
```

For parseable marked DX Check rule packs, push an invalid summary with `registry_source=project-local`, `provenance=local`, `lock_status=unlisted-local`, `signed=None`, `rule_count=0`, and push a `rule-pack-registry-rejected` diagnostic. Return non-rule `.sr` files as local sources so existing skip diagnostics and no-machine behavior stay intact.

- [x] **Step 4: Run green and guard tests**

Run:

```powershell
cargo test -j 1 strict_rule_pack_lock_blocks_unlisted_local_rule_pack --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 hash_rejected_locked_cache_blocks_builtin_fallback_even_with_non_rule_sr --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 non_strict_unsigned_locked_cache_scores_with_warning --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 read_only_non_rule_check_sr_is_skipped_and_builtin_rules_load --lib
```

- [x] **Step 5: Final lightweight checks**

Run:

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```
