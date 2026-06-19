# DX Check AI Maintainable Structure Rule Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the default `ai-maintainable-project-structure` rule so it reports missing worker-orientation structure instead of rewarding good structure with a finding.

**Architecture:** Keep the rule declarative in the built-in `.sr` pack and make `project_structure` a measured project-level count. The default rule should require `project_structure min 1`, so missing orientation reports `actual=0` and `threshold=1`.

**Tech Stack:** Rust, `dx-check-engine`, serializer-backed `.sr` rule definitions, focused `cargo test -j 1`, `cargo fmt --check`, `cargo check -j 1`.

---

## File Structure

- Modify: `G:\Dx\check\src\rules\builtin.rs`
  - Owns the serializer `.sr` default rule row for `ai-maintainable-project-structure`.
- Modify: `G:\Dx\check\src\rules\evaluator.rs`
  - Owns declarative metric evaluation for `project_structure`.
- Modify: `G:\Dx\check\src\rules\loader.rs`
  - Migrates the generated default `.sr` source when it still contains the legacy inverted row.
- Test: `G:\Dx\check\src\rules\mod.rs`
  - Adds default-rule tests proving good orientation is quiet and missing orientation is reported.

## Task 1: Correct Project Structure Semantics

- [ ] **Step 1: Write the failing tests**

Add these tests after `default_rules_flag_component_quality_gaps_without_requiring_shadcn`:

```rust
#[test]
fn default_rules_do_not_flag_ai_maintainable_structure_when_orientation_exists() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("README.md"), "# Demo\n").unwrap();
    fs::write(temp.path().join("TODO.md"), "- [ ] Ship\n").unwrap();
    fs::write(temp.path().join("CHANGELOG.md"), "# Changelog\n").unwrap();
    fs::write(temp.path().join("dx"), "name = \"demo\"\n").unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "ai-maintainable-project-structure"),
        "AI-maintainable structure should not be reported when orientation files exist"
    );
}

#[test]
fn default_rules_flag_missing_ai_maintainable_project_structure() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    let finding = report
        .findings
        .iter()
        .find(|finding| finding.id == "ai-maintainable-project-structure")
        .expect("missing AI-maintainable project structure finding");
    assert_eq!(finding.file, None);
    assert_eq!(finding.actual.as_deref(), Some("0"));
    assert_eq!(finding.threshold.as_deref(), Some("1"));
    assert!(finding.message.contains("missing"));
    assert!(finding.next_action.contains("README.md"));
}
```

- [ ] **Step 2: Verify red**

Run:

```powershell
cargo test -j 1 --lib --message-format short default_rules_do_not_flag_ai_maintainable_structure_when_orientation_exists
cargo test -j 1 --lib --message-format short default_rules_flag_missing_ai_maintainable_project_structure
```

Expected before implementation: first test FAILS because the current evaluator reports the rule when orientation exists; second test FAILS because missing orientation produces no finding.

- [ ] **Step 3: Implement the minimal evaluator fix**

Change the default `.sr` row in `G:\Dx\check\src\rules\builtin.rs`:

```sr
ai-maintainable-project-structure dx-framework-health info 4 project_structure min 1 docs/check/structure.md dx-default
```

Then change the `project_structure` arm in `G:\Dx\check\src\rules\evaluator.rs` to count root orientation files and apply normal numeric rule semantics:

```rust
let orientation_count = project_orientation_file_count(inventory);
if violates_numeric_rule(orientation_count, rule) {
    findings.push(rule_finding(
        rule,
        "Project is missing README.md, TODO.md, CHANGELOG.md, or dx orientation for future DX workers",
        "Add or maintain README.md, TODO.md, CHANGELOG.md, or the extensionless dx project config so AI workers can recover project intent quickly.",
        None,
        Some(orientation_count),
    ));
}
```

- [ ] **Step 4: Verify green**

Run the same two focused tests. Expected: both PASS.

## Task 2: Final Verification

## Task 2: Count DX Worker Contract Files As Orientation

- [ ] **Step 1: Write the failing DX worker contract test**

Add this test after the missing-structure test:

```rust
#[test]
fn default_rules_treat_dx_worker_contract_files_as_ai_orientation() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("AGENTS.md"),
        "Keep source-owned intent visible for future DX workers.\n",
    )
    .unwrap();
    fs::write(temp.path().join("src").join("lib.rs"), "pub fn ok() {}\n").unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.id == "ai-maintainable-project-structure"),
        "AGENTS.md should count as AI-maintainable worker orientation"
    );
}
```

- [ ] **Step 2: Verify red**

Run:

```powershell
cargo test -j 1 --lib --message-format short default_rules_treat_dx_worker_contract_files_as_ai_orientation
```

Expected before implementation: FAIL because the evaluator only recognizes README.md, TODO.md, CHANGELOG.md, and extensionless dx.

- [ ] **Step 3: Implement the minimal orientation expansion**

Add `AGENTS.md` and `DX.md` to the root orientation file match in `G:\Dx\check\src\rules\evaluator.rs`.

- [ ] **Step 4: Verify green**

Run the same focused test. Expected: PASS.

## Task 3: Migrate Legacy Generated Default Rule Source

- [ ] **Step 1: Write the failing migration test**

Add a test that writes an existing `.dx/check/dx-default.sr` with:

```sr
ai-maintainable-project-structure dx-framework-health info 4 project_structure present 0 docs/check/structure.md dx-default
```

Run `analyze_project(..., allow_writes: true)` and assert the source now contains:

```sr
ai-maintainable-project-structure dx-framework-health info 4 project_structure min 1 docs/check/structure.md dx-default
```

Also assert the emitted finding has `actual == Some("0")` and `threshold == Some("1")`.

- [ ] **Step 2: Verify red**

Run:

```powershell
cargo test -j 1 --lib --message-format short generated_default_rule_pack_migrates_legacy_ai_structure_row
```

Expected before implementation: FAIL because the loader does not rewrite existing generated defaults.

- [ ] **Step 3: Implement the narrow migration**

In `G:\Dx\check\src\rules\loader.rs`, when writes are allowed and `.dx/check/dx-default.sr` exists, replace only the exact legacy `ai-maintainable-project-structure` row with the new `min 1` row. Do not rewrite unrelated project-local rule packs.

- [ ] **Step 4: Verify green**

Run the same focused migration test. Expected: PASS.

## Task 4: Final Verification

- [ ] **Step 1: Run focused rule tests**

```powershell
cargo test -j 1 --lib --message-format short default_rules_
```

- [ ] **Step 2: Run lightweight crate checks**

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```
