//! Domain-separated hashing (docs/10 §1.4, §3.1, §3.4, §5.3).
//!
//! Every hash in OLB is prefixed with a tag naming what is being hashed. The
//! reason is narrow and important: without it, a signature over one kind of
//! message can be replayed as a signature over another whose bytes happen to
//! coincide. A commit preimage and a record preimage are both
//! `canon(<object>)`, so the tag is the only thing keeping them apart.
//!
//! Tags end in a NUL byte so no tag can be a prefix of another.

use crate::canon;
use crate::types::VcsError;
use sha2::{Digest, Sha256};

/// Commit preimage tag. The id hashes it and the author's signature covers
/// the same bytes, so a signature cannot be lifted onto a different message.
pub const COMMIT: &[u8] = b"olb.commit.v1\0";
/// Co-signers sign the 32-byte commit id under this tag, not the preimage.
pub const COMMIT_COSIGN: &[u8] = b"olb.commit.cosign.v1\0";
/// Record version content hash.
pub const RECORD: &[u8] = b"olb.record.v1\0";
/// RFC 6962 leaf tag for the anchoring tree (docs/10 §5.3).
pub const MERKLE_LEAF: &[u8] = &[0x00];
/// RFC 6962 interior node tag.
pub const MERKLE_NODE: &[u8] = &[0x01];
/// State-root leaf tag — a different tag from the anchor tree so a state
/// leaf can never be presented as an anchor leaf (docs/10 §3.4).
pub const TREE_LEAF: &[u8] = &[0x02];

/// Plain `SHA256`. Use this on a buffer that already carries its tag — a
/// `preimage()` result — so the tag is never applied twice.
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// `SHA256(tag || bytes)`, the shape every hash in the system takes.
pub fn tagged(tag: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(tag);
    h.update(bytes);
    h.finalize().into()
}

/// The exact bytes an author signs and the id hashes. Returned rather than
/// hashed immediately because the signature and the id must cover one
/// identical buffer — computing them from two separate serialisations is how
/// they drift apart.
pub fn preimage<T: serde::Serialize>(tag: &[u8], value: &T) -> Result<Vec<u8>, VcsError> {
    let body = canon::to_vec_of(value)?;
    let mut out = Vec::with_capacity(tag.len() + body.len());
    out.extend_from_slice(tag);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Lowercase hex, which is the only spelling that appears in a stored id.
pub fn hex32(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

/// Parse 64 hex characters back to 32 bytes.
pub fn unhex32(s: &str) -> Result<[u8; 32], VcsError> {
    let v = hex::decode(s).map_err(|e| VcsError::Malformed(format!("bad hex {s}: {e}")))?;
    <[u8; 32]>::try_from(v.as_slice())
        .map_err(|_| VcsError::Malformed(format!("expected 32 bytes, got {}", v.len() / 2)))
}

/// `olb:obj:<base32-nopad-lowercase>` from a version-1 record hash
/// (docs/10 §3.1).
///
/// An object's identity IS the hash of its first version, which has a useful
/// consequence: two members cannot independently create "the same" object
/// with different content, because different content gives a different hash
/// and therefore a different id. Creation can never conflict.
pub fn object_id_from_v1_hash(hash: &[u8; 32]) -> crate::types::ObjectId {
    let b32 = data_encoding::BASE32_NOPAD.encode(hash).to_lowercase();
    crate::types::ObjectId(format!("olb:obj:{b32}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tags_separate_identical_bodies() {
        let body = b"same bytes";
        assert_ne!(tagged(COMMIT, body), tagged(RECORD, body));
    }

    #[test]
    fn no_tag_is_a_prefix_of_another() {
        // The NUL terminator is what guarantees this. If one tag were a
        // prefix of another, a crafted body could make two different
        // (tag, body) pairs hash the same input.
        let tags: [&[u8]; 3] = [COMMIT, COMMIT_COSIGN, RECORD];
        for (i, a) in tags.iter().enumerate() {
            for (j, b) in tags.iter().enumerate() {
                if i != j {
                    assert!(!a.starts_with(b), "{a:?} starts with {b:?}");
                }
            }
        }
    }

    #[test]
    fn preimage_is_tag_then_canonical_body() {
        let p = preimage(COMMIT, &json!({"b": 1, "a": 2})).unwrap();
        assert!(p.starts_with(COMMIT));
        assert_eq!(&p[COMMIT.len()..], br#"{"a":2,"b":1}"#);
    }

    #[test]
    fn preimage_ignores_key_order() {
        let a = preimage(COMMIT, &json!({"x": 1, "y": 2})).unwrap();
        let b = preimage(COMMIT, &json!({"y": 2, "x": 1})).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hex_round_trips() {
        let h = tagged(RECORD, b"x");
        assert_eq!(unhex32(&hex32(&h)).unwrap(), h);
    }

    #[test]
    fn unhex_rejects_wrong_length() {
        assert!(unhex32("aabb").is_err());
    }

    #[test]
    fn object_id_is_lowercase_base32_with_no_padding() {
        let id = object_id_from_v1_hash(&[0xAB; 32]);
        let body = id.0.strip_prefix("olb:obj:").expect("prefix");
        assert!(!body.contains('='), "padding leaked in: {body}");
        assert_eq!(body, body.to_lowercase());
        // 32 bytes at 5 bits per character, rounded up.
        assert_eq!(body.len(), 52);
    }

    #[test]
    fn object_id_is_a_function_of_content_only() {
        assert_eq!(
            object_id_from_v1_hash(&[1; 32]),
            object_id_from_v1_hash(&[1; 32])
        );
        assert_ne!(
            object_id_from_v1_hash(&[1; 32]),
            object_id_from_v1_hash(&[2; 32])
        );
    }
}
