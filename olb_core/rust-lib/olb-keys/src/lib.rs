//! Identity, sealing, and the encrypted durable store.
//!
//! Pure like `olb-vcs`: no Logos SDK, so it has tests. What it is NOT pure
//! about is the filesystem and the macOS Keychain — those are the point of
//! the crate. Every one of them is reachable behind an override so tests
//! never touch the real Keychain or the real store directory.
//!
//! Specification: `docs/04-invite-flow.md` (invites, the hybrid key model),
//! `docs/11-identity-design.md` (identity), `docs/08` R6 (why the store lives
//! outside the profile tree) and R13 (what sealing does and does not buy).

pub mod identity;
pub mod invite;
pub mod keyring;
pub mod passphrase;
pub mod store;
pub mod vault;

/// Anything that can go wrong at the key, vault or store layer.
///
/// Separate from `olb_vcs::VcsError` on purpose: a caller that fails to open
/// the vault is in a completely different situation from one that received a
/// commit with a bad signature, and collapsing the two would let a decryption
/// failure be reported to the user as "bad signature".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyError {
    /// The OS keychain refused or is unavailable.
    Keyring(String),
    /// Ciphertext failed authentication. Either the wrong key, or tampering —
    /// AEAD cannot tell you which, and neither can we.
    Decrypt(String),
    Io(String),
    Malformed(String),
    NotFound(String),
    /// A rule was broken: sealing to an unknown member, rotating a key that
    /// does not exist, removing the last owner.
    Refused(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::Keyring(m) => write!(f, "keychain: {m}"),
            KeyError::Decrypt(m) => write!(f, "decrypt failed: {m}"),
            KeyError::Io(m) => write!(f, "io: {m}"),
            KeyError::Malformed(m) => write!(f, "malformed: {m}"),
            KeyError::NotFound(m) => write!(f, "not found: {m}"),
            KeyError::Refused(m) => write!(f, "refused: {m}"),
        }
    }
}

impl std::error::Error for KeyError {}

pub type Result<T> = std::result::Result<T, KeyError>;
