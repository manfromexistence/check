# DX Check Missing Categories Warning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Warn when a serializer `.sr` rule pack has scoring rules but no `categories[id label weight]` table, while preserving derived score bucket fallback.

**Architecture:** Keep `.sr` as the only authoring format. Extend rule-pack validation, not scoring, so missing category contracts are surfaced as diagnostics without changing how findings are evaluated.

**Tech Stack:** Rust, `dx-check-engine`, serializer `.sr`, focused integration tests.

---

### Task 1: Missing Categories Diagnostic

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`
- Modify: `G:\Dx\check\src\rules\validation.rs`

- [x] **Step 1: Write the failing test**

Add an integration test that writes a marked `dx-check-rule-pack` with a `rules[...]` table and no `categories[...]` table. Assert:

- diagnostic id `rule-pack-categories-missing`
- severity `Warning`
- message mentions `categories[id label weight]`
- the rule still scores as a finding
- `report.score.buckets` still contains a derived fallback bucket for the rule category

- [x] **Step 2: Run the red test**

Run: `cargo test -j 1 missing_categories --test rule_pack_diagnostics -- --nocapture`
Expected: FAIL because no warning exists yet.

- [x] **Step 3: Implement the minimal warning**

In `src/rules/validation.rs`, emit one warning when `rules` is non-empty and `categories` is empty. Do not mark the pack invalid. Do not drop rules. Do not change scoring.

- [x] **Step 4: Run the green test**

Run: `cargo test -j 1 missing_categories --test rule_pack_diagnostics -- --nocapture`
Expected: PASS.

### Task 2: Verification And Cleanup

**Files:**
- No new source modules.

- [x] **Step 1: Run focused diagnostics tests**

Run: `cargo test -j 1 --test rule_pack_diagnostics -- --nocapture`

- [x] **Step 2: Run final checks**

Run:
- `cargo test -j 1 --lib -- --nocapture`
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`

- [x] **Step 3: Commit and clean**

Commit the focused change, report no remote if still true, and run `cargo clean` after commit if a target directory exists.
