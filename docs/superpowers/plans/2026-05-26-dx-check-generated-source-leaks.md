# DX Check Generated Source Leak Rule Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect generated source leaks in source-owned DX projects from serializer-authored `.sr` rules without executing arbitrary code.

**Architecture:** Keep the rule declarative by adding one new safe metric, `generated_source`, to `G:\Dx\check`. The default `.sr` pack will own the rule row; validation will recognize the metric; the evaluator will inspect filenames and bounded source text markers from already-inventoried files.

**Tech Stack:** Rust 2024, existing `DxRuleDefinition`/`DxFinding` model, existing serializer `.sr` rule-pack loader, focused `cargo test -j 1`.

---

### Task 1: Add Generated Source Rule Test

**Files:**
- Modify: `G:\Dx\check\src\rules\mod.rs`

- [x] **Step 1: Write the failing test**

Add a test that creates:
- `src/schema.generated.ts`
- `src/client.gen.rs`
- `src/manual.ts` containing `@generated`
- `src/editable.ts` as normal hand-authored source

Assert that default rules emit `generated-source-leak` for the generated files and not for `editable.ts`.

- [x] **Step 2: Run the focused test red**

Run:

```powershell
cargo test -j 1 default_rules_flag_generated_source_leaks -- --nocapture
```

Expected before implementation: fail because `generated-source-leak` is not emitted.

### Task 2: Implement Declarative Metric

**Files:**
- Modify: `G:\Dx\check\src\rules\builtin.rs`
- Modify: `G:\Dx\check\src\rules\validation.rs`
- Modify: `G:\Dx\check\src\rules\evaluator.rs`
- Modify: `G:\Dx\check\src\rules\mod.rs`

- [x] **Step 1: Add the default `.sr` row**

Add:

```text
generated-source-leak structure warning 8 generated_source absent 0 docs/check/generated.md dx-default
```

- [x] **Step 2: Register the metric**

Add `generated_source` to the known metric registry.

- [x] **Step 3: Evaluate the metric safely**

For `generated_source absent`, scan existing inventoried source files and flag:
- filenames ending in `.generated.ts`, `.generated.tsx`, `.generated.js`, `.generated.jsx`, `.gen.ts`, `.gen.tsx`, `.gen.js`, `.gen.jsx`, `.gen.rs`, `.pb.go`, `.pb.ts`, `.pb.js`
- file content markers containing `@generated`, `DO NOT EDIT`, or `Code generated`

No commands are executed.

- [x] **Step 4: Run the focused test green**

Run:

```powershell
cargo test -j 1 default_rules_flag_generated_source_leaks -- --nocapture
```

Expected: test passes.

### Task 3: Verification

**Files:**
- No additional edits unless checks reveal a real issue.

- [x] **Step 1: Run focused rule tests**

```powershell
cargo test -j 1 default_rules_flag_generated_source_leaks default_rules_ignore_generated_serializer_cache_for_source_size_scoring
```

- [x] **Step 2: Run crate verification**

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```
