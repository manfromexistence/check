# DX Check Test Readiness Rule

## Goal

Make the built-in serializer `.sr` rule pack score missing tests through a declarative rule instead of leaving the `test-readiness` category empty.

## Scope

- Add a focused failing test for a source project with no discovered tests.
- Add one default `.sr` rule using a supported `test_count` metric.
- Pass `DxTestInventory` into rule evaluation before findings are scored.
- Keep the rule declarative and measured; do not run test commands for this score.

## Verification

- `cargo test -j 1 default_rules_flag_missing_test_readiness_when_no_tests --lib -- --nocapture`
- `cargo test -j 1 --lib rules -- --nocapture`
- `cargo fmt --check`
- `cargo check -j 1`
- `git diff --check`
