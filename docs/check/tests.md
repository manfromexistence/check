# Test Readiness

## Covered Rules

`test-readiness-missing`

## When This Fires

DX Check flags projects where no tests are discovered. The current discovery pass counts common shapes:

- Rust files containing `#[test]`.
- JS, TS, JSX, and TSX files with `.test.` or `.spec.` in the path.
- Python files named `test_*.py` or containing `def test_`.
- Go files ending in `_test.go`.
- C files with conventional test filenames or common C/C++ test macros.
- C++ files with conventional test filenames or GoogleTest, Catch2, or doctest-style macros.

## Why It Matters

Launch readiness needs evidence. A project with no discovered tests can still have manual checks or external verification, but DX Check reports that as weaker evidence instead of pretending it is fully measured.

## How To Fix

- Add at least one focused regression, adapter, parser, or smoke test.
- Put tests in the framework's conventional location so discovery can find them.
- Keep tests small enough to run during focused verification.
- Use launch receipts for compatibility, but prefer measured test evidence when claiming readiness.

## Verification

Re-run `dx check test` or `dx check score`. The finding is cleared when at least one supported test shape is discovered; test execution results are reported separately by adapters.
