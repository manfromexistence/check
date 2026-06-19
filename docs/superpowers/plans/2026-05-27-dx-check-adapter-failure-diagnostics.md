# DX Check Adapter Failure Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent adapter runs from being marked passed when a zero-exit tool emits parsed failure diagnostics.

**Architecture:** Keep adapter execution and parser contracts unchanged. Treat parsed `DxSeverity::Failure` diagnostics as authoritative run failures, while preserving blocked executor behavior and invalid-runner-output handling.

**Tech Stack:** Rust, `dx-check-engine`, focused adapter runner tests.

---

### Task 1: Failure Diagnostics Control Adapter Status

**Files:**
- Modify: `G:\Dx\check\tests\adapter_runner.rs`
- Modify: `G:\Dx\check\src\adapters.rs`

- [x] **Step 1: Write the failing test**

Add `zero_exit_failure_diagnostics_make_tool_run_failed` in `tests/adapter_runner.rs`.

The test creates a `DxToolPlan` with:
- `id = "gofmt-check"`
- `target = DxToolTarget::Format`
- `executable = "gofmt"`
- `args = ["-l", "."]`
- `parser = "gofmt-list"`

The executor returns:
- `stdout = b"main.go\n"`
- `stderr = b""`
- `exit_code = Some(0)`

Assert:
- `result.status == DxToolRunStatus::Failed`
- `result.exit_code == Some(0)`
- one parsed diagnostic exists
- that diagnostic has `DxSeverity::Failure`
- `blocked_reason == None`

- [x] **Step 2: Run the red test**

Run: `cargo test -j 1 zero_exit_failure_diagnostics_make_tool_run_failed --test adapter_runner -- --nocapture`

Expected: FAIL because current status logic treats exit code `0` as passed unless the parser reports runner-output-invalid.

- [x] **Step 3: Implement the minimal status fix**

In `src/adapters.rs`, replace the status condition with:
- Passed only when `exit_code == Some(0)`
- no runner-output-invalid diagnostic exists
- no diagnostic has `DxSeverity::Failure`

Do not change command construction, parser behavior, executor error blocking, or stdout/stderr retention.

- [x] **Step 4: Run the green test**

Run: `cargo test -j 1 zero_exit_failure_diagnostics_make_tool_run_failed --test adapter_runner -- --nocapture`

Expected: PASS.

### Task 2: Verification And Cleanup

**Files:**
- No new runtime modules.

- [x] **Step 1: Run focused runner and Go diagnostic tests**

Run:
- `cargo test -j 1 --test adapter_runner -- --nocapture`
- `cargo test -j 1 parses_go_line_oriented_outputs --test diagnostics -- --nocapture`

- [x] **Step 2: Run final checks**

Run:
- `cargo test -j 1 --lib -- --nocapture`
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`

- [x] **Step 3: Commit and clean**

Commit the focused adapter-status change, report that `G:\Dx\check` has no remote if still true, and run `cargo clean` after the final commit if a target directory exists.
