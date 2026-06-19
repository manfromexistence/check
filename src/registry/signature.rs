use ed25519_dalek::{Signature, VerifyingKey};

use super::RulePackLockEntry;

pub fn rule_pack_signature_payload(lock: &RulePackLockEntry) -> String {
    format!(
        "dx.check.rule_pack_signature.v1\nid={}\nversion={}\nsource={}\ncache={}\nhash_blake3={}\nprovenance={}\n",
        lock.id,
        lock.version,
        lock.source,
        lock.cache_path.as_deref().unwrap_or_default(),
        lock.hash_blake3.to_ascii_lowercase(),
        lock.provenance.as_deref().unwrap_or_default()
    )
}

pub(super) fn verify_signature(
    lock: &RulePackLockEntry,
    strict: bool,
) -> std::result::Result<String, (String, Option<String>)> {
    if !lock.signed {
        return Ok("unsigned".to_string());
    }
    let Some(public_key) = lock.public_key_ed25519.as_deref() else {
        if strict {
            return Err((
                "strict mode requires verifiable Ed25519 signature proof".to_string(),
                Some("missing".to_string()),
            ));
        }
        return Ok("missing".to_string());
    };
    let Some(signature) = lock.signature_ed25519.as_deref() else {
        if strict {
            return Err((
                "strict mode requires verifiable Ed25519 signature proof".to_string(),
                Some("missing".to_string()),
            ));
        }
        return Ok("missing".to_string());
    };

    let public_key = match decode_hex_exact(public_key, 32, "public_key_ed25519") {
        Ok(bytes) => bytes,
        Err(reason) => return Err((reason, Some("malformed".to_string()))),
    };
    let signature = match decode_hex_exact(signature, 64, "signature_ed25519") {
        Ok(bytes) => bytes,
        Err(reason) => return Err((reason, Some("malformed".to_string()))),
    };
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
        (
            "rule-pack signature proof has an invalid public_key_ed25519 length".to_string(),
            Some("malformed".to_string()),
        )
    })?;
    let verifying_key = VerifyingKey::from_bytes(&public_key).map_err(|_| {
        (
            "rule-pack signature proof has an invalid public_key_ed25519 value".to_string(),
            Some("malformed".to_string()),
        )
    })?;
    let signature = Signature::from_slice(&signature).map_err(|_| {
        (
            "rule-pack signature proof has an invalid signature_ed25519 value".to_string(),
            Some("malformed".to_string()),
        )
    })?;

    verifying_key
        .verify_strict(rule_pack_signature_payload(lock).as_bytes(), &signature)
        .map_err(|_| {
            (
                "rule-pack signature verification failed".to_string(),
                Some("invalid".to_string()),
            )
        })?;

    Ok("verified".to_string())
}

fn decode_hex_exact(
    value: &str,
    expected_len: usize,
    field: &str,
) -> std::result::Result<Vec<u8>, String> {
    let value = value.trim();
    if value.len() != expected_len * 2 {
        return Err(format!(
            "rule-pack signature proof has an invalid {field} length"
        ));
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(expected_len);
    for pair in bytes.chunks_exact(2) {
        let Some(high) = hex_nibble(pair[0]) else {
            return Err(format!(
                "rule-pack signature proof has malformed {field} hex"
            ));
        };
        let Some(low) = hex_nibble(pair[1]) else {
            return Err(format!(
                "rule-pack signature proof has malformed {field} hex"
            ));
        };
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
