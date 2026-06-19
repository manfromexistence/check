# DX Check Engine Score And Generated Source Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an engine-owned deterministic score summary and reduce generated-source false positives without changing CLI launch-readiness receipt scoring.

**Architecture:** `G:\Dx\check` owns this slice. The engine score is a compact 100-point summary derived from engine findings and standalone diagnostics; it is nested inside the engine report and serializer-native output. Existing `G:\Dx\cli` 500-point launch score and JSON receipts stay compatibility-owned by the CLI.

**Tech Stack:** Rust, `dx-check-engine`, existing serializer document/machine output, focused Cargo tests with `-j 1`.

---

### Task 1: Engine-Owned Score Summary

**Files:**
- Create: `G:\Dx\check\src\scoring.rs`
- Modify: `G:\Dx\check\src\model.rs`
- Modify: `G:\Dx\check\src\lib.rs`
- Test: `G:\Dx\check\src\scoring.rs`
- Test: `G:\Dx\check\src\lib.rs`

- [x] **Step 1: Write failing score unit tests**

Add tests for:
- Finding weights reduce a 100-point engine score, including info findings when their rules assign non-zero weight.
- Score clamps at zero when finding weights exceed 100.
- Failure diagnostics make status `Blocked` even if finding weight is zero.

- [x] **Step 2: Run score tests to verify they fail**

Run: `cargo test -j 1 scoring -- --nocapture`

Expected: FAIL because `scoring` module and `DxScoreSummary` do not exist yet.

- [x] **Step 3: Implement minimal model and scoring function**

Add:
- `DxScoreStatus::{Ready, Warning, Blocked}`
- `DxScoreSummary { schema_version, profile, score, max_score, status, finding_weight_total, failure_count, warning_count, info_count }`
- `summarize_score(findings, diagnostics)` that deducts finding weights from 100, clamps to 0, and classifies status from failures/warnings/score.

- [x] **Step 4: Wire `analyze_project`**

Compute `report.score` after source/syntax diagnostics are present and before finding diagnostics are mirrored into `report.diagnostics`, so finding diagnostics do not double-count.

- [x] **Step 5: Run focused score tests**

Run: `cargo test -j 1 scoring -- --nocapture`

Expected: PASS.

### Task 2: Serializer-Native Score Output

**Files:**
- Modify: `G:\Dx\check\src\output.rs`

- [x] **Step 1: Write failing output assertions**

Update existing output tests to require:
- context keys `dx_check_engine.score`, `dx_check_engine.score_status`, and `dx_check_engine.score_profile`
- a `score` section in both LLM and machine output

- [x] **Step 2: Run output test to verify it fails**

Run: `cargo test -j 1 report_to_ -- --nocapture`

Expected: FAIL because score output is not present.

- [x] **Step 3: Implement score output section**

Add a `score` section with one row:
`id schema profile score max_score status finding_weight_total failures warnings info`

- [x] **Step 4: Run focused output tests**

Run: `cargo test -j 1 report_to_ -- --nocapture`

Expected: PASS.

### Task 3: Generated-Source Marker False-Positive Guard

**Files:**
- Modify: `G:\Dx\check\src\rules\evaluator.rs`
- Test: `G:\Dx\check\src\rules\mod.rs`

- [x] **Step 1: Write failing rules test**

Add `default_rules_do_not_flag_generated_marker_documentation` that writes `docs/generated.md` containing `@generated` and `DO NOT EDIT`, plus `src/schema.generated.ts`, then asserts only the real generated source is reported.

- [x] **Step 2: Run rules test to verify it fails**

Run: `cargo test -j 1 default_rules_do_not_flag_generated_marker_documentation -- --nocapture`

Expected: FAIL because generated markers in docs are currently scanned as source leaks.

- [x] **Step 3: Restrict marker scanning to code-like source files**

Keep suffix detection for generated source filenames, but only scan file contents for generated markers when the path extension is code-like.

- [x] **Step 4: Run focused rules test**

Run: `cargo test -j 1 default_rules_do_not_flag_generated_marker_documentation -- --nocapture`

Expected: PASS.

### Task 4: Focused Verification

**Files:**
- Verify `G:\Dx\check` only unless CLI integration is touched.

- [x] **Step 1: Run focused behavior tests**

Run:

```powershell
cargo test -j 1 scoring -- --nocapture
cargo test -j 1 report_to_ -- --nocapture
cargo test -j 1 default_rules_do_not_flag_generated_marker_documentation -- --nocapture
```

- [x] **Step 2: Run crate checks**

Run:

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```

- [x] **Step 3: Report remaining gaps honestly**

Do not claim the full DX Check definition of done. Report that CLI launch scoring remains separate and that Forge/R2 lock/provenance and broader rule tuning remain follow-up work.
