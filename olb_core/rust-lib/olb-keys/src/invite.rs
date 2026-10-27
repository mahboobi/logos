//! Invites, membership, and the key hierarchy (docs/04 §4–§7).
//!
//! CONTRACT FILE — signatures are fixed.
//!
//! # The hybrid key model, and the three things that must land together
//!
//! The project key covers **metadata and membership only**. Idea content
//! keys are independent and granted explicitly, per member per idea
//! (docs/04 §6, as amended 26 Oct 2026). Implementing half of this is worse
//! than implementing neither, because each half implies a guarantee the
//! other half delivers:
//!
//! 1. An invite seals **N keys**, not one: the project metadata key plus one
//!    content key per idea in the access list.
//! 2. Removal rotates **only the keys that member actually held**. Rotating
//!    everything on every removal would be simpler and would also leak the
//!    access list to anyone watching key versions.
//! 3. **Removal is forward-only.** You cannot un-share a key somebody already
//!    has. They keep what they could already read and lose only what comes
//!    next, and the UI must say exactly that rather than implying revocation.

use crate::Result;
use zeroize::Zeroizing;

/// What a member may do. Enforcement is not here — it happens in every
/// member's core when validating incoming commits, which is what makes it
/// hold without trusting any single node (docs/04 §6, docs/10 §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Lead,
    Contributor,
    /// Read-only. A commit from a reviewer that edits records is rejected by
    /// every honest peer, and reviewers cannot approve merges (docs/05 §3).
    Reviewer,
    /// Pins ciphertext and receives NO content keys at all (docs/04 §8).
    Replica,
}

/// A symmetric content key, wiped on drop.
pub type ContentKey = Zeroizing<[u8; 32]>;

/// Which key, and which version of it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum KeyScope {
    /// Project metadata and membership.
    ProjectMeta,
    /// One idea's content.
    Idea(String),
}

/// A key sealed to one recipient's ML-KEM-768 encapsulation key.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SealedKey {
    pub scope: KeyScope,
    pub version: u32,
    /// ML-KEM ciphertext encapsulating the shared secret.
    pub kem_ct: Vec<u8>,
    /// The content key, wrapped under a secret derived from the KEM shared
    /// secret with HKDF. The KEM output is never used as a cipher key
    /// directly.
    pub wrapped: Vec<u8>,
}

/// Published to the invitee's inbox topic (docs/04 §4).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InviteEnvelope {
    pub v: u32,
    pub invite_id: String,
    pub project_id: String,
    pub project_name: String,
    pub role: Role,
    pub idea_ids: Vec<String>,
    /// One per key the invitee is being granted — N keys, not one.
    pub keys: Vec<SealedKey>,
    /// 64 hex Ed25519 public key of the inviter.
    pub inviter: String,
    /// Encrypted to the invitee; the note is not public.
    pub note: Vec<u8>,
    /// Unix seconds. Default 14 days (docs/04 §4).
    pub expiry: u64,
    /// Ed25519 over the canonical form of every field above.
    pub sig: String,
}

/// Where an invite has got to (docs/04 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InviteState {
    Sealed,
    Sent,
    Received,
    Accepted,
    Declined,
    Expired,
    Revoked,
}

/// A project member as recorded in the membership state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Member {
    pub sig_pk: String,
    pub seal_pk: Vec<u8>,
    pub name: String,
    pub role: Role,
    /// Ideas this member has been granted a content key for.
    pub ideas: Vec<String>,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<i64>,
}

/// Domain tag for an invite signature.
pub const INVITE_DOMAIN: &[u8] = b"olb.invite.v1\0";
/// Domain tag for the acceptance receipt.
pub const ACCEPT_DOMAIN: &[u8] = b"olb.invite.accept.v1\0";
/// HKDF info string for turning a KEM shared secret into a wrapping key.
pub const WRAP_INFO: &[u8] = b"olb.keywrap.v1";

/// Seal one content key to a recipient's ML-KEM-768 encapsulation key.
///
/// Encapsulate, run the shared secret through HKDF-SHA256 with `WRAP_INFO`,
/// and use the output as an AEAD key to wrap the content key. The KEM shared
/// secret is never used as a cipher key directly.
pub fn seal_key(
    recipient_seal_pk: &[u8],
    scope: KeyScope,
    version: u32,
    key: &ContentKey,
) -> Result<SealedKey> {
    let _ = (recipient_seal_pk, scope, version, key);
    todo!("docs/04 §6")
}

/// Unseal a key addressed to us.
pub fn unseal_key(seal_sk: &[u8], sealed: &SealedKey) -> Result<ContentKey> {
    let _ = (seal_sk, sealed);
    todo!("docs/04 §6")
}

/// Build an invite: N sealed keys, an encrypted note, and a signature.
pub fn build_invite(
    inviter: &crate::identity::SecretIdentity,
    invitee: &crate::identity::PublicIdentity,
    project_id: &str,
    project_name: &str,
    role: Role,
    idea_ids: &[String],
    keys: &[(KeyScope, u32, ContentKey)],
    note: &str,
    expiry: u64,
) -> Result<InviteEnvelope> {
    let _ = (
        inviter, invitee, project_id, project_name, role, idea_ids, keys, note, expiry,
    );
    todo!("docs/04 §4")
}

/// Verify an incoming invite's signature and expiry.
///
/// Verification is NOT acceptance. Never auto-accept: the UI must show the
/// inviter's fingerprint and wait for the user, because a signature only
/// proves who sent it, not that the user wanted it (docs/04 §9).
pub fn verify_invite(env: &InviteEnvelope, now: u64) -> Result<()> {
    let _ = (env, now);
    todo!("docs/04 §4 and §9")
}

/// Unseal every key in an accepted invite.
pub fn accept_invite(
    me: &crate::identity::SecretIdentity,
    env: &InviteEnvelope,
) -> Result<Vec<(KeyScope, u32, ContentKey)>> {
    let _ = (me, env);
    todo!("docs/04 §4")
}

/// Signed receipt the invitee publishes back to the project topic.
pub fn sign_accept(
    me: &crate::identity::SecretIdentity,
    invite_id: &str,
) -> Result<String> {
    let _ = (me, invite_id);
    todo!("docs/04 §4")
}

/// A one-time invite code (docs/04 §5).
///
/// The code carries NO project data — it only lets the holder's node find
/// the inviter's. Everything else happens after the lead has compared
/// fingerprints and approved.
pub fn make_code(me: &crate::identity::SecretIdentity, project_id: &str) -> Result<String> {
    let _ = (me, project_id);
    todo!("docs/04 §5")
}

/// Parse a redeemed code.
pub fn parse_code(code: &str) -> Result<(String, String)> {
    let _ = code;
    todo!("docs/04 §5")
}

/// Remove a member and rotate forward (docs/04 §7).
///
/// Returns the new key versions to seal to each REMAINING member. Rotates
/// only the scopes the removed member actually held: rotating everything
/// would leak their access list to anyone who can see key versions.
///
/// What this does NOT do, and cannot: make previously shared content
/// unreadable to them. They keep what they already had. The UI copy for this
/// is fixed in docs/04 §7 — "They keep what they already had, but cannot
/// read anything new" — and must not be softened into something that sounds
/// like revocation.
pub fn rotate_after_removal(
    removed: &Member,
    remaining: &[Member],
    current_versions: &[(KeyScope, u32)],
) -> Result<Vec<(Member, Vec<SealedKey>)>> {
    let _ = (removed, remaining, current_versions);
    todo!("docs/04 §7")
}

/// May this key write records to this project at this moment?
///
/// Implements `olb_vcs::dag::MemberPolicy` for the real membership state, so
/// the DAG can enforce roles without depending on this crate.
pub struct Membership {
    pub members: Vec<Member>,
    pub approvals_required: std::collections::BTreeMap<String, usize>,
}

impl olb_vcs::dag::MemberPolicy for Membership {
    fn may_write(&self, _author_hex: &str, _at_commit: Option<&str>) -> bool {
        todo!("docs/04 §6 — reviewers and replicas may not write")
    }

    fn may_approve(&self, _key_hex: &str, _at_commit: Option<&str>) -> bool {
        todo!("docs/05 §3 — reviewers cannot approve merges")
    }

    fn approvals_required(&self, _branch: &str) -> usize {
        todo!("docs/10 §4.5 — snapshotted per branch")
    }
}
