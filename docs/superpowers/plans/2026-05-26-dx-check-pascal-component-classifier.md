# DX Check Pascal Component Classifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make declarative component rules apply to common PascalCase TSX/JSX feature components, not only files under `components/` or paths containing `component`.

**Architecture:** Keep the rule pack declarative and change only the evaluator's component-shaped file classifier. Existing rules (`giant-component`, `component-boundary-leak`, `component-quality-affordance`) continue to drive behavior from `.sr`/`.machine` rule definitions.

**Tech Stack:** Rust 2024, `dx-check-engine`, serializer-backed rule packs, focused Cargo tests with `-j 1`.

---

### Task 1: Component Classifier Coverage

**Files:**
- Modify: `G:\Dx\check\src\rules\mod.rs`
- Modify: `G:\Dx\check\src\rules\evaluator.rs`

- [x] **Step 1: Write the failing test**

Add a test near the existing component-rule tests:

```rust
#[test]
fn default_rules_treat_pascal_case_tsx_files_as_component_shaped() {
    let temp = tempdir().unwrap();
    let dir = temp.path().join("src").join("features").join("chat");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("ChatPanel.tsx"),
        format!(
            "import fs from 'fs';\nexport function ChatPanel() {{ return <div />; }}\n{}",
            "const row = 1;\n".repeat(301)
        ),
    )
    .unwrap();

    let report = analyze_project(temp.path(), DxCheckEngineOptions::default()).unwrap();

    assert!(report.findings.iter().any(|finding| {
        finding.id == "giant-component"
            && finding.file.as_deref() == Some("src/features/chat/ChatPanel.tsx")
    }));
    assert!(report.findings.iter().any(|finding| {
        finding.id == "component-boundary-leak"
            && finding.file.as_deref() == Some("src/features/chat/ChatPanel.tsx")
    }));
}
```

- [x] **Step 2: Run red test**

Run:

```powershell
cargo test -j 1 default_rules_treat_pascal_case_tsx_files_as_component_shaped --lib
```

Expected: fail because `ChatPanel.tsx` is not recognized as component-shaped outside a `components/` path.

- [x] **Step 3: Implement the minimal classifier**

Extend `is_component_file` so `.tsx`/`.jsx` files with a PascalCase stem count as component-shaped. Keep lowercase utilities and route files out unless they already match existing component path rules.

- [x] **Step 4: Run green tests and focused guards**

Run:

```powershell
cargo test -j 1 default_rules_treat_pascal_case_tsx_files_as_component_shaped --lib
cargo test -j 1 default_rules_flag_component_quality_gaps_without_requiring_shadcn --lib
cargo test -j 1 default_rules_flag_components_with_server_boundary_leaks --lib
```

- [x] **Step 5: Final lightweight checks**

Run:

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```
