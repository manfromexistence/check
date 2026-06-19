# DX Check Rule Pack Identity Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make DX Check `.sr` rule-pack table identity/schema failures explicit and prevent duplicate rule IDs from double-scoring.

**Architecture:** Keep rule-pack parsing in `G:\Dx\check\src\rule_pack.rs`; the CLI remains only the receipt/orchestration bridge. The parser returns valid `DxRuleDefinition` values plus `DxDiagnostic` rows, while the loader decides whether to use source or `.machine` cache.

**Tech Stack:** Rust 2024, `dx-check-engine`, existing `serializer::DxDocument`, focused integration tests in `G:\Dx\check\tests\rule_pack_diagnostics.rs`, optional CLI bridge test in `G:\Dx\cli\tests\dx_check.rs`.

---

### Task 1: Add Red Rule-Pack Identity Tests

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`

- [x] **Step 1: Add duplicate rule ID test**

Add a `.dx/check/duplicate.sr` fixture with two `rules` rows using the same `id`.
Assert:
- one `rule-pack-duplicate-rule-id` diagnostic exists
- the diagnostic mentions the duplicate id
- only one scored finding with that id exists for the source file

- [x] **Step 2: Add missing required column test**

Add a `.dx/check/missing-column.sr` fixture whose `rules[...]` table omits `op`.
Assert:
- a rule-pack diagnostic exists
- the diagnostic mentions the missing `op` column
- the marked DX Check rule pack is not reported as `rule-pack-skipped-non-rule-source`

- [x] **Step 3: Verify red**

Run:

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
```

Expected before implementation: duplicate ID test fails because duplicate rules are accepted without diagnostics.

### Task 2: Implement Duplicate ID Diagnostics

**Files:**
- Modify: `G:\Dx\check\src\rule_pack.rs`

- [x] **Step 1: Track seen IDs during row parsing**

Inside `parse_rules_from_document`, keep a `HashSet<String>` of accepted rule IDs.

- [x] **Step 2: Emit duplicate diagnostic and skip later row**

When a parsed row id already exists, emit `DxDiagnostic`:
- `id = "rule-pack-duplicate-rule-id"`
- `source = "dx-check-rule-pack"`
- `severity = DxSeverity::Failure`
- `measurement = DxMeasurementKind::Measured`
- `file = source.display().to_string()`

Do not add the duplicate row to `rules`.

- [x] **Step 3: Verify green**

Run:

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
```

Expected: duplicate and missing-column cases pass.

### Task 3: Focused Regression Verification

**Files:**
- No additional edits unless tests expose a real gap.

- [x] **Step 1: Run engine checks**

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 rules -- --nocapture
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```

- [x] **Step 2: Run CLI launch receipt guards**

From `G:\Dx\cli`:

```powershell
cargo test -j 1 --test dx_check --message-format short local_sr_rule_pack_unknown_metric_is_visible_without_scoring
cargo test -j 1 --test dx_check_launch --message-format short dx_check_launch_
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```
