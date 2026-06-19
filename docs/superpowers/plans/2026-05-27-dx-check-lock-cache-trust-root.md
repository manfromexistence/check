# DX Check Lock Cache Trust Root Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent generated `.machine` rule-pack lock caches from overriding the human-authored `.dx/check/rule-pack-lock.sr` trust root.

**Architecture:** Keep `.machine` artifacts as generated serializer caches, but parse the lock authority from `.dx/check/rule-pack-lock.sr`. Continue generating `.dx/serializer/check-rule-pack-lock.machine` when writes are allowed, without letting it change strictness or lock entries.

**Tech Stack:** Rust 2024, `dx-check-engine`, existing `dx-serializer`, focused Cargo tests with `-j 1`.

---

### Task 1: Lock Source Authority

**Files:**
- Modify: `G:\Dx\check\src\rules\loader.rs`
- Test: `G:\Dx\check\tests\rule_pack_diagnostics.rs`

- [ ] **Step 1: Write the failing test**

Add a test that writes a strict empty `.dx/check/rule-pack-lock.sr`, writes a conflicting fresh `.dx/serializer/check-rule-pack-lock.machine` generated from a lock that points at a cached remote pack, and asserts the cached remote rule does not score.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 rule_pack_lock_machine_cache_cannot_override_source_lock --test rule_pack_diagnostics -- --nocapture`

Expected before implementation: FAIL because the current loader accepts the generated machine lock as authority.

- [ ] **Step 3: Implement source-first lock parsing**

Change `read_rule_pack_lock` so it still calls `serializer.process_file(&source)` when `allow_writes` is true, but always parses the lock from `read_source_document(&source)` rather than `read_machine_document(&paths.machine)`.

- [ ] **Step 4: Run focused regression tests**

Run:
- `cargo test -j 1 rule_pack_lock_machine_cache_cannot_override_source_lock --test rule_pack_diagnostics -- --nocapture`
- `cargo test -j 1 rule_pack_lock --test rule_pack_diagnostics -- --nocapture`

- [ ] **Step 5: Verify and commit**

Run:
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`

Commit only the plan, test, and loader change.
