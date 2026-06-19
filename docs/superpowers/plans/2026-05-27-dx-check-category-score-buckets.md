# DX Check Category Score Buckets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `.sr` rule-pack `categories` rows visible in the engine score as typed bucket summaries.

**Architecture:** Parse category definitions alongside rules, carry them through the rule-pack loader, and let scoring produce category buckets while preserving the existing flat engine score fields for compatibility. Unknown finding categories remain visible through derived buckets so reports never hide findings.

**Tech Stack:** Rust 2024, `dx-check-engine`, existing `.sr` serializer parser, focused Cargo tests with `-j 1`.

---

### Task 1: Category Bucket Summaries

**Files:**
- Modify: `G:\Dx\check\src\model.rs`
- Modify: `G:\Dx\check\src\rule_pack.rs`
- Modify: `G:\Dx\check\src\rules\loader.rs`
- Modify: `G:\Dx\check\src\scoring.rs`
- Modify: `G:\Dx\check\src\output.rs`
- Test: `G:\Dx\check\src\lib.rs`

- [x] **Step 1: Write the failing test**

Add a test that creates a local `.sr` pack with two categories and two failing rules, then asserts `report.score.buckets` contains category ids, labels, max scores, deductions, and statuses derived from those `.sr` categories.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -j 1 analyze_project_populates_category_score_buckets_from_sr_categories --lib -- --nocapture`

Expected before implementation: FAIL because `DxScoreSummary` has no typed category buckets.

- [x] **Step 3: Parse and carry categories**

Add `DxRuleCategoryDefinition`, parse `categories[id label weight]`, and store category definitions in `LoadedRulePackSet`.

- [x] **Step 4: Score buckets**

Add `DxScoreBucketSummary` and compute buckets from category definitions plus findings. Preserve current `score`, `max_score`, `finding_weight_total`, and status behavior.

- [x] **Step 5: Serializer-native output**

Add a `score_buckets` section to LLM/machine output so category buckets are available without making JSON the primary format.

- [x] **Step 6: Verify and commit**

Run:
- `cargo test -j 1 analyze_project_populates_category_score_buckets_from_sr_categories --lib -- --nocapture`
- `cargo test -j 1 scoring --lib -- --nocapture`
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`
