# DX Check Category Diagnostic Honesty Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make rule-pack category diagnostics name the category table honestly and warn when a rule references an undeclared category without breaking score bucket fallback.

**Architecture:** Keep serializer `.sr` as the source of rule-pack truth. Split parser diagnostics by table ownership: rule rows continue to emit `rule-pack-rule-invalid`, category table problems emit `rule-pack-category-invalid`, duplicate categories keep `rule-pack-duplicate-category-id`, and validation emits `rule-pack-unknown-category` warnings after parsing.

**Tech Stack:** Rust, `dx-check-engine`, serializer `.sr` rule packs, focused Rust integration tests.

---

### Task 1: Category Parser Diagnostics

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`
- Modify: `G:\Dx\check\src\rule_pack.rs`

- [ ] **Step 1: Write failing tests**

Add tests that create malformed `categories[...]` tables and assert diagnostics use `rule-pack-category-invalid`, mention `categories table` or `category row`, and do not blame the `rules table`.

- [ ] **Step 2: Run red test**

Run: `cargo test -j 1 category --test rule_pack_diagnostics -- --nocapture`
Expected: FAIL while category parse errors still use rule-table wording.

- [ ] **Step 3: Implement table-specific parser helpers**

Add category-specific invalid diagnostic helpers in `src/rule_pack.rs` and use them for category required columns, required cells, and numeric weight parsing.

- [ ] **Step 4: Run green test**

Run: `cargo test -j 1 category --test rule_pack_diagnostics -- --nocapture`
Expected: PASS for category parser diagnostics.

### Task 2: Unknown Category Validation

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`
- Modify: `G:\Dx\check\src\rules\validation.rs`
- Modify: `G:\Dx\check\src\rules\loader.rs`

- [ ] **Step 1: Write failing test**

Add a local rule pack with `categories` declaring `structure` and a rule referencing misspelled `strucutre`; assert `rule-pack-unknown-category` warning is emitted and the rule still scores through the derived bucket fallback.

- [ ] **Step 2: Run red test**

Run: `cargo test -j 1 undeclared_category --test rule_pack_diagnostics -- --nocapture`
Expected: FAIL because no unknown category diagnostic exists yet.

- [ ] **Step 3: Implement validation**

Update rule validation to accept parsed categories, warn when category declarations exist and a rule category is undeclared, and keep existing metric/operator warnings.

- [ ] **Step 4: Run green test**

Run: `cargo test -j 1 undeclared_category --test rule_pack_diagnostics -- --nocapture`
Expected: PASS and findings still include the misspelled category rule.

### Task 3: Verification And Cleanup

**Files:**
- No new source files.

- [ ] **Step 1: Run focused diagnostics tests**

Run: `cargo test -j 1 --test rule_pack_diagnostics -- --nocapture`

- [ ] **Step 2: Run final focused Rust checks**

Run:
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`

- [ ] **Step 3: Commit and clean**

Commit the focused changes, report that `G:\Dx\check` has no remote if unchanged, and run `cargo clean` after commit to reclaim target space.
