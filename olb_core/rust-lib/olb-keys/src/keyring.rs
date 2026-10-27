//! Where the store key lives.
//!
//! The key that encrypts the local store is kept in the macOS login Keychain,
//! not beside the data it protects. That is the entire security argument: a
//! copied `~/Library/Application Support/OpenLabbook` folder, a stolen laptop
//! at rest, or a Time Machine backup is useless without it. A key file next
//! to the ciphertext would buy nothing against any of those.
//!
//! What this does NOT defend against, stated plainly so nobody is surprised
//! later: malware running as the logged-in user. Once the Keychain is
//! unlocked — which it is, for the whole session, as soon as you log in —
//! anything running as you can ask for the same item. Defending against that
//! needs a passphrase that never touches disk, which was considered and
//! deliberately not chosen (docs/08 R13).

use crate::{KeyError, Result};
use zeroize::Zeroizing;

/// Keychain service name. Shared across profiles; the account distinguishes
/// them, so alice and bob hold separate keys and can never read each other's
/// store even though both run on this machine.
const SERVICE: &str = "OpenLabbook";

/// Test and CI override, read before the Keychain is ever touched.
///
/// Without this every `cargo test` run would prompt for Keychain access and
/// leave real items behind on a developer's machine. 64 hex characters.
const ENV_OVERRIDE: &str = "OLB_STORE_KEY_HEX";

/// A 32-byte symmetric key, wiped when dropped.
///
/// `Zeroizing` is not security theatre here: the store key is long-lived and
/// a plain `[u8; 32]` would be left behind in freed heap pages and in any
/// core dump taken while the app is running.
pub type StoreKey = Zeroizing<[u8; 32]>;

/// Fetch this profile's store key, generating and storing one on first run.
///
/// `profile` is the Keychain account, normally `$LOGOS_PROFILE`.
pub fn store_key(profile: &str) -> Result<StoreKey> {
    if let Ok(hex_key) = std::env::var(ENV_OVERRIDE) {
        let bytes = hex::decode(hex_key.trim())
            .map_err(|e| KeyError::Malformed(format!("{ENV_OVERRIDE} is not hex: {e}")))?;
        let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
            KeyError::Malformed(format!("{ENV_OVERRIDE} must be 64 hex characters (32 bytes)"))
        })?;
        return Ok(Zeroizing::new(arr));
    }
    platform::store_key(SERVICE, profile)
}

/// Delete this profile's store key. Used by tests and by an explicit
/// "forget this profile" action — never automatically, because without the
/// key the store is permanently unreadable.
pub fn forget(profile: &str) -> Result<()> {
    if std::env::var(ENV_OVERRIDE).is_ok() {
        return Ok(());
    }
    platform::forget(SERVICE, profile)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use rand::RngCore;
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    pub fn store_key(service: &str, account: &str) -> Result<StoreKey> {
        match get_generic_password(service, account) {
            Ok(bytes) => {
                let arr: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                    // Refuse rather than silently re-key: a wrong-length item
                    // means something else wrote it, and generating a
                    // replacement would make the existing store unreadable
                    // with no warning.
                    KeyError::Keyring(format!(
                        "keychain item {service}/{account} is {} bytes, expected 32; \
                         refusing to replace it — move it aside to investigate",
                        bytes.len()
                    ))
                })?;
                Ok(Zeroizing::new(arr))
            }
            Err(_) => {
                let mut key = Zeroizing::new([0u8; 32]);
                rand::rngs::OsRng.fill_bytes(key.as_mut());
                set_generic_password(service, account, key.as_ref())
                    .map_err(|e| KeyError::Keyring(format!("cannot store key: {e}")))?;
                Ok(key)
            }
        }
    }

    pub fn forget(service: &str, account: &str) -> Result<()> {
        match delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            // Already absent is the desired end state, not a failure.
            Err(_) => Ok(()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    /// No Secret Service backend yet (docs/08 R13 follow-up).
    ///
    /// This refuses rather than falling back to a key file. A silent fallback
    /// would mean a Linux build claiming "encrypted at rest" while keeping
    /// the key next to the ciphertext, which protects nothing and is worse
    /// than an honest error.
    pub fn store_key(_service: &str, _account: &str) -> Result<StoreKey> {
        Err(KeyError::Keyring(
            "no OS keychain backend on this platform; set OLB_STORE_KEY_HEX \
             explicitly, or add a Secret Service backend"
                .into(),
        ))
    }

    pub fn forget(_service: &str, _account: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests set a process-wide env var, so they must not run
    /// concurrently with each other. Rust runs tests in threads by default,
    /// hence the mutex rather than `#[serial]` (one less dependency).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn override_is_used_verbatim() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_OVERRIDE, "11".repeat(32));
        let k = store_key("test-profile").unwrap();
        assert_eq!(*k, [0x11u8; 32]);
        std::env::remove_var(ENV_OVERRIDE);
    }

    #[test]
    fn override_of_the_wrong_length_is_refused() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_OVERRIDE, "aabb");
        let err = store_key("test-profile").unwrap_err();
        assert!(matches!(err, KeyError::Malformed(_)), "got {err:?}");
        std::env::remove_var(ENV_OVERRIDE);
    }

    #[test]
    fn override_that_is_not_hex_is_refused() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ENV_OVERRIDE, "z".repeat(64));
        assert!(store_key("test-profile").is_err());
        std::env::remove_var(ENV_OVERRIDE);
    }
}
