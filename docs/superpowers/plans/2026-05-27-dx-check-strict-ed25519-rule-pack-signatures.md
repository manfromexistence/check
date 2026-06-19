# DX Check Strict Ed25519 Rule-Pack Signatures Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make strict Forge/R2 rule-pack locks require verifiable signature proof instead of trusting a `signed=true` boolean.

**Architecture:** `G:\Dx\check` keeps rule-pack trust inside the engine registry and loader. Lock metadata remains serializer-native `.sr` with generated `.machine` cache support; strict mode verifies an offline Ed25519 signature over a canonical lock payload before a cached pack can score.

**Tech Stack:** Rust 2024, `dx-serializer`, BLAKE3 hashes, Ed25519 verification, focused Cargo tests with `-j 1`.

---

### Task 1: Reject Boolean-Only Signed Locks In Strict Mode

**Files:**
- Modify: `G:\Dx\check\tests\rule_pack_diagnostics.rs`
- Modify: `G:\Dx\check\src\registry.rs`
- Modify: `G:\Dx\check\src\rules\loader.rs`
- Modify: `G:\Dx\check\src\model.rs`
- Modify: `G:\Dx\check\src\output.rs`
- Modify: `G:\Dx\check\src\rules\builtin.rs`
- Modify: `G:\Dx\check\Cargo.toml`

- [x] **Step 1: Write failing strict-mode regression**

Add a test that creates a cached rule pack and a lock row with `signed=true` but no `public_key_ed25519` or `signature_ed25519`. With `strict_rule_packs=true`, assert the cached pack does not score, the diagnostic says verifiable signature proof is required, and the pack summary exposes `signature_status=missing`.

- [x] **Step 2: Run red test**

Run: `cargo test -j 1 strict_registry_rejects_signed_flag_without_signature_material --lib -- --nocapture`

Observed before implementation: FAIL because the current registry accepted `signed=true` as enough proof.

- [x] **Step 3: Implement signature metadata fields**

Extend lock parsing with optional `signer`, `public_key_ed25519`, and `signature_ed25519` columns. Extend rule-pack summaries with optional `signer` and `signature_status` fields, and emit those fields in serializer output.

- [x] **Step 4: Implement strict verification**

Verify Ed25519 signatures over a canonical payload containing lock id, version, source, cache path, BLAKE3 hash, and provenance. Strict mode rejects signed rows with missing, malformed, or invalid proof.

- [x] **Step 5: Add positive signed-cache regression**

Add a test that signs the canonical payload with a deterministic test signing key, writes the public key and signature hex into the `.sr` lock, and asserts strict mode accepts the cached pack with `signature_status=verified`.

- [x] **Step 6: Verify focused tests**

Run:

```powershell
cargo test -j 1 strict_signed_locked_cache_without_signature_is_rejected_not_scored --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 strict_signed_locked_cache_with_valid_signature_scores --test rule_pack_diagnostics -- --nocapture
cargo test -j 1 --test rule_pack_diagnostics -- --nocapture
cargo fmt --check
cargo check -j 1
git diff --check
```
