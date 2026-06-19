# Python Black Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add configured Black format-check support to the DX Check engine adapter planner and diagnostics parser without running mutating format commands.

**Architecture:** `G:\Dx\check` owns tool adapter planning and tool-output parsing. The CLI remains the orchestration and receipt layer; this slice only changes engine adapter plans and typed diagnostics.

**Tech Stack:** Rust 2024, `dx-check-engine`, TOML project config detection, focused Cargo tests with `-j 1`.

---

### Task 1: Plan Black For Explicit Black Config

**Files:**
- Modify: `G:\Dx\check\tests\adapters.rs`
- Modify: `G:\Dx\check\src\adapters.rs`

- [ ] **Step 1: Write the failing adapter test**

Add this test to `G:\Dx\check\tests\adapters.rs`:

```rust
#[test]
fn python_black_pyproject_uses_black_format_check_without_unconfigured_ruff_format() {
    let python = tempdir().unwrap();
    fs::write(
        python.path().join("pyproject.toml"),
        "[tool.black]\nline-length = 100\n",
    )
    .unwrap();

    let plans = plan_tools(python.path(), &[DxToolTarget::Format]);

    assert!(plans.iter().any(|plan| {
        plan.id == "black-format-check"
            && plan.target == DxToolTarget::Format
            && plan.executable == "black"
            && plan.args == ["--check", "--diff", "."]
            && plan.parser == "black"
            && plan.detected_from == ["pyproject.toml"]
    }));
    assert!(
        !plans.iter().any(|plan| plan.id == "ruff-format-check"),
        "a pyproject.toml with only [tool.black] must not pretend Ruff formatting is configured"
    );
}
```

- [ ] **Step 2: Run the red test**

Run:

```powershell
cargo test -j 1 python_black_pyproject_uses_black_format_check_without_unconfigured_ruff_format --test adapters -- --nocapture
```

Expected before implementation: FAIL because `python_plans` emits `ruff-format-check` for every `pyproject.toml`.

- [ ] **Step 3: Implement minimal planner support**

In `G:\Dx\check\src\adapters.rs`, make Python config detection distinguish:

```rust
fn pyproject_has_tool(root: &Path, tool: &str) -> bool {
    let path = root.join("pyproject.toml");
    let Ok(body) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&body) else {
        return false;
    };
    value
        .get("tool")
        .and_then(|tool_table| tool_table.get(tool))
        .is_some()
}
```

Then use Black for `DxToolTarget::Format` when `[tool.black]` exists and Ruff is not explicitly configured by `[tool.ruff]`, `[tool.ruff.lint]`, `[tool.ruff.format]`, or `ruff.toml`.

- [ ] **Step 4: Run the green adapter test**

Run:

```powershell
cargo test -j 1 python_black_pyproject_uses_black_format_check_without_unconfigured_ruff_format --test adapters -- --nocapture
```

Expected after implementation: PASS.

### Task 2: Parse Black Check Output

**Files:**
- Modify: `G:\Dx\check\tests\diagnostics.rs`
- Modify: `G:\Dx\check\src\diagnostics.rs`

- [ ] **Step 1: Write the failing diagnostics test**

Add this test to `G:\Dx\check\tests\diagnostics.rs`:

```rust
#[test]
fn parses_black_check_output() {
    let plan = plan("black-format-check", "black", DxToolTarget::Format);
    let diagnostics = parse_tool_output(
        &plan,
        b"",
        b"would reformat app/main.py\nwould reformat tests/test_app.py\n2 files would be reformatted\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].id, "black-format-check:format");
    assert_eq!(diagnostics[0].file.as_deref(), Some("app/main.py"));
    assert_eq!(diagnostics[0].severity, dx_check_engine::DxSeverity::Failure);
    assert!(diagnostics[0].next_action.contains("Black"));
    assert_eq!(diagnostics[1].file.as_deref(), Some("tests/test_app.py"));
}
```

- [ ] **Step 2: Run the red diagnostics test**

Run:

```powershell
cargo test -j 1 parses_black_check_output --test diagnostics -- --nocapture
```

Expected before implementation: FAIL because parser `black` is unknown and emits a single invalid-output diagnostic.

- [ ] **Step 3: Implement Black parser**

In `G:\Dx\check\src\diagnostics.rs`, route parser `"black"` to a new `parse_black` function that reads stdout and stderr, extracts lines beginning with `would reformat `, and emits one measured failure diagnostic per file:

```rust
fn parse_black(source: &str, stdout: &[u8], stderr: &[u8]) -> Vec<DxDiagnostic> {
    combined_lossy(stdout, stderr)
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("would reformat ")
                .map(str::trim)
                .filter(|file| !file.is_empty())
                .map(|file| DxDiagnostic {
                    id: format!("{source}:format"),
                    source: source.to_string(),
                    severity: DxSeverity::Failure,
                    file: Some(file.to_string()),
                    line: None,
                    column: None,
                    message: format!("{file} would be reformatted by Black"),
                    next_action: "Run the approved Black formatter, then rerun dx check.".to_string(),
                    measurement: DxMeasurementKind::Measured,
                })
        })
        .collect()
}
```

- [ ] **Step 4: Run the green diagnostics test**

Run:

```powershell
cargo test -j 1 parses_black_check_output --test diagnostics -- --nocapture
```

Expected after implementation: PASS.

### Task 3: Focused Verification

**Files:**
- Verify: `G:\Dx\check`

- [ ] **Step 1: Run adapter and diagnostics focused suites**

Run:

```powershell
cargo test -j 1 --test adapters -- --nocapture
cargo test -j 1 --test diagnostics -- --nocapture
```

- [ ] **Step 2: Run formatting/check**

Run:

```powershell
cargo fmt --check
cargo check -j 1 --message-format short
git diff --check
```

Expected: all commands exit 0. If they do not, report the exact blocker and do not claim this slice complete.
