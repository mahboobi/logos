//! OLB identity: the signing key, the sealing key, and the ID string users
//! copy (docs/04 §2, docs/11).
//!
//! CONTRACT FILE — signatures are fixed; `invite` and `olb_core` call them.
//!
//! An identity is **not** a Logos Chat address. Chat addresses change every
//! time Basecamp starts on testnet, so treating one as identity would make
//! every member a stranger after a restart (CLAUDE.md platform constraint 5).
//!
//! # One master seed, three keys — with one exception
//!
//! docs/11 §1.2 derives all three keys from a 32-byte master seed `ms`, so
//! that backup is a 32-byte secret. Two of the three are derived that way
//! here: the Ed25519 seed **is** `ms` verbatim (which is what lets a Phase 0
//! `identity.key` migrate without changing anyone's author key), and the
//! inbox secret is `HKDF(ms)`.
//!
//! The ML-KEM-768 key is **not** derived. docs/11 §1.2 wants deterministic
//! keygen from `(d‖z)` per FIPS 203, which `ml-kem` only exposes behind its
//! `deterministic` feature — and `Cargo.toml` is fixed for this change. So
//! the decapsulation key is generated from `OsRng` and stored, in full, in
//! the sealed vault. The consequences are honest and worth stating rather
//! than hiding: the vault is 2.4 kB rather than 32 bytes, and a recovery
//! code built from `ms` alone would restore the signing identity but **not**
//! the sealing key, so it could re-sign but could not open invites sealed to
//! the old `seal_pk`. Turning on the `deterministic` feature removes the
//! whole exception; until then the vault is the backup unit.

use crate::{KeyError, Result};
use ed25519_dalek::Signer;
use hkdf::Hkdf;
use ml_kem::{EncodedSizeUser, KemCore, MlKem768};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Domain tag for the fingerprint, per docs/04 §2.1.
pub const FP_DOMAIN: &[u8] = b"olb-fp-v1";
/// Domain tag for the canonical bytes `self_sig` covers (docs/11 §2).
pub const ID_DOMAIN: &[u8] = b"olb-id-v1";
/// HKDF-Extract salt for every key derived from the master seed (docs/11 §1.2).
pub const KDF_SALT: &[u8] = b"olb-kdf-v1";
/// HKDF info for the 32-byte inbox secret (docs/04 §2).
pub const INBOX_INFO: &[u8] = b"olb-inbox-v1";
/// HKDF info for the 16-byte public inbox tag derived from that secret.
pub const INBOX_TAG_INFO: &[u8] = b"olb-inbox-tag-v1";

/// The only ID version this build writes or accepts.
pub const ID_VERSION: u32 = 1;
pub const ID_PREFIX: &str = "olb1:";
pub const SIG_PK_LEN: usize = 32;
/// ML-KEM-768 encapsulation key.
pub const SEAL_PK_LEN: usize = 1184;
pub const INBOX_LEN: usize = 16;
pub const SIG_LEN: usize = 64;
/// docs/04 §3: many identities share each bucket topic, for k-anonymity.
pub const INBOX_BUCKETS: u8 = 16;
/// Display caps from docs/11 §2 step 7.
pub const NAME_MAX_CHARS: usize = 80;
pub const ORCID_MAX_CHARS: usize = 19;

/// Sealed vault file (docs/11 §1.1). Sits in the durable store directory,
/// outside the tree `lgs basecamp launch` scrubs.
pub const VAULT_FILE: &str = "identity.olbk";
/// The Phase 0 file this replaces: 32 raw bytes at mode 0600.
pub const LEGACY_FILE: &str = "identity.key";
/// AAD for the vault, so a sealed vault cannot be pasted into any other
/// sealed file in the store and still open.
pub const VAULT_AAD: &str = "olb.identity.vault.v1";

/// One message for every parse failure, per docs/11 §2: a verifier that says
/// *which* check failed hands an attacker a tuning oracle.
const INVALID: &str = "invalid Open Labbook ID";

/// The public half of an identity — what travels in an OLB ID and what a
/// peer stores about a member.
///
/// `deny_unknown_fields` implements docs/11 §2 step 1's "CBOR parses as a map
/// with exactly these keys": without it, an attacker can pad an ID with extra
/// map entries that no signature covers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

type Dk = <MlKem768 as KemCore>::DecapsulationKey;

/// Generate a fresh identity.
pub fn generate(name: &str, orcid: Option<&str>) -> Result<SecretIdentity> {
    let mut ms = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(ms.as_mut());
    let (dk, _ek) = MlKem768::generate(&mut rand::rngs::OsRng);
    let seal_sk = Zeroizing::new(dk.as_bytes().to_vec());
    assemble(&ms, seal_sk, name, orcid, now_unix())
}

/// Re-derive the public half and self-signature after the name or ORCID
/// changes. The keys do not change, so the fingerprint does not either.
pub fn resign(secret: &mut SecretIdentity, name: &str, orcid: Option<&str>) -> Result<()> {
    let name = clean_name(name)?;
    let orcid = clean_orcid(orcid)?;
    let mut public = PublicIdentity {
        v: ID_VERSION,
        name,
        orcid,
        sig_pk: secret.public.sig_pk.clone(),
        seal_pk: secret.public.seal_pk.clone(),
        inbox: secret.public.inbox.clone(),
        created: secret.public.created,
        self_sig: Vec::new(),
    };
    public.self_sig = secret.signing.sign(&canonical_bytes(&public)?).to_bytes().to_vec();
    // The fingerprint covers only the two keys, so a rename must not move it.
    debug_assert_eq!(fingerprint(&public), fingerprint(&secret.public));
    secret.public = public;
    Ok(())
}

/// `olb1:<base64url(CBOR(identity))>` (docs/04 §2.1).
pub fn encode_id(public: &PublicIdentity) -> Result<String> {
    let mut cbor = Vec::new();
    ciborium::into_writer(public, &mut cbor)
        .map_err(|e| KeyError::Malformed(format!("cannot encode identity: {e}")))?;
    Ok(format!(
        "{ID_PREFIX}{}",
        data_encoding::BASE64URL_NOPAD.encode(&cbor)
    ))
}

/// Parse and verify an OLB ID.
///
/// Rejects a bad version, a bad self-signature, and wrong key lengths. A
/// parse that succeeds says only that the holder controls `sig_pk` — the
/// name and ORCID remain unverified claims.
pub fn decode_id(s: &str) -> Result<PublicIdentity> {
    let invalid = || KeyError::Malformed(INVALID.into());

    let body = s.trim().strip_prefix(ID_PREFIX).ok_or_else(invalid)?;
    let cbor = data_encoding::BASE64URL_NOPAD
        .decode(body.as_bytes())
        .map_err(|_| invalid())?;
    let public: PublicIdentity = ciborium::from_reader(&cbor[..]).map_err(|_| invalid())?;

    if public.v != ID_VERSION
        || public.sig_pk.len() != SIG_PK_LEN
        || public.seal_pk.len() != SEAL_PK_LEN
        || public.inbox.len() != INBOX_LEN
        || public.self_sig.len() != SIG_LEN
    {
        return Err(invalid());
    }

    // An Ed25519 key that decompresses but is small-order would let anyone
    // forge a signature under it, so it is refused here rather than relied
    // on `verify_strict` alone.
    let vk = verifying_key(&public.sig_pk).map_err(|_| invalid())?;
    if vk.is_weak() {
        return Err(invalid());
    }

    // ML-KEM-768 encapsulation-key check: 1184 bytes that re-encode to
    // themselves. A non-canonical encoding would mean two distinct ID
    // strings for one key, and therefore two fingerprints for one member.
    let ek = encapsulation_key(&public.seal_pk).map_err(|_| invalid())?;
    if ek.as_bytes().as_slice() != public.seal_pk.as_slice() {
        return Err(invalid());
    }

    // docs/11 §2 step 5. A `created` in the future is how a forged ID claims
    // to be newer than the one it is replacing.
    let now = now_unix();
    if public.created > now.saturating_add(86_400) || public.created < EPOCH_2026 {
        return Err(invalid());
    }

    // Untrusted display strings. Refused rather than silently sanitised:
    // sanitising after the signature check would leave a struct that no
    // longer re-encodes to the ID that was verified, and callers compare
    // re-encoded IDs. `sanitise_display` is the sanitiser for anything that
    // reaches a UI.
    if !display_is_clean(&public.name, NAME_MAX_CHARS) {
        return Err(invalid());
    }
    match public.orcid.as_deref() {
        None => {}
        Some(o) if display_is_clean(o, ORCID_MAX_CHARS) && orcid_is_well_formed(o) => {}
        Some(_) => return Err(invalid()),
    }

    // `verify_strict` rather than `verify`: it rejects a small-order `R`
    // as well as a small-order key, which is the strict API docs/11 §2 asks
    // for by name.
    let sig = signature(&public.self_sig).map_err(|_| invalid())?;
    vk.verify_strict(&canonical_bytes(&public)?, &sig)
        .map_err(|_| invalid())?;

    Ok(public)
}

/// First 12 bytes of `SHA-256("olb-fp-v1" || sig_pk || seal_pk)`, formatted
/// as 6 groups of 4 uppercase hex: `9C4F 2B71 D0A8 · 5E3C 11F9 E21A`.
///
/// This is the only thing a user can meaningfully compare out of band, so
/// the formatting is part of the contract, not a display detail.
pub fn fingerprint(public: &PublicIdentity) -> String {
    let hex = fingerprint_hex(public);
    let g: Vec<&str> = (0..6).map(|i| &hex[i * 4..i * 4 + 4]).collect();
    format!(
        "{} {} {} · {} {} {}",
        g[0], g[1], g[2], g[3], g[4], g[5]
    )
}

/// The same 12 bytes as 24 uppercase hex characters, no separators — the
/// form that goes in records and comparisons (docs/11 §1.3 `pub.fp`).
pub fn fingerprint_hex(public: &PublicIdentity) -> String {
    let mut h = Sha256::new();
    h.update(FP_DOMAIN);
    h.update(&public.sig_pk);
    h.update(&public.seal_pk);
    hex::encode_upper(&h.finalize()[..12])
}

/// The private inbox topic this identity listens on (docs/04 §3).
pub fn inbox_topic(public: &PublicIdentity) -> String {
    // A missing or short tag cannot happen for an identity that came from
    // `decode_id` or `generate`; bucket 0 is the safe answer for a
    // hand-built one, and is better than panicking inside a Logos callback.
    let first = public.inbox.first().copied().unwrap_or(0);
    format!("/olb/1/inbox-{}/proto", first % INBOX_BUCKETS)
}

/// Sanitise an untrusted display string for a UI (docs/11 §2 step 7).
///
/// Control characters and bidi overrides are removed — a right-to-left
/// override in a member name can make `alice` render as the lead's name in
/// QML — and the result is capped. It is NOT a validator: it returns
/// something printable for anything.
pub fn sanitise_display(s: &str, max_chars: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() && !is_bidi_control(*c))
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
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
    let vault_path = dir.join(VAULT_FILE);
    let legacy_path = dir.join(LEGACY_FILE);

    // 1. A vault exists: it wins over everything, including the arguments.
    if let Some(plain) = crate::vault::read_sealed(&vault_path, store_key, VAULT_AAD)? {
        let mut me = open_vault(&plain)?;
        // A rename is persisted, but an empty name never overwrites one.
        let wanted_name = if name.trim().is_empty() {
            me.public.name.clone()
        } else {
            clean_name(name)?
        };
        let wanted_orcid = match orcid {
            None => me.public.orcid.clone(),
            Some(o) => clean_orcid(Some(o))?,
        };
        if wanted_name != me.public.name || wanted_orcid != me.public.orcid {
            resign(&mut me, &wanted_name, wanted_orcid.as_deref())?;
            save(dir, store_key, &me)?;
        }
        return Ok(me);
    }

    // 2. Phase 0's plaintext seed: ADOPT it as the master seed. The Ed25519
    //    seed is `ms` verbatim, so `sig_pk` — and therefore the author of
    //    every commit already in the log — is unchanged. The fingerprint
    //    does change, because there was no sealing key to preserve and the
    //    fingerprint covers both keys (docs/11 §2).
    let me = match std::fs::read(&legacy_path) {
        Ok(raw) => {
            let ms: [u8; 32] = raw.as_slice().try_into().map_err(|_| {
                // Do NOT generate a replacement. A wrong-length file is
                // something we did not write, and silently re-keying is
                // exactly the orphaning this function exists to prevent.
                KeyError::Malformed(format!(
                    "{} is {} bytes, expected 32; refusing to replace it — \
                     move it aside to investigate",
                    legacy_path.display(),
                    raw.len()
                ))
            })?;
            let ms = Zeroizing::new(ms);
            let (dk, _ek) = MlKem768::generate(&mut rand::rngs::OsRng);
            assemble(
                &ms,
                Zeroizing::new(dk.as_bytes().to_vec()),
                name,
                orcid,
                now_unix(),
            )?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => generate(name, orcid)?,
        Err(e) => return Err(KeyError::Io(e.to_string())),
    };

    save(dir, store_key, &me)?;

    // Only now, with the vault fsynced and renamed (docs/11 §1.6 step 2).
    if legacy_path.exists() {
        std::fs::remove_file(&legacy_path).map_err(|e| KeyError::Io(e.to_string()))?;
    }
    Ok(me)
}

/// Write the sealed vault. Public because `resign` leaves it to the caller
/// to decide when a rename is durable.
pub fn save(dir: &std::path::Path, store_key: &[u8; 32], me: &SecretIdentity) -> Result<()> {
    let doc = serde_json::json!({
        "v": 1,
        // The master seed. Everything except the sealing key derives from it.
        "ms": data_encoding::BASE64URL_NOPAD.encode(&me.signing.to_bytes()),
        "seal_sk": data_encoding::BASE64URL_NOPAD.encode(&me.seal_sk),
        "name": me.public.name,
        "orcid": me.public.orcid,
        "created": me.public.created,
    });
    let bytes = serde_json::to_vec(&doc).map_err(|e| KeyError::Malformed(e.to_string()))?;
    let path = dir.join(VAULT_FILE);
    crate::vault::write_sealed(&path, store_key, VAULT_AAD, &bytes)?;
    restrict(&path)
}

fn open_vault(plain: &[u8]) -> Result<SecretIdentity> {
    let doc: serde_json::Value =
        serde_json::from_slice(plain).map_err(|e| KeyError::Malformed(format!("vault: {e}")))?;
    let b64 = |k: &str| -> Result<Vec<u8>> {
        let s = doc
            .get(k)
            .and_then(|v| v.as_str())
            .ok_or_else(|| KeyError::Malformed(format!("vault: missing {k}")))?;
        data_encoding::BASE64URL_NOPAD
            .decode(s.as_bytes())
            .map_err(|e| KeyError::Malformed(format!("vault: {k} is not base64url: {e}")))
    };
    let ms: [u8; 32] = b64("ms")?
        .as_slice()
        .try_into()
        .map_err(|_| KeyError::Malformed("vault: ms is not 32 bytes".into()))?;
    let ms = Zeroizing::new(ms);
    let seal_sk = Zeroizing::new(b64("seal_sk")?);
    let name = doc.get("name").and_then(|v| v.as_str()).unwrap_or_default();
    let orcid = doc.get("orcid").and_then(|v| v.as_str());
    let created = doc
        .get("created")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| KeyError::Malformed("vault: missing created".into()))?;
    assemble(&ms, seal_sk, name, orcid, created)
}

/// Build the whole identity from the parts that are actually stored.
///
/// Everything else — `seal_pk`, the inbox secret and tag, the self-signature
/// — is recomputed on every load rather than cached, so there is no stored
/// copy that can disagree with the keys it describes.
fn assemble(
    ms: &Zeroizing<[u8; 32]>,
    seal_sk: Zeroizing<Vec<u8>>,
    name: &str,
    orcid: Option<&str>,
    created: u64,
) -> Result<SecretIdentity> {
    let signing = ed25519_dalek::SigningKey::from_bytes(ms);
    let dk = decapsulation_key(&seal_sk)?;
    let seal_pk = dk.encapsulation_key().as_bytes().to_vec();

    let mut inbox_secret = Zeroizing::new([0u8; 32]);
    expand(ms.as_ref(), KDF_SALT, INBOX_INFO, inbox_secret.as_mut())?;

    // One-way step between the secret and the published tag: the tag is in
    // every OLB ID, and it must not let anyone reconstruct the secret that
    // will later authenticate inbox headers.
    let mut inbox = [0u8; INBOX_LEN];
    expand(
        inbox_secret.as_ref(),
        INBOX_TAG_INFO,
        INBOX_TAG_INFO,
        &mut inbox,
    )?;

    let mut public = PublicIdentity {
        v: ID_VERSION,
        name: clean_name(name)?,
        orcid: clean_orcid(orcid)?,
        sig_pk: signing.verifying_key().to_bytes().to_vec(),
        seal_pk,
        inbox: inbox.to_vec(),
        created,
        self_sig: Vec::new(),
    };
    public.self_sig = signing.sign(&canonical_bytes(&public)?).to_bytes().to_vec();

    Ok(SecretIdentity {
        signing,
        seal_sk,
        inbox_secret,
        public,
    })
}

/// The bytes `self_sig` covers (docs/11 §2).
///
/// Built here, by hand, and never by re-serialising CBOR: a verifier that
/// has to reproduce an encoder byte-for-byte is one encoder upgrade away
/// from rejecting every valid ID in the wild. Lengths are prefixed so no
/// two different field splits can produce the same buffer.
fn canonical_bytes(public: &PublicIdentity) -> Result<Vec<u8>> {
    if public.v > u8::MAX as u32 {
        return Err(KeyError::Malformed(format!(
            "identity version {} does not fit docs/11 §2's u8(v)",
            public.v
        )));
    }
    let mut out = Vec::with_capacity(ID_DOMAIN.len() + 1300);
    out.extend_from_slice(ID_DOMAIN);
    out.push(public.v as u8);
    push_len_prefixed(&mut out, public.name.as_bytes());
    push_len_prefixed(
        &mut out,
        public.orcid.as_deref().unwrap_or_default().as_bytes(),
    );
    out.extend_from_slice(&public.sig_pk);
    out.extend_from_slice(&public.seal_pk);
    out.extend_from_slice(&public.inbox);
    out.extend_from_slice(&public.created.to_be_bytes());
    Ok(out)
}

pub(crate) fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

pub(crate) fn expand(ikm: &[u8], salt: &[u8], info: &[u8], out: &mut [u8]) -> Result<()> {
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, out)
        .map_err(|e| KeyError::Malformed(format!("hkdf: {e}")))
}

pub(crate) fn verifying_key(bytes: &[u8]) -> Result<ed25519_dalek::VerifyingKey> {
    let arr: [u8; SIG_PK_LEN] = bytes
        .try_into()
        .map_err(|_| KeyError::Malformed("signing public key is not 32 bytes".into()))?;
    ed25519_dalek::VerifyingKey::from_bytes(&arr)
        .map_err(|e| KeyError::Malformed(format!("not an ed25519 public key: {e}")))
}

pub(crate) fn signature(bytes: &[u8]) -> Result<ed25519_dalek::Signature> {
    let arr: [u8; SIG_LEN] = bytes
        .try_into()
        .map_err(|_| KeyError::Malformed("signature is not 64 bytes".into()))?;
    Ok(ed25519_dalek::Signature::from_bytes(&arr))
}

pub(crate) fn encapsulation_key(bytes: &[u8]) -> Result<<MlKem768 as KemCore>::EncapsulationKey> {
    let enc = ml_kem::Encoded::<<MlKem768 as KemCore>::EncapsulationKey>::try_from(bytes)
        .map_err(|_| {
            KeyError::Malformed(format!(
                "sealing public key is {} bytes, expected {SEAL_PK_LEN}",
                bytes.len()
            ))
        })?;
    Ok(<MlKem768 as KemCore>::EncapsulationKey::from_bytes(&enc))
}

pub(crate) fn decapsulation_key(bytes: &[u8]) -> Result<Dk> {
    let enc = ml_kem::Encoded::<Dk>::try_from(bytes).map_err(|_| {
        KeyError::Malformed(format!(
            "sealing private key is {} bytes, which is not an ML-KEM-768 decapsulation key",
            bytes.len()
        ))
    })?;
    Ok(Dk::from_bytes(&enc))
}

/// 2026-01-01T00:00:00Z — docs/11 §2 step 5's floor on `created`.
const EPOCH_2026: u64 = 1_767_225_600;

/// Seconds since the Unix epoch. Public because invite expiry is checked
/// against this clock from outside the crate.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn display_is_clean(s: &str, max_chars: usize) -> bool {
    s.chars().count() <= max_chars && !s.chars().any(|c| c.is_control() || is_bidi_control(c))
}

/// Sanitise at mint time, so OLB never produces an ID whose signed name a
/// peer would have to refuse.
///
/// An EMPTY name is allowed. It used to be refused, which bricked first run:
/// `load_or_create` takes the name from the stored profile, a fresh profile
/// has none, so a new node could never create an identity at all and the
/// whole store stayed locked. A display name is self-declared and never
/// verified — the fingerprint is the only identifier that means anything —
/// so "no name declared" is a legitimate state, and an honest one. The
/// length cap and the sanitising below still apply to anything non-empty.
fn clean_name(name: &str) -> Result<String> {
    Ok(sanitise_display(name, NAME_MAX_CHARS))
}

fn clean_orcid(orcid: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = orcid else { return Ok(None) };
    let cleaned = sanitise_display(raw, ORCID_MAX_CHARS);
    if cleaned.is_empty() {
        return Ok(None);
    }
    if !orcid_is_well_formed(&cleaned) {
        return Err(KeyError::Refused(format!(
            "'{cleaned}' is not a well-formed ORCID (0000-0000-0000-0000); \
             a well-formed one is still not a verified one"
        )));
    }
    Ok(Some(cleaned))
}

/// Format plus the ISO 7064 MOD 11-2 check digit (docs/11 §2 step 7).
///
/// This says the string is *shaped* like an ORCID. It says nothing about
/// whether it belongs to the person claiming it — there is no ORCID OAuth
/// here, so no code or copy may treat this as verification.
pub fn orcid_is_well_formed(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 4 || parts.iter().any(|p| p.len() != 4) {
        return false;
    }
    let chars: Vec<char> = parts.concat().chars().collect();
    if chars.len() != 16 || !chars[..15].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut total: u32 = 0;
    for c in &chars[..15] {
        total = (total + (*c as u32 - '0' as u32)) * 2 % 11;
    }
    let expect = (12 - total % 11) % 11;
    let given = match chars[15] {
        'X' => 10,
        d if d.is_ascii_digit() => d as u32 - '0' as u32,
        _ => return false,
    };
    expect == given
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| KeyError::Io(e.to_string()))
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [3u8; 32];

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "olb-identity-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn an_id_round_trips_through_encode_and_decode() {
        let me = generate("Rui Santos", Some("0000-0002-1825-0097")).unwrap();
        let s = encode_id(&me.public).unwrap();
        assert!(s.starts_with(ID_PREFIX));
        let back = decode_id(&s).unwrap();
        assert_eq!(back, me.public);
        assert_eq!(fingerprint(&back), fingerprint(&me.public));
    }

    #[test]
    fn the_fingerprint_is_six_groups_of_four_uppercase_hex() {
        let me = generate("Rui Santos", None).unwrap();
        let fp = fingerprint(&me.public);
        let (left, right) = fp.split_once(" · ").expect("the · separator");
        let groups: Vec<&str> = left
            .split(' ')
            .chain(right.split(' '))
            .collect();
        assert_eq!(groups.len(), 6, "{fp}");
        for g in &groups {
            assert_eq!(g.len(), 4, "{fp}");
            assert!(
                g.chars()
                    .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)),
                "{fp} is not uppercase hex"
            );
        }
        assert_eq!(fingerprint_hex(&me.public), groups.concat());
    }

    #[test]
    fn a_corrupted_id_is_rejected_not_half_parsed() {
        let me = generate("Rui Santos", None).unwrap();
        let good = encode_id(&me.public).unwrap();

        assert!(decode_id("").is_err());
        assert!(decode_id(&good[5..]).is_err(), "no olb1: prefix");
        assert!(decode_id(&format!("olb2:{}", &good[5..])).is_err());
        assert!(decode_id(&format!("{ID_PREFIX}not base64url!!")).is_err());
        // Truncated CBOR.
        assert!(decode_id(&good[..good.len() - 40]).is_err());
        // One flipped character in the body.
        let mut bytes = good.clone().into_bytes();
        let i = bytes.len() - 6;
        bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
        assert!(decode_id(&String::from_utf8(bytes).unwrap()).is_err());
    }

    #[test]
    fn tampering_with_any_signed_field_invalidates_the_id() {
        let me = generate("Rui Santos", Some("0000-0002-1825-0097")).unwrap();

        let mut renamed = me.public.clone();
        renamed.name = "Mallory".into();
        assert!(decode_id(&encode_id(&renamed).unwrap()).is_err());

        let mut reorcided = me.public.clone();
        reorcided.orcid = None;
        assert!(decode_id(&encode_id(&reorcided).unwrap()).is_err());

        let mut reinboxed = me.public.clone();
        reinboxed.inbox[0] ^= 1;
        assert!(decode_id(&encode_id(&reinboxed).unwrap()).is_err());

        let mut redated = me.public.clone();
        redated.created -= 1;
        assert!(decode_id(&encode_id(&redated).unwrap()).is_err());

        let mut resigned = me.public.clone();
        resigned.self_sig[0] ^= 1;
        assert!(decode_id(&encode_id(&resigned).unwrap()).is_err());
    }

    #[test]
    fn substituting_the_sealing_key_changes_the_fingerprint() {
        // The attack the two-key fingerprint exists to stop: keep a signing
        // key the human has already verified, swap in your own sealing key,
        // and receive every project key sealed to it.
        let alice = generate("Alice", None).unwrap();
        let mallory = generate("Mallory", None).unwrap();
        let mut spliced = alice.public.clone();
        spliced.seal_pk = mallory.public.seal_pk.clone();
        assert_ne!(fingerprint(&spliced), fingerprint(&alice.public));
        // And it does not even parse, because `self_sig` covers `seal_pk`.
        assert!(decode_id(&encode_id(&spliced).unwrap()).is_err());
    }

    #[test]
    fn wrong_key_lengths_are_rejected() {
        let me = generate("Rui Santos", None).unwrap();
        for mutate in [
            (|p: &mut PublicIdentity| p.sig_pk.pop().map(|_| ()).unwrap()) as fn(&mut PublicIdentity),
            |p: &mut PublicIdentity| p.seal_pk.truncate(SEAL_PK_LEN - 1),
            |p: &mut PublicIdentity| p.inbox.push(0),
            |p: &mut PublicIdentity| p.self_sig.push(0),
        ] {
            let mut bad = me.public.clone();
            mutate(&mut bad);
            assert!(decode_id(&encode_id(&bad).unwrap()).is_err());
        }
    }

    #[test]
    fn an_extra_cbor_field_is_refused() {
        // Padding an ID with map entries no signature covers.
        let me = generate("Rui Santos", None).unwrap();
        let mut v: ciborium::Value =
            ciborium::from_reader(&cbor_of(&me.public)[..]).unwrap();
        if let ciborium::Value::Map(entries) = &mut v {
            entries.push((
                ciborium::Value::Text("role".into()),
                ciborium::Value::Text("lead".into()),
            ));
        } else {
            panic!("an identity encodes as a CBOR map");
        }
        let mut out = Vec::new();
        ciborium::into_writer(&v, &mut out).unwrap();
        let s = format!("{ID_PREFIX}{}", data_encoding::BASE64URL_NOPAD.encode(&out));
        assert!(decode_id(&s).is_err());
    }

    fn cbor_of(p: &PublicIdentity) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(p, &mut out).unwrap();
        out
    }

    #[test]
    fn an_identity_can_be_created_with_no_display_name() {
        // The first-run path: a fresh profile has no name, load_or_create
        // passes "" through, and refusing it left the whole store locked
        // with the user looking at an empty board. A name is self-declared
        // and never verified, so having none is legitimate.
        let me = generate("", None).expect("an empty name must be allowed");
        assert_eq!(me.public.name, "");
        let id = encode_id(&me.public).unwrap();
        let back = decode_id(&id).expect("an ID with no name must round-trip");
        assert_eq!(back.name, "");
        assert_eq!(back.sig_pk, me.public.sig_pk);
        // The fingerprint is the real identifier and is unaffected.
        assert!(!fingerprint(&me.public).is_empty());
    }

    #[test]
    fn a_name_can_be_added_later_without_changing_the_keys() {
        let mut me = generate("", None).unwrap();
        let fp_before = fingerprint(&me.public);
        let pk_before = me.public.sig_pk.clone();
        resign(&mut me, "Priya Raman", None).unwrap();
        assert_eq!(me.public.name, "Priya Raman");
        assert_eq!(me.public.sig_pk, pk_before);
        // Fingerprint covers the keys, not the name, so it must not move.
        assert_eq!(fingerprint(&me.public), fp_before);
        decode_id(&encode_id(&me.public).unwrap()).expect("still valid after rename");
    }

    #[test]
    fn a_hostile_display_name_is_sanitised_at_mint_time() {
        let nasty = format!("Ali\u{202E}ce\u{0007}{}", "x".repeat(10_000));
        let me = generate(&nasty, None).unwrap();
        assert_eq!(me.public.name.chars().count(), NAME_MAX_CHARS);
        assert!(!me.public.name.contains('\u{202E}'));
        assert!(!me.public.name.contains('\u{0007}'));
        // And the minted ID parses, because the name it signed is clean.
        assert!(decode_id(&encode_id(&me.public).unwrap()).is_ok());
    }

    #[test]
    fn a_blank_name_is_accepted_and_a_bad_orcid_is_refused() {
        // A whitespace-only name sanitises to empty, which is allowed: this
        // test previously asserted the opposite and that assertion was the
        // bug — it locked the store on every first run.
        assert_eq!(generate("   ", None).unwrap().public.name, "");
        // A malformed ORCID is still refused: unlike a name, it has a
        // checksum, so a wrong one is an error rather than a claim.
        assert!(generate("Rui", Some("1234-5678-9012-3456")).is_err());
        // Blank ORCID means "not given", not "invalid".
        assert_eq!(generate("Rui", Some("  ")).unwrap().public.orcid, None);
    }

    #[test]
    fn the_orcid_checksum_is_the_real_iso_7064() {
        assert!(orcid_is_well_formed("0000-0002-1825-0097"));
        assert!(orcid_is_well_formed("0000-0001-5109-3700"));
        assert!(orcid_is_well_formed("0000-0002-1694-233X"));
        assert!(!orcid_is_well_formed("0000-0002-1825-0098"));
        assert!(!orcid_is_well_formed("0000-0002-1825-009"));
        assert!(!orcid_is_well_formed("0000000218250097"));
    }

    #[test]
    fn renaming_keeps_the_keys_and_the_fingerprint() {
        let mut me = generate("Rui Santos", None).unwrap();
        let fp = fingerprint(&me.public);
        let sig_pk = me.public.sig_pk.clone();
        resign(&mut me, "Rui S. Santos", Some("0000-0002-1825-0097")).unwrap();
        assert_eq!(me.public.name, "Rui S. Santos");
        assert_eq!(me.public.sig_pk, sig_pk);
        assert_eq!(fingerprint(&me.public), fp);
        assert!(decode_id(&encode_id(&me.public).unwrap()).is_ok());
    }

    #[test]
    fn the_inbox_topic_is_one_of_sixteen_buckets() {
        let me = generate("Rui", None).unwrap();
        let t = inbox_topic(&me.public);
        assert!(t.starts_with("/olb/1/inbox-") && t.ends_with("/proto"), "{t}");
        let bucket: u8 = t
            .trim_start_matches("/olb/1/inbox-")
            .trim_end_matches("/proto")
            .parse()
            .unwrap();
        assert!(bucket < INBOX_BUCKETS);
        assert_eq!(bucket, me.public.inbox[0] % INBOX_BUCKETS);
    }

    #[test]
    fn the_published_inbox_tag_does_not_reveal_the_inbox_secret() {
        let me = generate("Rui", None).unwrap();
        assert_eq!(me.public.inbox.len(), INBOX_LEN);
        assert_ne!(&me.public.inbox[..], &me.inbox_secret[..INBOX_LEN]);
    }

    #[test]
    fn a_relaunch_loads_the_same_identity() {
        // The R6 gap: the store directory outlives the profile tree, so the
        // author of a new commit must match the previous run's.
        let d = tmpdir("relaunch");
        let first = load_or_create(&d, &KEY, "Rui Santos", None).unwrap();
        let second = load_or_create(&d, &KEY, "Rui Santos", None).unwrap();
        assert_eq!(first.public, second.public);
        assert_eq!(first.signing.to_bytes(), second.signing.to_bytes());
        assert_eq!(*first.seal_sk, *second.seal_sk);
    }

    #[test]
    fn the_vault_is_sealed_and_a_wrong_store_key_cannot_open_it() {
        let d = tmpdir("sealed");
        let me = load_or_create(&d, &KEY, "Rui Santos", None).unwrap();
        let raw = std::fs::read(d.join(VAULT_FILE)).unwrap();
        // The seed is not in the file in the clear.
        assert!(!raw
            .windows(32)
            .any(|w| w == me.signing.to_bytes().as_slice()));
        assert!(load_or_create(&d, &[9u8; 32], "Rui Santos", None).is_err());
    }

    #[test]
    fn a_rename_through_load_or_create_is_persisted() {
        let d = tmpdir("rename");
        let fp = fingerprint(&load_or_create(&d, &KEY, "Rui Santos", None).unwrap().public);
        let renamed = load_or_create(&d, &KEY, "Rui S. Santos", None).unwrap();
        assert_eq!(renamed.public.name, "Rui S. Santos");
        assert_eq!(fingerprint(&renamed.public), fp);
        let reloaded = load_or_create(&d, &KEY, "", None).unwrap();
        assert_eq!(reloaded.public.name, "Rui S. Santos");
    }

    #[test]
    fn migration_from_a_plaintext_identity_key_preserves_the_public_key() {
        // The whole point: a node that silently changes its author key
        // orphans every commit it has ever signed.
        let d = tmpdir("migrate");
        let seed = [42u8; 32];
        std::fs::write(d.join(LEGACY_FILE), seed).unwrap();
        let expected = ed25519_dalek::SigningKey::from_bytes(&seed)
            .verifying_key()
            .to_bytes()
            .to_vec();

        let me = load_or_create(&d, &KEY, "Rui Santos", None).unwrap();
        assert_eq!(me.public.sig_pk, expected);
        assert_eq!(me.signing.to_bytes(), seed);
        // The plaintext seed is gone and the vault is there.
        assert!(!d.join(LEGACY_FILE).exists());
        assert!(d.join(VAULT_FILE).exists());
        // And it is stable across the next launch.
        let again = load_or_create(&d, &KEY, "Rui Santos", None).unwrap();
        assert_eq!(again.public, me.public);
    }

    #[test]
    fn a_legacy_file_of_the_wrong_length_is_refused_not_replaced() {
        let d = tmpdir("badlegacy");
        std::fs::write(d.join(LEGACY_FILE), b"not a seed").unwrap();
        assert!(load_or_create(&d, &KEY, "Rui", None).is_err());
        // Still there, for the user to investigate.
        assert!(d.join(LEGACY_FILE).exists());
        assert!(!d.join(VAULT_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_vault_is_not_group_or_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmpdir("perms");
        load_or_create(&d, &KEY, "Rui", None).unwrap();
        let mode = std::fs::metadata(d.join(VAULT_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "mode {:o}", mode);
    }
}
