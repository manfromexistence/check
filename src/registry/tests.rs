use std::fs;
use std::path::Path;

use ed25519_dalek::{Signer, SigningKey};
use serializer::{document_to_machine, llm_to_document, machine_to_document};
use tempfile::tempdir;

use crate::registry::{
    RulePackLockEntry, RulePackTrustDecision, rule_pack_lock_from_document,
    rule_pack_signature_payload, verify_cached_pack,
};

#[test]
fn rule_pack_lock_sr_round_trips_through_serializer_machine() {
    let pack = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    let hash = blake3::hash(pack.as_bytes()).to_hex().to_string();
    let lock_source = format!(
        r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=true registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
demo 1 r2://forge/check/demo .dx/check/cache/demo.sr {hash} true forge/demo
)
"#
    );
    let document = llm_to_document(&lock_source).unwrap();
    let machine = document_to_machine(&document);
    let restored = machine_to_document(&machine).unwrap();

    let lock =
        rule_pack_lock_from_document(&restored, Path::new(".dx/check/rule-pack-lock.sr")).unwrap();

    assert!(lock.strict);
    assert_eq!(lock.registry, "forge-r2");
    assert_eq!(lock.entries.len(), 1);
    assert_eq!(lock.entries[0].id, "demo");
    assert_eq!(lock.entries[0].version, "1");
    assert_eq!(
        lock.entries[0].cache_path.as_deref(),
        Some(".dx/check/cache/demo.sr")
    );
    assert_eq!(lock.entries[0].hash_blake3, hash);
    assert_eq!(lock.entries[0].provenance.as_deref(), Some("forge/demo"));
    assert!(lock.entries[0].signed);
    assert_eq!(lock.entries[0].signer, None);
    assert_eq!(lock.entries[0].public_key_ed25519, None);
    assert_eq!(lock.entries[0].signature_ed25519, None);
}

#[test]
fn rule_pack_lock_rejects_case_variant_cache_path_duplicates() {
    let hash = "a".repeat(64);
    let lock_source = format!(
        r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
demo 1 r2://forge/check/demo .dx/check/cache/demo.sr {hash} false forge/demo
demo-extra 1 r2://forge/check/demo-extra .DX/check/cache/DEMO.SR {hash} false forge/demo-extra
)
"#
    );
    let document = llm_to_document(&lock_source).unwrap();

    let error = rule_pack_lock_from_document(&document, Path::new(".dx/check/rule-pack-lock.sr"))
        .expect_err("case variants of the same cache path should be duplicate lock entries");

    assert!(error.to_string().contains("duplicates cache path"));
    assert!(error.to_string().contains(".dx/check/cache/demo.sr"));
}

#[test]
fn strict_registry_rejects_unsigned_pack_even_when_hash_matches() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let unsigned = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: false,
        provenance: Some("forge/demo".to_string()),
        signer: None,
        public_key_ed25519: None,
        signature_ed25519: None,
    };

    let decision = verify_cached_pack(&pack, &unsigned, true);
    assert!(matches!(
        decision,
        RulePackTrustDecision::Rejected {
            reason,
            signature_status: Some(signature_status),
        } if reason.contains("signed") && signature_status == "unsigned"
    ));
}

#[test]
fn strict_registry_rejects_signed_flag_without_signature_material() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let signed_without_proof = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: true,
        provenance: Some("forge/demo".to_string()),
        signer: Some("forge-test-key".to_string()),
        public_key_ed25519: None,
        signature_ed25519: None,
    };

    let decision = verify_cached_pack(&pack, &signed_without_proof, true);
    assert!(matches!(
        decision,
        RulePackTrustDecision::Rejected {
            reason,
            signature_status: Some(signature_status),
        } if reason.contains("signature") && signature_status == "missing"
    ));
}

#[test]
fn strict_registry_rejects_invalid_signature_even_when_hash_matches() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let mut locked = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: true,
        provenance: Some("forge/demo".to_string()),
        signer: Some("forge-test-key".to_string()),
        public_key_ed25519: Some("11".repeat(32)),
        signature_ed25519: Some("22".repeat(64)),
    };
    locked.signature_ed25519 = Some("00".repeat(64));

    let decision = verify_cached_pack(&pack, &locked, true);
    assert!(matches!(
        decision,
        RulePackTrustDecision::Rejected {
            reason,
            signature_status: Some(signature_status),
        } if reason.contains("signature") && signature_status != "verified"
    ));
}

#[test]
fn strict_registry_accepts_valid_trusted_signature() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let mut locked = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: true,
        provenance: Some("forge/demo".to_string()),
        signer: Some("forge-test-key".to_string()),
        public_key_ed25519: None,
        signature_ed25519: None,
    };
    sign_lock_entry(&mut locked);

    assert_eq!(
        verify_cached_pack(&pack, &locked, true),
        RulePackTrustDecision::Accepted {
            signature_status: "verified".to_string()
        }
    );
}

#[test]
fn registry_verification_rejects_locked_id_or_version_mismatch() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=other version=2 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let locked = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: false,
        provenance: Some("forge/demo".to_string()),
        signer: None,
        public_key_ed25519: None,
        signature_ed25519: None,
    };

    let decision = verify_cached_pack(&pack, &locked, false);
    assert!(matches!(
        decision,
        RulePackTrustDecision::Rejected {
            reason,
            signature_status: Some(signature_status),
        } if reason.contains("id/version") && signature_status == "unsigned"
    ));
}

#[test]
fn non_strict_registry_accepts_unsigned_hash_matched_pack_with_provenance() {
    let temp = tempdir().unwrap();
    let pack = temp.path().join("pack.sr");
    let pack_source = "rule_pack(id=demo version=1 kind=dx-check-rule-pack)";
    fs::write(&pack, pack_source).unwrap();

    let unsigned = RulePackLockEntry {
        id: "demo".to_string(),
        version: "1".to_string(),
        source: "r2://dx/demo".to_string(),
        cache_path: Some(".dx/check/cache/demo.sr".to_string()),
        hash_blake3: blake3::hash(pack_source.as_bytes()).to_hex().to_string(),
        signed: false,
        provenance: Some("forge/demo".to_string()),
        signer: None,
        public_key_ed25519: None,
        signature_ed25519: None,
    };

    assert_eq!(
        verify_cached_pack(&pack, &unsigned, false),
        RulePackTrustDecision::Accepted {
            signature_status: "unsigned".to_string()
        }
    );
}

fn sign_lock_entry(entry: &mut RulePackLockEntry) {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    entry.public_key_ed25519 = Some(encode_hex(&signing_key.verifying_key().to_bytes()));
    let signature = signing_key.sign(rule_pack_signature_payload(entry).as_bytes());
    entry.signature_ed25519 = Some(encode_hex(&signature.to_bytes()));
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;

        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}
