# DX Check Engine Parser Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `G:\Dx\check` adapter diagnostics honest by covering every parser emitted by `plan_tools` and by surfacing invalid machine-readable tool output as typed diagnostics instead of silently dropping it.

**Architecture:** Keep command execution and launch receipts in `G:\Dx\cli`; keep tool-output parsing in `G:\Dx\check\src\diagnostics.rs`. The engine returns typed `DxDiagnostic` values only; the CLI remains responsible for runner receipts, JSON compatibility, and launch aggregation.

**Tech Stack:** Rust 2024, `serde_json`, existing `DxToolPlan`/`DxDiagnostic` contracts, focused `cargo test -j 1` and `cargo check -j 1`.

---

### Task 1: Add Parser Coverage Tests

**Files:**
- Modify: `G:\Dx\check\src\diagnostics.rs`
- Modify: `G:\Dx\check\src\adapters.rs`

- [ ] **Step 1: Add parser coverage test**

In `G:\Dx\check\src\diagnostics.rs`, add a unit test that creates fixture `DxToolPlan` values for every parser emitted by `plan_tools`: `rustfmt`, `cargo-json`, `package-script`, `ruff-json`, `ruff-format`, `pytest`, `gofmt-list`, `go-vet`, and `go-test`.

- [ ] **Step 2: Add red tests for missing parsers**

Add focused unit tests for:
- `rustfmt` stderr such as `Diff in src/lib.rs:1:`
- TypeScript/package-script output such as `src/app.ts(12,5): error TS2322: bad type`
- pytest output such as `tests/test_demo.py::test_demo FAILED`

- [ ] **Step 3: Verify red**

Run:

```powershell
cargo test -j 1 diagnostics -- --nocapture
```

Expected before implementation: tests fail because `rustfmt`, `package-script`, and `pytest` currently return no diagnostics.

### Task 2: Add Safe Parser Implementations

**Files:**
- Modify: `G:\Dx\check\src\diagnostics.rs`

- [ ] **Step 1: Add parser match arms**

Extend `parse_tool_output` with:
- `rustfmt` -> parse line-oriented rustfmt diff markers
- `package-script` -> parse TypeScript-style file/line/column diagnostics and generic non-empty failure lines
- `pytest` -> parse pytest failed test lines and file/line traceback snippets

- [ ] **Step 2: Keep diagnostics honest**

Do not fabricate file, line, or column when they are not present. Use `source = plan.id`, measured diagnostics, and next actions that name the adapter family.

- [ ] **Step 3: Verify green**

Run:

```powershell
cargo test -j 1 diagnostics -- --nocapture
```

Expected: parser tests pass.

### Task 3: Surface Invalid Machine Output

**Files:**
- Modify: `G:\Dx\check\src\diagnostics.rs`

- [ ] **Step 1: Add invalid JSON tests**

Add tests proving:
- Non-empty invalid `ruff-json` stdout emits one `runner-output-invalid` diagnostic.
- Non-array `ruff-json` JSON emits one `runner-output-invalid` diagnostic.
- Malformed non-empty `cargo-json` lines emit one `runner-output-invalid` diagnostic, while unrelated valid non-diagnostic cargo JSON remains ignored.

- [ ] **Step 2: Implement bounded invalid-output diagnostics**

Add a small helper that produces `DxDiagnostic` with:
- `id = "{source}:runner-output-invalid"`
- `severity = DxSeverity::Failure`
- no file/line/column
- a bounded sanitized excerpt
- next action explaining that the adapter promised machine-readable output

- [ ] **Step 3: Verify green**

Run:

```powershell
cargo test -j 1 diagnostics -- --nocapture
```

Expected: invalid machine output produces visible diagnostics without making JSON the primary rule/config source.

### Task 4: Focused Integration Verification

**Files:**
- No additional edits unless tests reveal a real issue.

- [ ] **Step 1: Run engine checks**

```powershell
cargo fmt --check
cargo test -j 1 diagnostics
cargo check -j 1 --message-format short
git diff --check
```

- [ ] **Step 2: Run CLI bridge regression**

From `G:\Dx\cli`:

```powershell
cargo test -j 1 --test dx_check --message-format short dx_check_run_captures_engine_adapter_diagnostics
```

Expected: CLI runner receipts still persist engine diagnostics, preserving launch-readiness JSON compatibility.
