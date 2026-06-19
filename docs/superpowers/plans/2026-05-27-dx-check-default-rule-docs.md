# DX Check Default Rule Docs

## Goal

Make every built-in DX Check rule documentation link resolve to a checked-in, maintainable doc page so diagnostics do not point users at dead paths.

## Constraints

- Keep docs concise and source-owned.
- Do not change rule semantics or scores.
- Do not add a new config format.
- Use TDD: add a failing guard test before adding docs.
- Keep verification focused and run Rust commands with `-j 1`.

## Plan

1. Add a unit test that parses `DEFAULT_RULE_PACK` and fails if any built-in rule `docs` path does not exist under the crate root.
2. Run the focused test and confirm it fails against the missing `docs/check/*.md` files.
3. Add concise documentation pages for the default DX Check rule families.
4. Re-run the focused test, then `cargo fmt --check`, `cargo check -j 1`, and `git diff --check`.
5. Commit the focused change and run `cargo clean` to reclaim target space.
