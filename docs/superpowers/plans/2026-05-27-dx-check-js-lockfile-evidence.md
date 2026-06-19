# DX Check JS Lockfile Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make JavaScript adapter `detected_from` evidence list only real package-manager lockfiles and report `bun.lockb` truthfully.

**Architecture:** Keep adapter execution unchanged. Tighten only package-manager detection metadata so DX reports do not claim a missing `package-lock.json` or mislabel an existing `bun.lockb`.

**Tech Stack:** Rust, `dx-check-engine`, focused adapter tests.

---

### Task 1: Truthful JS Lockfile Evidence

**Files:**
- Modify: `G:\Dx\check\tests\adapters.rs`
- Modify: `G:\Dx\check\src\adapters.rs`

- [x] **Step 1: Write the failing tests**

Add one test proving that a JS package with `package.json` and no lockfile reports `detected_from == ["package.json"]`.

Add one test proving that a package with `bun.lockb` reports `detected_from == ["package.json", "bun.lockb"]` instead of `bun.lock`.

- [x] **Step 2: Run the red tests**

Run: `cargo test -j 1 js_package_manager_detected_from --test adapters -- --nocapture`

Expected: FAIL because current adapter plans include `package-lock.json` for npm fallback and report `bun.lock` for `bun.lockb`.

- [x] **Step 3: Implement the minimal metadata fix**

Change `PackageManager.lockfile` from a required string to optional real evidence. In `package_manager(root)`, return:

- `Some("bun.lock")` when `bun.lock` exists
- `Some("bun.lockb")` when `bun.lockb` exists
- `Some("pnpm-lock.yaml")` when `pnpm-lock.yaml` exists
- `Some("yarn.lock")` when `yarn.lock` exists
- `Some("package-lock.json")` when `package-lock.json` exists
- `None` for npm fallback without a lockfile

When building a `DxToolPlan`, start `detected_from` with `package.json` and append the lockfile only when present.

- [x] **Step 4: Run the green tests**

Run: `cargo test -j 1 js_package_manager_detected_from --test adapters -- --nocapture`

Expected: PASS.

### Task 2: Verification And Cleanup

**Files:**
- No new runtime modules.

- [x] **Step 1: Run focused adapter suite**

Run: `cargo test -j 1 --test adapters -- --nocapture`

- [ ] **Step 2: Run final checks**

Run:
- `cargo test -j 1 --lib -- --nocapture`
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`

- [ ] **Step 3: Commit and clean**

Commit the focused change, report that `G:\Dx\check` has no remote if still true, and run `cargo clean` after commit if a target directory exists.
