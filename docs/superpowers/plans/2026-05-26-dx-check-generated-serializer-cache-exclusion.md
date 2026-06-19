# DX Check Generated Serializer Cache Exclusion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep generated `.dx/serializer/*.machine` artifacts out of source-owned DX Check inventory and scoring while still flagging misplaced `.machine` files in hand-authored source paths.

**Architecture:** The exclusion belongs in `G:\Dx\check\src\inventory.rs` because inventory owns the source/config file set that drives checked paths, syntax diagnostics, and generic file-size rules. Rule evaluation keeps its existing `generated_machine` guard so source-owned `.machine` leaks outside `.dx/serializer` remain visible.

**Tech Stack:** Rust 2024, `dx-check-engine`, serializer-generated `.machine` artifacts, focused cargo tests with `-j 1`.

---

### Task 1: Add Red Inventory And Rule Tests

**Files:**
- Modify: `G:\Dx\check\src\inventory.rs`
- Modify: `G:\Dx\check\src\rules\mod.rs`

- [ ] **Step 1: Add the failing inventory test**

Add this test inside the existing `#[cfg(test)] mod tests` in `G:\Dx\check\src\inventory.rs`:

```rust
#[test]
fn scan_project_ignores_generated_serializer_machine_artifacts() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("serializer")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("generated.machine"), "cache").unwrap();
    fs::write(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-launch-report.machine"),
        "generated cache\n",
    )
    .unwrap();

    let inventory = scan_project(temp.path()).unwrap();

    assert!(
        inventory
            .files
            .iter()
            .any(|file| file.relative_path == "src/generated.machine")
    );
    assert!(
        !inventory
            .files
            .iter()
            .any(|file| file.relative_path.starts_with(".dx/serializer/")),
        "generated serializer caches should not be source-owned inventory"
    );
}
```

- [ ] **Step 2: Add the failing scoring regression**

Add this test inside `G:\Dx\check\src\rules\mod.rs`:

```rust
#[test]
fn default_rules_ignore_generated_serializer_cache_for_source_size_scoring() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".dx").join("serializer")).unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(
        temp.path()
            .join(".dx")
            .join("serializer")
            .join("check-launch-report.machine"),
        "x".repeat(130_000),
    )
    .unwrap();
    fs::write(temp.path().join("src").join("generated.machine"), "cache").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        !report.findings.iter().any(|finding| {
            finding
                .file
                .as_deref()
                .is_some_and(|file| file.starts_with(".dx/serializer/"))
        }),
        "generated serializer caches must not create source-owned findings"
    );
    assert!(
        report.findings.iter().any(|finding| {
            finding.id == "generated-machine-leak"
                && finding.file.as_deref() == Some("src/generated.machine")
        }),
        "misplaced machine artifacts outside .dx/serializer should still be flagged"
    );
}
```

- [ ] **Step 3: Run red tests**

Run:

```powershell
cargo test -j 1 --lib --message-format short generated_serializer
```

Expected: tests fail because `.dx/serializer/check-launch-report.machine` is still included in inventory and can be scored.

### Task 2: Exclude Generated Serializer Caches In Inventory

**Files:**
- Modify: `G:\Dx\check\src\inventory.rs`

- [ ] **Step 1: Add a path helper**

Add a helper near `relative_path`:

```rust
fn is_generated_serializer_artifact(root: &Path, path: &Path) -> bool {
    let relative = relative_path(root, path);
    relative == ".dx/serializer" || relative.starts_with(".dx/serializer/")
}
```

- [ ] **Step 2: Skip the generated serializer directory and files**

In `scan_project`, before pushing a directory to `stack`, skip it when `is_generated_serializer_artifact(root, &path)` is true.

Before adding a file to `inventory.files`, skip it when `is_generated_serializer_artifact(root, &path)` is true.

- [ ] **Step 3: Run green tests**

Run:

```powershell
cargo test -j 1 --lib --message-format short generated_serializer
```

Expected: both generated serializer tests pass.

### Task 3: Focused Verification

**Files:**
- Verify only.

- [ ] **Step 1: Run related rule tests**

Run:

```powershell
cargo test -j 1 --lib --message-format short default_rules_
```

Expected: all `default_rules_` tests pass, including the existing generated-machine leak test.

- [ ] **Step 2: Run formatting and compile checks**

Run:

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```

Expected: formatting passes, compile check passes, and no whitespace errors.

