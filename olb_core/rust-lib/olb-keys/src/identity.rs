//! OLB identity: the signing key, the sealing key, and the ID string users
//! copy (docs/04 §2, docs/11).
//!
//! CONTRACT FILE — signatures are fixed; `invite` and `olb_core` call them.
//!
//! An identity is **not** a Logos Chat address. Chat addresses change every
//! time Basecamp starts on testnet, so treating one as identity would make
//! every member a stranger after a restart (CLAUDE.md platform constraint 5).

use crate::Result;
use zeroize::Zeroizing;

/// Domain tag for the fingerprint, per docs/04 §2.1.
pub const FP_DOMAIN: &[u8] = b"olb-fp-v1";

/// The public half of an identity — what travels in an OLB ID and what a
/// peer stores about a member.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PublicIdentity {
    pub v: u32,
    /// Self-declared and UNVERIFIED. Every surface that shows this must
    /// label it so — a name in an ID is a claim, not a fact, and the only
    /// thing a user can actually check is the fingerprint.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orcid: Option<String>,
    /// 32 bytes, Ed25519.
    #[serde(with = "serde_bytes_vec")]
    pub sig_pk: Vec<u8>,
    /// 1184 bytes, ML-KEM-768 encapsulation key.
    #[serde(with = "serde_bytes_vec")]
    pub seal_pk: Vec<u8>,
    /// 16 bytes. Derives the private inbox topic tag (docs/04 §3).
    #[serde(with = "serde_bytes_vec")]
    pub inbox: Vec<u8>,
    pub created: u64,
    /// Ed25519 over every field above.
    #[serde(with = "serde_bytes_vec")]
    pub self_sig: Vec<u8>,
}

/// CBOR wants real byte strings, not arrays of numbers; this keeps the ID
/// at the ~1.7 kB docs/04 §2.1 predicts rather than inflating it 3x.
mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let v: serde_bytes_shim::ByteBuf = Deserialize::deserialize(d)?;
        Ok(v.0)
    }

    pub mod serde_bytes_shim {
        use serde::de::{Deserialize, Deserializer, Error, SeqAccess, Visitor};

        pub struct ByteBuf(pub Vec<u8>);

        impl<'de> Deserialize<'de> for ByteBuf {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = ByteBuf;
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str("a byte string")
                    }
                    fn visit_bytes<E: Error>(self, v: &[u8]) -> Result<ByteBuf, E> {
                        Ok(ByteBuf(v.to_vec()))
                    }
                    fn visit_byte_buf<E: Error>(self, v: Vec<u8>) -> Result<ByteBuf, E> {
                        Ok(ByteBuf(v))
                    }
                    fn visit_seq<A: SeqAccess<'de>>(
                        self,
                        mut seq: A,
                    ) -> Result<ByteBuf, A::Error> {
                        let mut out = Vec::new();
                        while let Some(b) = seq.next_element::<u8>()? {
                            out.push(b);
                        }
                        Ok(ByteBuf(out))
                    }
                }
                d.deserialize_byte_buf(V)
            }
        }
    }
}

/// The private half. Never leaves `olb_core` — the UI never sees any of it
/// (CLAUDE.md engineering rules).
pub struct SecretIdentity {
    pub signing: ed25519_dalek::SigningKey,
    /// ML-KEM-768 decapsulation key, serialised.
    pub seal_sk: Zeroizing<Vec<u8>>,
    pub inbox_secret: Zeroizing<[u8; 32]>,
    pub public: PublicIdentity,
}

/// Generate a fresh identity.
pub fn generate(name: &str, orcid: Option<&str>) -> Result<SecretIdentity> {
    let _ = (name, orcid);
    todo!("docs/04 §2 — Ed25519 + ML-KEM-768 via ml_kem::MlKem768::generate, \
           16-byte inbox tag derived from a 32-byte inbox secret with HKDF")
}

/// Re-derive the public half and self-signature after the name or ORCID
/// changes. The keys do not change, so the fingerprint does not either.
pub fn resign(secret: &mut SecretIdentity, name: &str, orcid: Option<&str>) -> Result<()> {
    let _ = (secret, name, orcid);
    todo!("docs/04 §2.1")
}

/// `olb1:<base64url(CBOR(identity))>` (docs/04 §2.1).
pub fn encode_id(public: &PublicIdentity) -> Result<String> {
    let _ = public;
    todo!("docs/04 §2.1")
}

/// Parse and verify an OLB ID.
///
/// Rejects a bad version, a bad self-signature, and wrong key lengths. A
/// parse that succeeds says only that the holder controls `sig_pk` — the
/// name and ORCID remain unverified claims.
pub fn decode_id(s: &str) -> Result<PublicIdentity> {
    let _ = s;
    todo!("docs/04 §2.1")
}

/// First 12 bytes of `SHA-256("olb-fp-v1" || sig_pk || seal_pk)`, formatted
/// as 6 groups of 4 uppercase hex: `9C4F 2B71 D0A8 · 5E3C 11F9 E21A`.
///
/// This is the only thing a user can meaningfully compare out of band, so
/// the formatting is part of the contract, not a display detail.
pub fn fingerprint(public: &PublicIdentity) -> String {
    let _ = public;
    todo!("docs/04 §2.1")
}

/// The private inbox topic this identity listens on (docs/04 §3).
pub fn inbox_topic(public: &PublicIdentity) -> String {
    let _ = public;
    todo!("docs/04 §3")
}

/// Load the identity for a profile, generating it on first run.
///
/// The secret half is sealed with the store key, so it is protected at rest
/// the same way the store is. This replaces the Phase 0 behaviour of writing
/// a raw 32-byte seed to `identity.key` at mode 0600, and must MIGRATE an
/// existing `identity.key` rather than generate a new identity — a node that
/// silently changes its author key orphans every commit it has ever made.
pub fn load_or_create(
    dir: &std::path::Path,
    store_key: &[u8; 32],
    name: &str,
    orcid: Option<&str>,
) -> Result<SecretIdentity> {
    let _ = (dir, store_key, name, orcid);
    todo!("docs/11 — and migrate an existing plaintext identity.key, do not replace it")
}
