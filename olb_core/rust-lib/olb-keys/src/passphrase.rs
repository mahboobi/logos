//! Passphrase-protected store key (docs/11 §1.4, chosen 27 Oct 2026).
//!
//! The store is encrypted with a random 32-byte key. That key is not derived
//! from the passphrase — it is *wrapped* by a KEK that is. The indirection
//! buys two things: changing a passphrase rewraps 32 bytes instead of
//! re-encrypting the whole store, and the key that protects the data never
//! depends on how good the passphrase was at the moment it was chosen.
//!
//! What this protects, and what it does not. A copied store directory, a
//! stolen laptop at rest, and a Time Machine backup are all useless without
//! the passphrase, because the KEK exists only in memory while the app runs.
//! It does NOT defend against malware running as the logged-in user while
//! the store is unlocked, and it cannot — the key has to be somewhere to
//! decrypt anything.
//!
//! **There is no recovery.** Forgetting the passphrase means the store is
//! gone. That is the point of it, and any surface that collects one has to
//! say so plainly rather than treating it as a password that can be reset.

use crate::{vault, KeyError, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

/// The file holding the wrapped key. Readable by anyone — it is useless
/// without the passphrase, which is the whole design.
const VAULT_FILE: &str = "keyvault.json";
/// AAD binding the wrapped key to its purpose.
const WRAP_AAD: &str = "olb.keyvault.v1";

/// Argon2id cost. 64 MiB, 3 passes, 1 lane — the RFC 9106 "second
/// recommended" option, which is a deliberate second or so on a laptop.
/// A faster setting would make the prompt feel better and make an offline
/// guessing attack proportionally cheaper, which is the wrong trade for the
/// one secret that protects everything else.
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 1;

/// Stored beside the wrapped key so a future cost increase can still open
/// an old vault. Reading the parameters from the file rather than assuming
/// today's constants is what makes raising them later a non-event.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct KdfParams {
    algorithm: String,
    version: u32,
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            algorithm: "argon2id".into(),
            version: 0x13,
            m_cost_kib: M_COST_KIB,
            t_cost: T_COST,
            p_cost: P_COST,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct KeyVault {
    v: u32,
    kdf: KdfParams,
    /// 16 bytes, hex.
    salt: String,
    /// The store key sealed under the KEK, hex.
    wrapped: String,
}

/// Is a passphrase already set for this store?
pub fn exists(dir: &Path) -> bool {
    dir.join(VAULT_FILE).exists()
}

fn derive(passphrase: &str, salt: &[u8], kdf: &KdfParams) -> Result<Zeroizing<[u8; 32]>> {
    if kdf.algorithm != "argon2id" {
        return Err(KeyError::Malformed(format!(
            "unsupported key derivation '{}'",
            kdf.algorithm
        )));
    }
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32))
        .map_err(|e| KeyError::Malformed(format!("bad Argon2 parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, out.as_mut())
        .map_err(|e| KeyError::Malformed(format!("key derivation failed: {e}")))?;
    Ok(out)
}

/// Minimum passphrase length.
///
/// Deliberately a length floor and nothing else — no character-class rules.
/// Those push people toward `Passw0rd!` and away from the long, memorable
/// phrases that actually resist an offline attack, which is the only attack
/// this faces.
pub const MIN_LEN: usize = 10;

fn check_strength(passphrase: &str) -> Result<()> {
    let n = passphrase.chars().count();
    if n < MIN_LEN {
        return Err(KeyError::Refused(format!(
            "a passphrase needs at least {MIN_LEN} characters; this one has {n}. \
             A few ordinary words you will remember beat a short complicated one — \
             and there is no way to reset it if you forget."
        )));
    }
    Ok(())
}

/// Create the vault: mint a store key, wrap it under the passphrase.
///
/// Refuses if one already exists, rather than overwriting. Overwriting a
/// keyvault silently orphans the store it was protecting, which is the same
/// class of mistake that already cost one store on this project.
pub fn create(dir: &Path, passphrase: &str) -> Result<Zeroizing<[u8; 32]>> {
    if exists(dir) {
        return Err(KeyError::Refused(
            "a passphrase is already set for this store; changing it is \
             change_passphrase, not create"
                .into(),
        ));
    }
    check_strength(passphrase)?;
    std::fs::create_dir_all(dir).map_err(|e| KeyError::Io(e.to_string()))?;

    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let mut store_key = Zeroizing::new([0u8; 32]);
    rand::rngs::OsRng.fill_bytes(store_key.as_mut());

    let kdf = KdfParams::default();
    let kek = derive(passphrase, &salt, &kdf)?;
    let wrapped = vault::seal(&kek, WRAP_AAD, store_key.as_ref())?;

    let doc = KeyVault {
        v: 1,
        kdf,
        salt: hex::encode(salt),
        wrapped: hex::encode(&wrapped),
    };
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| KeyError::Malformed(e.to_string()))?;
    write_atomic(&dir.join(VAULT_FILE), &bytes)?;
    Ok(store_key)
}

/// Unlock: derive the KEK and unwrap the store key.
pub fn unlock(dir: &Path, passphrase: &str) -> Result<Zeroizing<[u8; 32]>> {
    let raw = std::fs::read(dir.join(VAULT_FILE)).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            KeyError::NotFound("no passphrase has been set for this store yet".into())
        } else {
            KeyError::Io(e.to_string())
        }
    })?;
    let doc: KeyVault =
        serde_json::from_slice(&raw).map_err(|e| KeyError::Malformed(format!("keyvault: {e}")))?;
    if doc.v != 1 {
        return Err(KeyError::Malformed(format!(
            "keyvault version {} is not supported",
            doc.v
        )));
    }
    let salt = hex::decode(&doc.salt).map_err(|e| KeyError::Malformed(e.to_string()))?;
    let wrapped = hex::decode(&doc.wrapped).map_err(|e| KeyError::Malformed(e.to_string()))?;

    let kek = derive(passphrase, &salt, &doc.kdf)?;
    // A wrong passphrase fails here, as an AEAD tag mismatch. The message
    // says "passphrase" rather than "authentication failed" because that is
    // what it means to the person typing, and because the alternative —
    // a tampered file — looks identical and leads to the same next step.
    let key = vault::open(&kek, WRAP_AAD, &wrapped)
        .map_err(|_| KeyError::Decrypt("that passphrase does not open this store".into()))?;
    let arr: [u8; 32] = key
        .as_slice()
        .try_into()
        .map_err(|_| KeyError::Malformed("the wrapped key is not 32 bytes".into()))?;
    Ok(Zeroizing::new(arr))
}

/// Change the passphrase by rewrapping the same store key.
///
/// The store key does not change, so nothing already encrypted has to be
/// rewritten — and a member who already holds data keeps reading it.
pub fn change(dir: &Path, current: &str, new: &str) -> Result<()> {
    let store_key = unlock(dir, current)?;
    check_strength(new)?;

    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    let kdf = KdfParams::default();
    let kek = derive(new, &salt, &kdf)?;
    let wrapped = vault::seal(&kek, WRAP_AAD, store_key.as_ref())?;

    let doc = KeyVault {
        v: 1,
        kdf,
        salt: hex::encode(salt),
        wrapped: hex::encode(&wrapped),
    };
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|e| KeyError::Malformed(e.to_string()))?;
    write_atomic(&dir.join(VAULT_FILE), &bytes)
}

/// Temp-then-rename. A half-written keyvault is an unopenable store, so the
/// replacement has to be atomic even though the file is small.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp).map_err(|e| KeyError::Io(e.to_string()))?;
        f.write_all(bytes).map_err(|e| KeyError::Io(e.to_string()))?;
        f.sync_all().map_err(|e| KeyError::Io(e.to_string()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| KeyError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "olb-pass-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// Argon2id at 64 MiB is deliberately slow, so the tests that only need
    /// a vault use a cheap one. The real cost is covered once, below.
    fn cheap_vault(dir: &Path, pw: &str) -> Zeroizing<[u8; 32]> {
        std::fs::create_dir_all(dir).unwrap();
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let mut key = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng.fill_bytes(key.as_mut());
        let kdf = KdfParams {
            algorithm: "argon2id".into(),
            version: 0x13,
            m_cost_kib: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let kek = derive(pw, &salt, &kdf).unwrap();
        let wrapped = vault::seal(&kek, WRAP_AAD, key.as_ref()).unwrap();
        let doc = KeyVault {
            v: 1,
            kdf,
            salt: hex::encode(salt),
            wrapped: hex::encode(&wrapped),
        };
        write_atomic(
            &dir.join(VAULT_FILE),
            &serde_json::to_vec_pretty(&doc).unwrap(),
        )
        .unwrap();
        key
    }

    #[test]
    fn unlock_returns_the_same_key_that_was_wrapped() {
        let d = tmpdir("roundtrip");
        let made = cheap_vault(&d, "correct horse battery");
        let got = unlock(&d, "correct horse battery").unwrap();
        assert_eq!(*made, *got);
    }

    #[test]
    fn a_wrong_passphrase_is_refused() {
        let d = tmpdir("wrong");
        cheap_vault(&d, "correct horse battery");
        let err = unlock(&d, "correct horse batterz").unwrap_err();
        assert!(matches!(err, KeyError::Decrypt(_)), "got {err:?}");
    }

    #[test]
    fn the_stored_parameters_are_used_not_todays_constants() {
        // A vault written at a low cost must still open after the defaults
        // are raised. Reading the parameters from the file is what makes
        // increasing them later a non-event rather than a migration.
        let d = tmpdir("params");
        let made = cheap_vault(&d, "correct horse battery");
        let raw = std::fs::read(d.join(VAULT_FILE)).unwrap();
        let doc: KeyVault = serde_json::from_slice(&raw).unwrap();
        assert_eq!(doc.kdf.m_cost_kib, 8, "fixture should be the cheap cost");
        assert_ne!(doc.kdf.m_cost_kib, M_COST_KIB);
        assert_eq!(*unlock(&d, "correct horse battery").unwrap(), *made);
    }

    #[test]
    fn changing_the_passphrase_keeps_the_same_store_key() {
        // The store key must survive, or every byte already encrypted
        // becomes unreadable the moment someone changes their passphrase.
        let d = tmpdir("change");
        let before = cheap_vault(&d, "correct horse battery");
        change(&d, "correct horse battery", "a different long phrase").unwrap();
        let after = unlock(&d, "a different long phrase").unwrap();
        assert_eq!(*before, *after);
        assert!(unlock(&d, "correct horse battery").is_err());
    }

    #[test]
    fn changing_with_the_wrong_current_passphrase_is_refused() {
        let d = tmpdir("changewrong");
        cheap_vault(&d, "correct horse battery");
        assert!(change(&d, "not it at all", "a different long phrase").is_err());
        // And the original still opens — a failed change must not damage it.
        assert!(unlock(&d, "correct horse battery").is_ok());
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_vault() {
        // Overwriting orphans the store the old key protected. This project
        // has already lost one store to exactly that shape of mistake.
        let d = tmpdir("nooverwrite");
        cheap_vault(&d, "correct horse battery");
        assert!(matches!(
            create(&d, "some other long phrase").unwrap_err(),
            KeyError::Refused(_)
        ));
    }

    #[test]
    fn a_short_passphrase_is_refused_with_a_useful_reason() {
        let d = tmpdir("short");
        let err = create(&d, "short").unwrap_err();
        match err {
            KeyError::Refused(m) => {
                assert!(m.contains("10"), "should say the minimum: {m}");
                assert!(m.contains("forget"), "should warn there is no reset: {m}");
            }
            other => panic!("got {other:?}"),
        }
        assert!(!exists(&d), "nothing should be written for a refused create");
    }

    #[test]
    fn a_missing_vault_says_so_rather_than_failing_obscurely() {
        let d = tmpdir("missing");
        std::fs::create_dir_all(&d).unwrap();
        assert!(!exists(&d));
        assert!(matches!(
            unlock(&d, "correct horse battery").unwrap_err(),
            KeyError::NotFound(_)
        ));
    }

    #[test]
    fn a_tampered_vault_does_not_open() {
        let d = tmpdir("tamper");
        cheap_vault(&d, "correct horse battery");
        let raw = std::fs::read(d.join(VAULT_FILE)).unwrap();
        let mut doc: KeyVault = serde_json::from_slice(&raw).unwrap();
        let mut w = hex::decode(&doc.wrapped).unwrap();
        let last = w.len() - 1;
        w[last] ^= 1;
        doc.wrapped = hex::encode(&w);
        std::fs::write(
            d.join(VAULT_FILE),
            serde_json::to_vec_pretty(&doc).unwrap(),
        )
        .unwrap();
        assert!(unlock(&d, "correct horse battery").is_err());
    }

    #[test]
    #[ignore = "runs the real 64 MiB Argon2id cost; slow by design"]
    fn the_real_cost_round_trips() {
        let d = tmpdir("realcost");
        let made = create(&d, "correct horse battery staple").unwrap();
        let got = unlock(&d, "correct horse battery staple").unwrap();
        assert_eq!(*made, *got);
    }
}
