# Rust Maintainability

## Covered Rules

`rust-unwrap-maintainability`

## When This Fires

DX Check flags Rust source files that contain `.unwrap()` or `.expect(` when the default `rust_unwraps` threshold is zero.

## Why It Matters

Panics in application paths turn recoverable failures into crashes. In a DX toolchain, filesystem, process, network, and serializer work should usually return typed errors with enough context for the caller to report a useful diagnostic.

## How To Fix

- Return `Result` from fallible functions and propagate errors with context.
- Convert optional values into explicit errors when absence is expected.
- Reserve `expect` for invariants that truly cannot fail, and make the message specific.
- Keep tests free to use `unwrap` when the panic is the clearest failure signal.

## Verification

Re-run `dx check score`, then the focused Rust tests for the changed path. The finding is cleared when source files no longer contain panic shortcuts outside accepted test or invariant contexts.
