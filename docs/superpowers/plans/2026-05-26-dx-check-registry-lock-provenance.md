# DX Check Registry Lock And Provenance Plan

## Slice

Implement the serializer-native rule-pack registry trust core in `dx-check-engine`.
This slice does not add network publishing or download commands. It creates the
safe foundation those commands must use: `.sr` lock sources, generated
`.machine` round trips, hash verification, provenance metadata, and strict-mode
rejection of unsigned or malformed cached packs.

## Architecture

- `src/registry.rs` owns registry lock contracts and trust verification.
- `src/rules/loader.rs` remains the rule-pack loader/orchestrator for local
  `.dx/check/*.sr` and `.dx/serializer/*.machine`.
- `src/model.rs` carries summary fields that downstream CLI and serializer
  output can report without changing launch receipt scoring.
- `src/output.rs` serializes provenance/lock status in DX serializer sections.

## Contracts

- Lock authoring source: `.dx/check/rule-pack-lock.sr`
- Lock runtime cache: `.dx/serializer/check-rule-pack-lock.machine`
- Cached registry packs: serializer `.sr` files only; no executable hooks.
- Hash: BLAKE3 of the cached pack bytes.
- Strict mode: reject unsigned, malformed, hash-mismatched, or id/version
  mismatched packs.
- Non-strict mode: may accept unsigned packs only when hash and identity match.

## Tests First

- Add tests that parse a serializer `.sr` lock document and round-trip it
  through `.machine`.
- Add tests that strict mode rejects unsigned packs even when hashes match.
- Add tests that registry verification rejects locked id/version mismatches.
- Add output tests proving rule-pack summaries include provenance and lock
  status in serializer output.

## Verification

- `cargo test -j 1 registry --lib -- --nocapture`
- Focused output test if changed.
- `cargo fmt --check`
- `cargo check -j 1 --message-format short`
- `git diff --check`

## Non-Goals

- No Forge/R2 network fetch implementation in this slice.
- No new JSON rule/config format.
- No changes to launch-readiness receipt scoring.
