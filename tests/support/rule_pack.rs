#![allow(dead_code)]

use std::fs;
use std::path::Path;

use dx_check_engine::registry::{RulePackLockEntry, rule_pack_signature_payload};
use ed25519_dalek::{Signer, SigningKey};

pub fn write_rule_pack(root: &Path, name: &str, rules: &str) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("small.rs"), "fn one() {}\n".repeat(6)).unwrap();
    fs::write(
        root.join(".dx").join("check").join(name),
        format!(
            r#"
rule_pack(id=local-check version=1 title=LocalCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
{rules}
)
"#
        ),
    )
    .unwrap();
}

pub fn write_cached_rule_pack(root: &Path, source_body: String) -> String {
    fs::create_dir_all(root.join(".dx").join("check").join("cache")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("small.rs"), source_body).unwrap();
    let pack = r#"
rule_pack(id=remote-check version=1 title=RemoteCheckRules kind=dx-check-rule-pack)

categories[id label weight](
structure Structure 100
)

rules[id category severity weight metric op threshold docs provenance](
cache-line-budget structure warning 9 line_count max 1 docs/check/cache.md forge/remote-check
)
"#;
    fs::write(
        root.join(".dx")
            .join("check")
            .join("cache")
            .join("remote-check.sr"),
        pack,
    )
    .unwrap();
    blake3::hash(pack.as_bytes()).to_hex().to_string()
}

pub fn write_rule_pack_lock(root: &Path, id: &str, signed: bool, hash: String) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    fs::write(
        root.join(".dx").join("check").join("rule-pack-lock.sr"),
        format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance](
{id} 1 r2://forge/check/{id} .dx/check/cache/{id}.sr {hash} {signed} forge/{id}
)
"#
        ),
    )
    .unwrap();
}

pub fn write_signed_rule_pack_lock(root: &Path, id: &str, hash: String) {
    fs::create_dir_all(root.join(".dx").join("check")).unwrap();
    let mut entry = RulePackLockEntry {
        id: id.to_string(),
        version: "1".to_string(),
        source: format!("r2://forge/check/{id}"),
        cache_path: Some(format!(".dx/check/cache/{id}.sr")),
        hash_blake3: hash,
        signed: true,
        provenance: Some(format!("forge/{id}")),
        signer: Some("forge-test-key".to_string()),
        public_key_ed25519: None,
        signature_ed25519: None,
    };
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    entry.public_key_ed25519 = Some(encode_hex(&signing_key.verifying_key().to_bytes()));
    let signature = signing_key.sign(rule_pack_signature_payload(&entry).as_bytes());
    entry.signature_ed25519 = Some(encode_hex(&signature.to_bytes()));

    fs::write(
        root.join(".dx").join("check").join("rule-pack-lock.sr"),
        format!(
            r#"
rule_pack_lock(schema=dx.check.rule_pack_lock.v1 strict=false registry=forge-r2)

locks[id version source cache hash_blake3 signed provenance signer public_key_ed25519 signature_ed25519](
{} {} {} {} "{}" {} {} {} "{}" "{}"
)
"#,
            entry.id,
            entry.version,
            entry.source,
            entry.cache_path.as_deref().unwrap_or_default(),
            entry.hash_blake3,
            entry.signed,
            entry.provenance.as_deref().unwrap_or_default(),
            entry.signer.as_deref().unwrap_or_default(),
            entry.public_key_ed25519.as_deref().unwrap_or_default(),
            entry.signature_ed25519.as_deref().unwrap_or_default(),
        ),
    )
    .unwrap();
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;

        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}
