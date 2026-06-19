# DX Check Rule Pack Row Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make malformed `.sr` rule-pack rows visible as typed DX Check diagnostics instead of silently dropping them before scoring.

**Architecture:** Keep `.sr` and generated `.machine` as the rule-pack source/cache boundary in `G:\Dx\check`. Add a parsed-rule result in `rule_pack.rs` that returns valid `DxRuleDefinition` values plus row-level `DxDiagnostic` entries; keep `rules_from_document` as the compatibility helper for existing evaluators and comparisons.

**Tech Stack:** Rust 2024, existing `serializer::DxDocument` / `DxLlmValue`, `DxDiagnostic`, focused integration tests in `G:\Dx\check\tests`.

---

### Task 1: Add Red Tests For Malformed Rule Rows

**Files:**
- Create: `G:\Dx\check\tests\rule_pack_diagnostics.rs`

- [x] **Step 1: Invalid severity is diagnostic, not scored**

Create a local `.dx/check/bad-severity.sr` with `kind=dx-check-rule-pack` and one `rules` row whose severity is `critical`. Run `analyze_project`. Assert:
- diagnostic id `rule-pack-rule-invalid`
- diagnostic message contains the rule id and `severity`
- no finding/rule from that malformed row is scored

- [x] **Step 2: Invalid weight is diagnostic, not scored**

Create a local rule with `weight=too-heavy`. Assert the same diagnostic behavior and no phantom finding.

- [x] **Step 3: Missing required cell is diagnostic**

Create a `rules` row missing the `op` cell. Assert a `rule-pack-rule-invalid` diagnostic mentioning `op`.

- [x] **Step 4: Valid rows still load beside invalid rows**

Create one valid row and one invalid row in the same `.sr`. Assert the valid row can produce a finding and the invalid row emits a diagnostic.

- [x] **Step 5: Verify red**

Run:

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
```

Expected before implementation: tests fail because malformed rows are silently dropped or the source is misreported as non-rule.

### Task 2: Parse Rule Rows With Diagnostics

**Files:**
- Modify: `G:\Dx\check\src\rule_pack.rs`
- Modify: `G:\Dx\check\src\rules\loader.rs`

- [x] **Step 1: Add parsed result type**

Add:

```rust
pub struct ParsedRulePackRules {
    pub rules: Vec<DxRuleDefinition>,
    pub diagnostics: Vec<DxDiagnostic>,
}
```

- [x] **Step 2: Add diagnostic parser**

Add `parse_rules_from_document(document, source)` that parses rows explicitly. For missing columns, missing cells, invalid severity, invalid weight, invalid metric/operator string cells, and invalid threshold values, emit `DxDiagnostic` with:
- `id = "rule-pack-rule-invalid"`
- `source = "dx-check-rule-pack"`
- `severity = DxSeverity::Failure`
- `measurement = DxMeasurementKind::Measured`
- `file = source.display().to_string()`

- [x] **Step 3: Keep compatibility helper**

Make `rules_from_document(document)` call `parse_rules_from_document(document, Path::new("<memory>")).rules` or keep an internal parser that ignores diagnostics. Existing evaluator and machine/source comparison code must keep compiling.

- [x] **Step 4: Wire loader diagnostics**

In `load_rule_pack_set`, use `parse_rules_from_document` for both source and selected document. Push row diagnostics into `LoadedRulePackSet.diagnostics`; only valid parsed rules should be scored.

- [x] **Step 5: Verify green**

Run:

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
```

Expected: malformed rule rows are visible diagnostics and valid rows still score.

### Task 3: Focused Regression Verification

**Files:**
- No additional edits unless a test exposes a real issue.

- [x] **Step 1: Run rule-pack tests**

```powershell
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 rules -- --nocapture
```

- [x] **Step 2: Run engine and CLI checks**

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```

From `G:\Dx\cli`:

```powershell
cargo test -j 1 --test dx_check --message-format short dx_check_receipt_exposes_json_toml_syntax_diagnostics
cargo test -j 1 --test dx_check_launch --message-format short dx_check_launch_
```
