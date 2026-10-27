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
//!
//! # What is addressed by what
//!
//! Every recipient here is addressed by an ML-KEM-768 encapsulation key and
//! identified by an Ed25519 key. A Logos Chat address is never either of
//! those: it changes every time Basecamp starts (CLAUDE.md constraint 5), so
//! nothing in this module accepts or stores one.
//!
//! # Construction, once, for every seal in this file
//!
//! ```text
//! (kem_ct, ss) = MLKEM768.Encaps(recipient.seal_pk)
//! wrap_key     = HKDF-SHA256(salt = label, ikm = ss).expand(WRAP_INFO, 32)
//! wrapped      = XChaCha20-Poly1305(wrap_key, random nonce, secret, aad = label)
//! ```
//!
//! The KEM shared secret is **never** used as a cipher key directly, and
//! `vault::seal` is the only AEAD construction in this crate — a second one
//! would be a second thing to get wrong. `label` names the scope and version
//! and is authenticated, so a key wrapped for `idea:7:lid` cannot be
//! relabelled as the project metadata key and still open.

use crate::identity::{push_len_prefixed, SecretIdentity};
use crate::{vault, KeyError, Result};
use ed25519_dalek::Signer;
use ml_kem::kem::{Decapsulate, Encapsulate};
use ml_kem::{KemCore, MlKem768};
use rand::RngCore;
use sha2::{Digest, Sha256};
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
/// HKDF info for the key that encrypts an invite's note.
pub const NOTE_INFO: &[u8] = b"olb.invite.note.v1";
/// HKDF info for the key an invite code derives (docs/04 §5).
pub const JOIN_INFO: &[u8] = b"olb-join-v1";
/// The only invite version this build writes or accepts.
pub const INVITE_VERSION: u32 = 1;
/// ML-KEM-768 ciphertext length.
pub const KEM_CT_LEN: usize = 1088;
/// Bytes of entropy in a one-time code: 80 bits (docs/04 §5).
pub const CODE_BYTES: usize = 10;
/// docs/04 §5: join requests are spread over the same 16 buckets as inboxes.
pub const JOIN_BUCKETS: u8 = 16;

type Ek = <MlKem768 as KemCore>::EncapsulationKey;

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
    let ek: Ek = crate::identity::encapsulation_key(recipient_seal_pk)?;
    let (kem_ct, ss) = ek
        .encapsulate(&mut rand::rngs::OsRng)
        .map_err(|e| KeyError::Malformed(format!("ml-kem encapsulation failed: {e:?}")))?;

    let label = scope_label(&scope, version);
    let wrap = wrap_key(ss.as_slice(), &label)?;
    let wrapped = vault::seal(&wrap, &label, key.as_ref())?;

    Ok(SealedKey {
        scope,
        version,
        kem_ct: kem_ct.as_slice().to_vec(),
        wrapped,
    })
}

/// Unseal a key addressed to us.
pub fn unseal_key(seal_sk: &[u8], sealed: &SealedKey) -> Result<ContentKey> {
    let ss = decapsulate(seal_sk, &sealed.kem_ct)?;
    let label = scope_label(&sealed.scope, sealed.version);
    let wrap = wrap_key(&ss, &label)?;
    // ML-KEM is implicitly rejecting: a wrong decapsulation key yields a
    // different shared secret rather than an error, so "wrong key" and
    // "tampered with" both surface here, as an AEAD failure.
    let plain = Zeroizing::new(vault::open(&wrap, &label, &sealed.wrapped)?);
    let arr: [u8; 32] = plain
        .as_slice()
        .try_into()
        .map_err(|_| KeyError::Malformed("a wrapped content key is not 32 bytes".into()))?;
    Ok(Zeroizing::new(arr))
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
    if project_id.trim().is_empty() {
        return Err(KeyError::Refused("an invite needs a project id".into()));
    }
    if expiry == 0 {
        return Err(KeyError::Refused(
            "an invite needs an expiry; docs/04 §4 defaults to 14 days".into(),
        ));
    }
    check_grant(role, idea_ids, keys.iter().map(|(s, _, _)| s))?;

    let mut invite_id_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut invite_id_bytes);
    let invite_id = hex::encode(invite_id_bytes);

    let mut sealed = Vec::with_capacity(keys.len());
    for (scope, version, key) in keys {
        sealed.push(seal_key(&invitee.seal_pk, scope.clone(), *version, key)?);
    }

    let note = seal_note(&invitee.seal_pk, &invite_id, note)?;

    let mut env = InviteEnvelope {
        v: INVITE_VERSION,
        invite_id,
        project_id: project_id.to_string(),
        project_name: project_name.to_string(),
        role,
        idea_ids: idea_ids.to_vec(),
        keys: sealed,
        inviter: hex::encode(inviter.public.sig_pk.as_slice()),
        note,
        expiry,
        sig: String::new(),
    };
    env.sig = hex::encode(inviter.signing.sign(&invite_bytes(&env)).to_bytes());
    Ok(env)
}

/// Verify an incoming invite's signature and expiry.
///
/// Verification is NOT acceptance. Never auto-accept: the UI must show the
/// inviter's fingerprint and wait for the user, because a signature only
/// proves who sent it, not that the user wanted it (docs/04 §9).
///
/// Two things this deliberately does not do. It does not unseal anything —
/// the return type is `()`, so a successful verification hands the caller no
/// key material and cannot be mistaken for a grant. And it does not consult
/// any list of invites already seen: replay protection is `invite_id` plus
/// expiry held by the caller's store (docs/04 §9), and `invite_id` is inside
/// the signed bytes so it cannot be swapped for a fresh one.
pub fn verify_invite(env: &InviteEnvelope, now: u64) -> Result<()> {
    if env.v != INVITE_VERSION {
        return Err(KeyError::Malformed(format!(
            "invite version {} is not supported",
            env.v
        )));
    }

    // Signature first, before any field is trusted and before anything is
    // decrypted: a forged invite must be rejected without its ciphertext
    // ever reaching a cipher.
    let pk = hex::decode(&env.inviter)
        .map_err(|e| KeyError::Malformed(format!("inviter key is not hex: {e}")))?;
    let vk = crate::identity::verifying_key(&pk)?;
    if vk.is_weak() {
        return Err(KeyError::Malformed(
            "inviter key is a small-order point, under which anyone can forge".into(),
        ));
    }
    let sig_bytes = hex::decode(&env.sig)
        .map_err(|e| KeyError::Malformed(format!("invite signature is not hex: {e}")))?;
    let sig = crate::identity::signature(&sig_bytes)?;
    vk.verify_strict(&invite_bytes(env), &sig).map_err(|_| {
        KeyError::Malformed("invite signature does not verify under the inviter's key".into())
    })?;

    if now > env.expiry {
        return Err(KeyError::Refused(format!(
            "this invite expired at {} (it is now {now})",
            env.expiry
        )));
    }

    // Structural rules, checked even though the invite is correctly signed:
    // a lead who seals an idea key the invite does not declare is handing
    // out access that no history records.
    check_grant(
        env.role,
        &env.idea_ids,
        env.keys.iter().map(|k| &k.scope),
    )?;
    for k in &env.keys {
        if k.kem_ct.len() != KEM_CT_LEN {
            return Err(KeyError::Malformed(format!(
                "sealed key for {} has a {}-byte KEM ciphertext, expected {KEM_CT_LEN}",
                scope_label(&k.scope, k.version),
                k.kem_ct.len()
            )));
        }
    }
    Ok(())
}

/// Unseal every key in an accepted invite.
///
/// Called only after a human has accepted. It re-verifies first, so no code
/// path reaches a decapsulation without a valid signature behind it.
pub fn accept_invite(
    me: &crate::identity::SecretIdentity,
    env: &InviteEnvelope,
) -> Result<Vec<(KeyScope, u32, ContentKey)>> {
    verify_invite(env, crate::identity::now_unix())?;
    let mut out = Vec::with_capacity(env.keys.len());
    for sealed in &env.keys {
        // Every key must open. One that does not means the invite was sealed
        // to somebody else, and a partial accept would leave a member who
        // believes they have access to an idea they cannot read.
        out.push((
            sealed.scope.clone(),
            sealed.version,
            unseal_key(&me.seal_sk, sealed)?,
        ));
    }
    Ok(out)
}

/// Read the invite's note. Separate from `accept_invite` because the note is
/// what the UI shows *before* the user decides (docs/04 §9), and deciding is
/// the only thing that may install a key.
pub fn open_note(me: &SecretIdentity, env: &InviteEnvelope) -> Result<String> {
    if env.note.len() < KEM_CT_LEN {
        return Err(KeyError::Malformed("invite note is truncated".into()));
    }
    let (kem_ct, body) = env.note.split_at(KEM_CT_LEN);
    let ss = decapsulate(&me.seal_sk, kem_ct)?;
    let aad = note_label(&env.invite_id);
    let key = note_key(&ss, &aad)?;
    let plain = vault::open(&key, &aad, body)?;
    String::from_utf8(plain).map_err(|e| KeyError::Malformed(format!("note is not UTF-8: {e}")))
}

/// Signed receipt the invitee publishes back to the project topic.
pub fn sign_accept(
    me: &crate::identity::SecretIdentity,
    invite_id: &str,
) -> Result<String> {
    Ok(hex::encode(
        me.signing.sign(&accept_bytes(invite_id)).to_bytes(),
    ))
}

/// Check a receipt. The inviter calls this before writing `member_add`:
/// docs/11 §4 makes consent structural, because without it a lead could name
/// a researcher as a member — and therefore as an author on a publication —
/// who never agreed.
pub fn verify_accept(sig_pk_hex: &str, invite_id: &str, sig_hex: &str) -> Result<()> {
    let pk = hex::decode(sig_pk_hex)
        .map_err(|e| KeyError::Malformed(format!("member key is not hex: {e}")))?;
    let vk = crate::identity::verifying_key(&pk)?;
    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| KeyError::Malformed(format!("accept signature is not hex: {e}")))?;
    let sig = crate::identity::signature(&sig_bytes)?;
    vk.verify_strict(&accept_bytes(invite_id), &sig)
        .map_err(|_| KeyError::Malformed("acceptance signature does not verify".into()))
}

/// A one-time invite code (docs/04 §5).
///
/// The code carries NO project data — it only lets the holder's node find
/// the inviter's. Everything else happens after the lead has compared
/// fingerprints and approved.
///
/// So `me` and `project_id` are not in the code and cannot be: they identify
/// the record the caller stores next to it (`{code_hash, role, idea_ids,
/// expiry, uses_left}` — one row per lead per project), and the returned
/// string is 80 random bits and nothing else. They are validated here so a
/// caller cannot create a code with nowhere to file it.
pub fn make_code(me: &crate::identity::SecretIdentity, project_id: &str) -> Result<String> {
    if project_id.trim().is_empty() {
        return Err(KeyError::Refused("a code needs a project id".into()));
    }
    if me.public.sig_pk.len() != crate::identity::SIG_PK_LEN {
        return Err(KeyError::Malformed("inviter identity is malformed".into()));
    }
    let mut raw = [0u8; CODE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    Ok(group_code(&crockford().encode(&raw)))
}

/// Parse a redeemed code.
///
/// Returns `(canonical code, tag)` where the tag is
/// `hex(SHA-256(canonical code))` — what a `JoinRequest` carries and what
/// the lead matches against its stored `code_hash` (docs/04 §5). The code
/// itself never goes on the wire.
///
/// Accepts what a human will actually type: lower case, missing dashes,
/// spaces, and the Crockford confusions `I`/`L` for `1` and `O` for `0`.
pub fn parse_code(code: &str) -> Result<(String, String)> {
    let mut body = String::new();
    for c in code.trim().chars() {
        match c {
            '-' | ' ' | '_' => continue,
            c => body.push(c.to_ascii_uppercase()),
        }
    }
    let body = body.strip_prefix("OLB").unwrap_or(&body).to_string();
    let normalised: String = body
        .chars()
        .map(|c| match c {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        })
        .collect();

    let raw = crockford()
        .decode(normalised.as_bytes())
        .map_err(|_| KeyError::Malformed("that is not an Open Labbook invite code".into()))?;
    if raw.len() != CODE_BYTES {
        return Err(KeyError::Malformed(format!(
            "an invite code carries {CODE_BYTES} bytes, this one carries {}",
            raw.len()
        )));
    }
    let canonical = group_code(&crockford().encode(&raw));
    let tag = hex::encode(Sha256::digest(canonical.as_bytes()));
    Ok((canonical, tag))
}

/// Does this typed code match a stored tag? Constant time, per docs/04 §9.
pub fn code_matches(code: &str, stored_tag_hex: &str) -> bool {
    match parse_code(code) {
        Ok((_, tag)) => ct_eq(tag.as_bytes(), stored_tag_hex.as_bytes()),
        Err(_) => false,
    }
}

/// The join topic a code rendezvouses on (docs/04 §5, docs/11 §3.2).
pub fn join_topic(code: &str) -> Result<String> {
    let (_, tag) = parse_code(code)?;
    let first = hex::decode(&tag[..2])
        .map_err(|e| KeyError::Malformed(e.to_string()))?[0];
    Ok(format!("/olb/1/join-{}/proto", first % JOIN_BUCKETS))
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
///
/// The new keys are returned only in sealed form, even to the caller. The
/// lead is itself one of `remaining`, so it unseals its own grants like
/// everybody else, and no plaintext project or idea key crosses this
/// boundary where a UI-facing method could pick one up.
pub fn rotate_after_removal(
    removed: &Member,
    remaining: &[Member],
    current_versions: &[(KeyScope, u32)],
) -> Result<Vec<(Member, Vec<SealedKey>)>> {
    let version_of = |scope: &KeyScope| -> Option<u32> {
        current_versions
            .iter()
            .find(|(s, _)| s == scope)
            .map(|(_, v)| *v)
    };

    if remaining.iter().any(|m| m.sig_pk == removed.sig_pk) {
        return Err(KeyError::Refused(
            "the removed member is still in the remaining list".into(),
        ));
    }
    // docs/11 §4: a project must always have at least one Lead, or nobody
    // can administer it. Rotation is where that becomes irreversible, so it
    // is refused here too rather than only in the membership validator.
    if removed.role == Role::Lead
        && !remaining
            .iter()
            .any(|m| m.role == Role::Lead && m.removed_at.is_none())
    {
        return Err(KeyError::Refused(
            "that is the last lead; a project with no lead cannot be administered".into(),
        ));
    }

    // Exactly the scopes the removed member held: project metadata, which
    // every member has, plus their ideas and nothing else. Rotating an idea
    // they never had access to protects nothing and tells anyone watching
    // version numbers that something changed for that idea.
    let mut scopes = vec![KeyScope::ProjectMeta];
    for idea in &removed.ideas {
        let scope = KeyScope::Idea(idea.clone());
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }

    let mut fresh: Vec<(KeyScope, u32, ContentKey)> = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let current = version_of(&scope).ok_or_else(|| {
            KeyError::Refused(format!(
                "no current version for {} — refusing to rotate a key that does not exist",
                scope_label(&scope, 0)
            ))
        })?;
        let mut key: ContentKey = Zeroizing::new([0u8; 32]);
        rand::rngs::OsRng.fill_bytes(key.as_mut());
        fresh.push((scope, current.saturating_add(1), key));
    }

    let mut out = Vec::with_capacity(remaining.len());
    for member in remaining {
        if member.removed_at.is_some() {
            continue;
        }
        let mut grants = Vec::new();
        // A replica pins ciphertext and gets no content keys at all — not
        // even the metadata key (docs/04 §8). It still appears here, with an
        // empty grant, so the caller can see it was considered rather than
        // forgotten.
        if member.role != Role::Replica {
            for (scope, version, key) in &fresh {
                let entitled = match scope {
                    KeyScope::ProjectMeta => true,
                    KeyScope::Idea(id) => member.ideas.contains(id),
                };
                if entitled {
                    grants.push(seal_key(&member.seal_pk, scope.clone(), *version, key)?);
                }
            }
        }
        out.push((member.clone(), grants));
    }
    Ok(out)
}

/// May this key write records to this project at this moment?
///
/// Implements `olb_vcs::dag::MemberPolicy` for the real membership state, so
/// the DAG can enforce roles without depending on this crate.
///
/// `members` is a **replay of the membership record series** as of one point
/// in history (docs/11 §4), not a stored table. The `at_commit` argument of
/// the trait is therefore advisory here: this struct already *is* the answer
/// for whatever commit the caller replayed it at, and it cannot re-derive
/// another one. The caller must build it at the right point — the parent of
/// the commit being validated, or the merge base for an approval — because
/// a policy evaluated against the wrong state gives a deterministic answer
/// to the wrong question.
pub struct Membership {
    pub members: Vec<Member>,
    pub approvals_required: std::collections::BTreeMap<String, usize>,
}

impl Membership {
    /// The active member for a key, if any. Hex is compared case-insensitively
    /// because a commit author and a membership record are written by
    /// different code paths and only one of them has to slip.
    pub fn active(&self, key_hex: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| m.removed_at.is_none() && m.sig_pk.eq_ignore_ascii_case(key_hex))
    }
}

impl olb_vcs::dag::MemberPolicy for Membership {
    fn may_write(&self, author_hex: &str, _at_commit: Option<&str>) -> bool {
        matches!(
            self.active(author_hex).map(|m| m.role),
            Some(Role::Lead) | Some(Role::Contributor)
        )
    }

    fn may_approve(&self, key_hex: &str, _at_commit: Option<&str>) -> bool {
        // docs/05 §3 and docs/10 §4.5: reviewers cannot approve merges, and a
        // replica is not a person. Distinctness from the merge author and
        // staleness are the DAG's job, not this one's.
        matches!(
            self.active(key_hex).map(|m| m.role),
            Some(Role::Lead) | Some(Role::Contributor)
        )
    }

    fn approvals_required(&self, branch: &str) -> usize {
        // Snapshotted per branch (docs/10 §4.5). A branch nobody protected
        // is unprotected, which is 0 — the same answer `OpenPolicy` gives.
        self.approvals_required.get(branch).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------- internals

/// Names a key and its version, for both the HKDF salt and the AEAD AAD.
///
/// `project-meta:` and `idea:` can never collide, and the numeric version is
/// delimited, so no idea id can spell another scope's label.
fn scope_label(scope: &KeyScope, version: u32) -> String {
    match scope {
        KeyScope::ProjectMeta => format!("olb.key.project-meta:{version}"),
        KeyScope::Idea(id) => format!("olb.key.idea:{version}:{id}"),
    }
}

fn note_label(invite_id: &str) -> String {
    format!("olb.invite.note:{invite_id}")
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Lead => "lead",
        Role::Contributor => "contributor",
        Role::Reviewer => "reviewer",
        Role::Replica => "replica",
    }
}

fn wrap_key(ss: &[u8], label: &str) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0u8; 32]);
    crate::identity::expand(ss, label.as_bytes(), WRAP_INFO, key.as_mut())?;
    Ok(key)
}

fn note_key(ss: &[u8], label: &str) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0u8; 32]);
    crate::identity::expand(ss, label.as_bytes(), NOTE_INFO, key.as_mut())?;
    Ok(key)
}

fn decapsulate(seal_sk: &[u8], kem_ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let dk = crate::identity::decapsulation_key(seal_sk)?;
    let ct = ml_kem::Ciphertext::<MlKem768>::try_from(kem_ct).map_err(|_| {
        KeyError::Malformed(format!(
            "KEM ciphertext is {} bytes, expected {KEM_CT_LEN}",
            kem_ct.len()
        ))
    })?;
    let ss = dk
        .decapsulate(&ct)
        .map_err(|e| KeyError::Decrypt(format!("ml-kem decapsulation failed: {e:?}")))?;
    Ok(Zeroizing::new(ss.as_slice().to_vec()))
}

fn seal_note(recipient_seal_pk: &[u8], invite_id: &str, note: &str) -> Result<Vec<u8>> {
    let ek: Ek = crate::identity::encapsulation_key(recipient_seal_pk)?;
    let (kem_ct, ss) = ek
        .encapsulate(&mut rand::rngs::OsRng)
        .map_err(|e| KeyError::Malformed(format!("ml-kem encapsulation failed: {e:?}")))?;
    let aad = note_label(invite_id);
    let key = note_key(ss.as_slice(), &aad)?;
    let mut out = kem_ct.as_slice().to_vec();
    out.extend_from_slice(&vault::seal(&key, &aad, note.as_bytes())?);
    Ok(out)
}

/// The bytes an invite signature covers: every public field, each length
/// prefixed so no two different field splits produce the same buffer.
fn invite_bytes(env: &InviteEnvelope) -> Vec<u8> {
    let mut out = Vec::with_capacity(INVITE_DOMAIN.len() + 256 + env.note.len());
    out.extend_from_slice(INVITE_DOMAIN);
    out.extend_from_slice(&env.v.to_be_bytes());
    push_len_prefixed(&mut out, env.invite_id.as_bytes());
    push_len_prefixed(&mut out, env.project_id.as_bytes());
    push_len_prefixed(&mut out, env.project_name.as_bytes());
    push_len_prefixed(&mut out, role_label(env.role).as_bytes());
    out.extend_from_slice(&(env.idea_ids.len() as u32).to_be_bytes());
    for id in &env.idea_ids {
        push_len_prefixed(&mut out, id.as_bytes());
    }
    out.extend_from_slice(&(env.keys.len() as u32).to_be_bytes());
    for k in &env.keys {
        push_len_prefixed(&mut out, scope_label(&k.scope, k.version).as_bytes());
        out.extend_from_slice(&k.version.to_be_bytes());
        push_len_prefixed(&mut out, &k.kem_ct);
        push_len_prefixed(&mut out, &k.wrapped);
    }
    push_len_prefixed(&mut out, env.inviter.as_bytes());
    push_len_prefixed(&mut out, &env.note);
    out.extend_from_slice(&env.expiry.to_be_bytes());
    out
}

fn accept_bytes(invite_id: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(ACCEPT_DOMAIN.len() + 4 + invite_id.len());
    out.extend_from_slice(ACCEPT_DOMAIN);
    push_len_prefixed(&mut out, invite_id.as_bytes());
    out
}

/// Every key an invite seals must be one the invite declares, and every idea
/// it declares must come with a key. Either half missing is a silent access
/// bug: a key nobody recorded, or a member who believes they can read an
/// idea and cannot.
fn check_grant<'a>(
    role: Role,
    idea_ids: &[String],
    scopes: impl Iterator<Item = &'a KeyScope>,
) -> Result<()> {
    let scopes: Vec<&KeyScope> = scopes.collect();
    let mut seen: Vec<&KeyScope> = Vec::with_capacity(scopes.len());
    for s in &scopes {
        if seen.contains(s) {
            return Err(KeyError::Refused(format!(
                "{} is sealed twice in one invite",
                scope_label(s, 0)
            )));
        }
        seen.push(s);
    }

    if role == Role::Replica {
        // docs/04 §8. A replica serves ciphertext and must not be able to
        // read a word of it.
        if !scopes.is_empty() || !idea_ids.is_empty() {
            return Err(KeyError::Refused(
                "a replica node receives no content keys at all (docs/04 §8)".into(),
            ));
        }
        return Ok(());
    }

    if !scopes.contains(&&KeyScope::ProjectMeta) {
        return Err(KeyError::Refused(
            "every member needs the project metadata key".into(),
        ));
    }
    for s in &scopes {
        if let KeyScope::Idea(id) = s {
            if !idea_ids.contains(id) {
                return Err(KeyError::Refused(format!(
                    "idea '{id}' is sealed but not in the invite's access list"
                )));
            }
        }
    }
    for id in idea_ids {
        if !scopes.contains(&&KeyScope::Idea(id.clone())) {
            return Err(KeyError::Refused(format!(
                "idea '{id}' is in the access list but no key is sealed for it"
            )));
        }
    }
    Ok(())
}

/// Constant-time equality (docs/04 §9).
///
/// Both sides are hashed first so the comparison is always over 32 bytes,
/// and the comparison itself is `digest`'s `CtOutput`, whose `PartialEq` is
/// `subtle`'s `ct_eq`. That keeps this to audited code without adding
/// `subtle` as a direct dependency — `Cargo.toml` is fixed for this change —
/// and without a hand-rolled compare loop an optimiser may short-circuit.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    use hkdf::hmac::digest::CtOutput;
    let (ha, hb) = (Sha256::digest(a), Sha256::digest(b));
    CtOutput::<Sha256>::new(ha) == CtOutput::<Sha256>::new(hb)
}

/// Crockford base32: no `I`, `L`, `O` or `U`, so a code read aloud or
/// written down survives the trip (docs/11 §3.2).
fn crockford() -> &'static data_encoding::Encoding {
    static ENC: std::sync::OnceLock<data_encoding::Encoding> = std::sync::OnceLock::new();
    ENC.get_or_init(|| {
        let mut spec = data_encoding::Specification::new();
        spec.symbols.push_str("0123456789ABCDEFGHJKMNPQRSTVWXYZ");
        spec.encoding()
            .expect("Crockford base32 is 32 distinct symbols")
    })
}

/// `OLB-7Q4M-K2XD-93RB-XXXX` — groups of four, for reading out loud.
fn group_code(symbols: &str) -> String {
    let groups: Vec<String> = symbols
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect();
    format!("OLB-{}", groups.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{self, PublicIdentity};

    const DAY: u64 = 86_400;

    fn who(name: &str) -> SecretIdentity {
        identity::generate(name, None).unwrap()
    }

    fn key(b: u8) -> ContentKey {
        Zeroizing::new([b; 32])
    }

    fn member(id: &SecretIdentity, role: Role, ideas: &[&str]) -> Member {
        Member {
            sig_pk: hex::encode(id.public.sig_pk.as_slice()),
            seal_pk: id.public.seal_pk.clone(),
            name: id.public.name.clone(),
            role,
            ideas: ideas.iter().map(|s| s.to_string()).collect(),
            added_at: 1_761_500_000,
            removed_at: None,
        }
    }

    fn five_ideas() -> Vec<String> {
        ["lid", "comp", "assay", "stats", "writeup"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// alice invites carol as a reviewer with 2 of 5 ideas.
    fn reviewer_invite(
        alice: &SecretIdentity,
        carol: &PublicIdentity,
    ) -> (InviteEnvelope, Vec<(KeyScope, u32, ContentKey)>) {
        let granted = vec![
            (KeyScope::ProjectMeta, 3, key(1)),
            (KeyScope::Idea("lid".into()), 2, key(2)),
            (KeyScope::Idea("comp".into()), 7, key(3)),
        ];
        let env = build_invite(
            alice,
            carol,
            "p-1",
            "Lipid nanoparticle delivery",
            Role::Reviewer,
            &["lid".to_string(), "comp".to_string()],
            &granted,
            "Happy to have you review these two.",
            identity::now_unix() + 14 * DAY,
        )
        .unwrap();
        (env, granted)
    }

    // ------------------------------------------------ seal and unseal keys

    #[test]
    fn a_sealed_key_round_trips() {
        let carol = who("Carol");
        let k = key(9);
        let sealed = seal_key(&carol.public.seal_pk, KeyScope::ProjectMeta, 3, &k).unwrap();
        assert_eq!(sealed.kem_ct.len(), KEM_CT_LEN);
        assert_eq!(*unseal_key(&carol.seal_sk, &sealed).unwrap(), *k);
    }

    #[test]
    fn the_wrong_key_cannot_unseal() {
        let (carol, bob) = (who("Carol"), who("Bob"));
        let sealed = seal_key(&carol.public.seal_pk, KeyScope::ProjectMeta, 3, &key(9)).unwrap();
        let err = unseal_key(&bob.seal_sk, &sealed).unwrap_err();
        assert!(matches!(err, KeyError::Decrypt(_)), "got {err:?}");
    }

    #[test]
    fn the_content_key_is_not_in_the_ciphertext_and_the_kem_secret_is_not_the_cipher_key() {
        let carol = who("Carol");
        let k = key(0xAB);
        let sealed = seal_key(&carol.public.seal_pk, KeyScope::ProjectMeta, 1, &k).unwrap();
        assert!(!sealed
            .wrapped
            .windows(32)
            .any(|w| w == k.as_ref() as &[u8]));
        // The wrapping key is HKDF of the shared secret, never the secret.
        let ss = decapsulate(&carol.seal_sk, &sealed.kem_ct).unwrap();
        let label = scope_label(&KeyScope::ProjectMeta, 1);
        assert_ne!(&ss[..], wrap_key(&ss, &label).unwrap().as_ref() as &[u8]);
        assert!(vault::open(
            &ss.as_slice().try_into().unwrap(),
            &label,
            &sealed.wrapped
        )
        .is_err());
    }

    #[test]
    fn a_sealed_key_cannot_be_relabelled() {
        // The AAD binds scope and version, so an idea key cannot be passed
        // off as the project metadata key, nor as a later version.
        let carol = who("Carol");
        let sealed = seal_key(
            &carol.public.seal_pk,
            KeyScope::Idea("comp".into()),
            7,
            &key(3),
        )
        .unwrap();

        let mut as_meta = sealed.clone();
        as_meta.scope = KeyScope::ProjectMeta;
        assert!(unseal_key(&carol.seal_sk, &as_meta).is_err());

        let mut bumped = sealed.clone();
        bumped.version = 8;
        assert!(unseal_key(&carol.seal_sk, &bumped).is_err());

        let mut other_idea = sealed;
        other_idea.scope = KeyScope::Idea("lid".into());
        assert!(unseal_key(&carol.seal_sk, &other_idea).is_err());
    }

    // ------------------------------------------------------------- invites

    #[test]
    fn an_invite_seals_n_keys_and_round_trips() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let (env, granted) = reviewer_invite(&alice, &carol.public);

        assert_eq!(env.keys.len(), 3, "project metadata plus two ideas");
        verify_invite(&env, identity::now_unix()).unwrap();

        let got = accept_invite(&carol, &env).unwrap();
        assert_eq!(got.len(), granted.len());
        for (scope, version, k) in &granted {
            let (_, gv, gk) = got
                .iter()
                .find(|(s, _, _)| s == scope)
                .expect("every granted scope arrives");
            assert_eq!(gv, version);
            assert_eq!(**gk, **k, "{scope:?} must match byte for byte");
        }
    }

    #[test]
    fn a_reviewer_with_two_of_five_ideas_gets_exactly_those_two() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let (env, _) = reviewer_invite(&alice, &carol.public);
        let got = accept_invite(&carol, &env).unwrap();

        let ideas: Vec<&String> = got
            .iter()
            .filter_map(|(s, _, _)| match s {
                KeyScope::Idea(id) => Some(id),
                KeyScope::ProjectMeta => None,
            })
            .collect();
        assert_eq!(ideas, vec!["lid", "comp"]);
        // The project has five ideas; the invite declared two.
        let all = five_ideas();
        assert_eq!(all.len(), 5);
        assert_eq!(env.idea_ids.len(), 2);
        // The other three are independent keys, so holding the project
        // metadata key derives nothing: this is the amended hierarchy.
        for absent in all.iter().filter(|i| !env.idea_ids.contains(i)) {
            assert!(!got
                .iter()
                .any(|(s, _, _)| s == &KeyScope::Idea(absent.clone())));
        }
    }

    #[test]
    fn an_invite_cannot_seal_a_key_it_does_not_declare() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let undeclared = vec![
            (KeyScope::ProjectMeta, 3, key(1)),
            (KeyScope::Idea("stats".into()), 1, key(4)),
        ];
        let err = build_invite(
            &alice,
            &carol.public,
            "p-1",
            "P",
            Role::Reviewer,
            &["lid".to_string()],
            &undeclared,
            "",
            identity::now_unix() + DAY,
        )
        .unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");
    }

    #[test]
    fn an_invite_must_carry_a_key_for_every_idea_it_declares() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let err = build_invite(
            &alice,
            &carol.public,
            "p-1",
            "P",
            Role::Reviewer,
            &["lid".to_string(), "comp".to_string()],
            &[
                (KeyScope::ProjectMeta, 3, key(1)),
                (KeyScope::Idea("lid".into()), 1, key(2)),
            ],
            "",
            identity::now_unix() + DAY,
        )
        .unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");
    }

    #[test]
    fn a_replica_gets_no_content_keys_at_all() {
        let (alice, replica) = (who("Alice"), who("replica-1"));
        let now = identity::now_unix();

        // Even the project metadata key is refused.
        let err = build_invite(
            &alice,
            &replica.public,
            "p-1",
            "P",
            Role::Replica,
            &[],
            &[(KeyScope::ProjectMeta, 3, key(1))],
            "",
            now + DAY,
        )
        .unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");

        // A grant with no keys is what a replica actually receives.
        let env = build_invite(
            &alice,
            &replica.public,
            "p-1",
            "P",
            Role::Replica,
            &[],
            &[],
            "pin this project",
            now + DAY,
        )
        .unwrap();
        verify_invite(&env, now).unwrap();
        assert!(accept_invite(&replica, &env).unwrap().is_empty());
        assert_eq!(open_note(&replica, &env).unwrap(), "pin this project");
    }

    #[test]
    fn an_invite_tampered_in_any_field_fails_verification() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let (env, _) = reviewer_invite(&alice, &carol.public);
        let now = identity::now_unix();

        type Mutation = (&'static str, fn(&mut InviteEnvelope));
        let mutations: Vec<Mutation> = vec![
            ("v", |e| e.v = 2),
            ("invite_id", |e| e.invite_id = "00".repeat(16)),
            ("project_id", |e| e.project_id = "p-2".into()),
            ("project_name", |e| e.project_name = "Other project".into()),
            ("role", |e| e.role = Role::Lead),
            ("idea_ids", |e| e.idea_ids.push("stats".into())),
            ("key scope", |e| {
                e.keys[1].scope = KeyScope::Idea("stats".into())
            }),
            ("key version", |e| e.keys[0].version += 1),
            ("kem_ct", |e| e.keys[0].kem_ct[0] ^= 1),
            ("wrapped", |e| e.keys[0].wrapped[0] ^= 1),
            ("note", |e| e.note[0] ^= 1),
            ("expiry", |e| e.expiry += DAY),
            ("sig", |e| {
                e.sig = format!("{:0>128}", "");
            }),
            ("inviter", |e| {
                e.inviter = "11".repeat(32);
            }),
        ];
        for (field, mutate) in mutations {
            let mut bad = env.clone();
            mutate(&mut bad);
            assert!(
                verify_invite(&bad, now).is_err(),
                "tampering with {field} was not detected"
            );
        }
        // And the untouched original still verifies, so the loop is not
        // passing because everything fails.
        verify_invite(&env, now).unwrap();
    }

    #[test]
    fn an_invite_signed_by_someone_else_is_rejected() {
        let (alice, carol, mallory) = (who("Alice"), who("Carol"), who("Mallory"));
        let (env, _) = reviewer_invite(&alice, &carol.public);

        // Mallory re-signs alice's envelope under her own key and claims to
        // be alice: the signature no longer matches the claimed inviter.
        let mut forged = env.clone();
        forged.sig = hex::encode(mallory.signing.sign(&invite_bytes(&env)).to_bytes());
        assert!(verify_invite(&forged, identity::now_unix()).is_err());

        // Or she claims her own key, which is honest but no longer alice —
        // the fingerprint the user compared is alice's.
        let mut relabelled = env;
        relabelled.inviter = hex::encode(mallory.public.sig_pk.as_slice());
        relabelled.sig = hex::encode(
            mallory
                .signing
                .sign(&invite_bytes(&relabelled))
                .to_bytes(),
        );
        verify_invite(&relabelled, identity::now_unix()).unwrap();
        assert_ne!(relabelled.inviter, hex::encode(alice.public.sig_pk));
    }

    #[test]
    fn an_expired_invite_is_refused() {
        let (alice, carol) = (who("Alice"), who("Carol"));
        let now = identity::now_unix();
        let env = build_invite(
            &alice,
            &carol.public,
            "p-1",
            "P",
            Role::Contributor,
            &[],
            &[(KeyScope::ProjectMeta, 3, key(1))],
            "",
            now - 1,
        )
        .unwrap();
        let err = verify_invite(&env, now).unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");
        // Accepting goes through the same gate, so an expired invite cannot
        // install a key by skipping verification.
        assert!(accept_invite(&carol, &env).is_err());
    }

    #[test]
    fn verifying_an_invite_is_not_accepting_it() {
        // Bob can verify an invite addressed to carol — a signature is
        // public — and that gets him nothing. Verification proves who sent
        // it, never that anyone wanted it.
        let (alice, carol, bob) = (who("Alice"), who("Carol"), who("Bob"));
        let (env, _) = reviewer_invite(&alice, &carol.public);

        // The success type is `()`: a verification cannot hand anyone a key.
        let _verified: () = verify_invite(&env, identity::now_unix()).unwrap();
        assert!(accept_invite(&bob, &env).is_err());
        assert!(open_note(&bob, &env).is_err());

        // And the invitee's own decision is a separate, signed act.
        let receipt = sign_accept(&carol, &env.invite_id).unwrap();
        verify_accept(
            &hex::encode(carol.public.sig_pk.as_slice()),
            &env.invite_id,
            &receipt,
        )
        .unwrap();
    }

    #[test]
    fn a_receipt_is_bound_to_its_invite_and_its_signer() {
        let (carol, bob) = (who("Carol"), who("Bob"));
        let receipt = sign_accept(&carol, "abc123").unwrap();
        let carol_pk = hex::encode(carol.public.sig_pk.as_slice());
        verify_accept(&carol_pk, "abc123", &receipt).unwrap();
        assert!(verify_accept(&carol_pk, "abc124", &receipt).is_err());
        assert!(verify_accept(
            &hex::encode(bob.public.sig_pk.as_slice()),
            "abc123",
            &receipt
        )
        .is_err());
    }

    #[test]
    fn the_note_is_readable_only_by_the_invitee() {
        let (alice, carol, bob) = (who("Alice"), who("Carol"), who("Bob"));
        let (env, _) = reviewer_invite(&alice, &carol.public);
        assert_eq!(
            open_note(&carol, &env).unwrap(),
            "Happy to have you review these two."
        );
        assert!(open_note(&bob, &env).is_err());
        // The note is not in the clear on the bucket topic.
        let wire = serde_json::to_vec(&env).unwrap();
        assert!(!wire
            .windows(5)
            .any(|w| w == b"Happy"));
    }

    // --------------------------------------------------------------- codes

    #[test]
    fn a_code_round_trips_through_what_a_human_types() {
        let alice = who("Alice");
        let code = make_code(&alice, "p-1").unwrap();
        assert!(code.starts_with("OLB-"), "{code}");
        let (canonical, tag) = parse_code(&code).unwrap();
        assert_eq!(canonical, code);
        assert_eq!(tag.len(), 64);
        // 10 bytes of entropy, in groups of four symbols.
        let symbols: String = code.trim_start_matches("OLB-").replace('-', "");
        assert_eq!(symbols.len(), 16, "80 bits: {code}");
        assert!(!symbols.contains(['I', 'L', 'O', 'U']), "{code}");

        // Lower case, no dashes, and the Crockford confusions all land on
        // the same tag.
        let typed = symbols.to_lowercase();
        assert_eq!(parse_code(&typed).unwrap(), (canonical.clone(), tag.clone()));
        assert!(code_matches(&format!("olb {typed}"), &tag));
    }

    #[test]
    fn a_one_character_wrong_code_matches_nothing() {
        let alice = who("Alice");
        let code = make_code(&alice, "p-1").unwrap();
        let (_, tag) = parse_code(&code).unwrap();

        let mut chars: Vec<char> = code.chars().collect();
        let last = chars.len() - 1;
        chars[last] = if chars[last] == '7' { '8' } else { '7' };
        let wrong: String = chars.into_iter().collect();

        assert!(!code_matches(&wrong, &tag));
        assert!(!code_matches("OLB-0000-0000-0000-0000", &tag));
        assert!(!code_matches("not a code", &tag));
        assert!(code_matches(&code, &tag));
    }

    #[test]
    fn two_codes_are_never_the_same_and_a_code_carries_no_project_data() {
        let alice = who("Alice");
        let a = make_code(&alice, "p-1").unwrap();
        let b = make_code(&alice, "p-1").unwrap();
        assert_ne!(a, b, "a code is 80 random bits, not a function of inputs");
        assert!(!a.contains("p-1"));
        assert!(make_code(&alice, "  ").is_err());
        let t = join_topic(&a).unwrap();
        let bucket: u8 = t
            .trim_start_matches("/olb/1/join-")
            .trim_end_matches("/proto")
            .parse()
            .unwrap();
        assert!(bucket < JOIN_BUCKETS);
    }

    // ------------------------------------------------------------ rotation

    struct Project {
        alice: SecretIdentity,
        bob: SecretIdentity,
        carol: SecretIdentity,
        dave: SecretIdentity,
        replica: SecretIdentity,
    }

    impl Project {
        fn new() -> Self {
            Self {
                alice: who("Alice"),
                bob: who("Bob"),
                carol: who("Carol"),
                dave: who("Dave"),
                replica: who("replica-1"),
            }
        }
    }

    fn versions() -> Vec<(KeyScope, u32)> {
        vec![
            (KeyScope::ProjectMeta, 3),
            (KeyScope::Idea("lid".into()), 2),
            (KeyScope::Idea("comp".into()), 7),
            (KeyScope::Idea("assay".into()), 1),
        ]
    }

    #[test]
    fn rotation_gives_remaining_members_new_keys_and_the_removed_member_nothing() {
        let p = Project::new();
        // dave held project metadata plus two of the three ideas.
        let dave = member(&p.dave, Role::Contributor, &["lid", "comp"]);
        let remaining = vec![
            member(&p.alice, Role::Lead, &["lid", "comp", "assay"]),
            member(&p.bob, Role::Contributor, &["lid"]),
            member(&p.replica, Role::Replica, &[]),
        ];

        let out = rotate_after_removal(&dave, &remaining, &versions()).unwrap();

        // Nobody sealed anything to dave.
        assert!(!out.iter().any(|(m, _)| m.sig_pk == dave.sig_pk));
        for (_, grants) in &out {
            for g in grants {
                assert!(
                    unseal_key(&p.dave.seal_sk, g).is_err(),
                    "the removed member must not be able to open a new grant"
                );
            }
        }

        let grants_for = |id: &SecretIdentity| -> Vec<SealedKey> {
            let pk = hex::encode(id.public.sig_pk.as_slice());
            out.iter()
                .find(|(m, _)| m.sig_pk == pk)
                .map(|(_, g)| g.clone())
                .unwrap()
        };

        // alice held all three ideas, but only the two dave held rotate.
        let alice_grants = grants_for(&p.alice);
        let mut alice_scopes: Vec<(KeyScope, u32)> = alice_grants
            .iter()
            .map(|g| (g.scope.clone(), g.version))
            .collect();
        alice_scopes.sort();
        assert_eq!(
            alice_scopes,
            vec![
                (KeyScope::ProjectMeta, 4),
                (KeyScope::Idea("comp".into()), 8),
                (KeyScope::Idea("lid".into()), 3),
            ]
        );

        // bob held only `lid`, so he gets metadata and `lid` — not `comp`,
        // which he could not read before and still cannot.
        let bob_grants = grants_for(&p.bob);
        let mut bob_scopes: Vec<KeyScope> =
            bob_grants.iter().map(|g| g.scope.clone()).collect();
        bob_scopes.sort();
        assert_eq!(
            bob_scopes,
            vec![KeyScope::ProjectMeta, KeyScope::Idea("lid".into())]
        );

        // Everyone who is entitled to a scope gets the SAME new key, or
        // they could not read each other's new content.
        let alice_meta = alice_grants
            .iter()
            .find(|g| g.scope == KeyScope::ProjectMeta)
            .unwrap();
        let bob_meta = bob_grants
            .iter()
            .find(|g| g.scope == KeyScope::ProjectMeta)
            .unwrap();
        let a = unseal_key(&p.alice.seal_sk, alice_meta).unwrap();
        let b = unseal_key(&p.bob.seal_sk, bob_meta).unwrap();
        assert_eq!(*a, *b);
        // And it is a new key, not the one dave had.
        assert_ne!(*a, *key(1));
        // Sealed to one member only.
        assert!(unseal_key(&p.bob.seal_sk, alice_meta).is_err());
    }

    #[test]
    fn rotation_touches_only_the_scopes_the_removed_member_held() {
        let p = Project::new();
        // dave held `lid` only.
        let dave = member(&p.dave, Role::Contributor, &["lid"]);
        let remaining = vec![
            member(&p.alice, Role::Lead, &["lid", "comp", "assay"]),
            member(&p.carol, Role::Reviewer, &["comp"]),
        ];

        let out = rotate_after_removal(&dave, &remaining, &versions()).unwrap();
        let all: Vec<KeyScope> = out
            .iter()
            .flat_map(|(_, g)| g.iter().map(|k| k.scope.clone()))
            .collect();

        assert!(all.contains(&KeyScope::ProjectMeta));
        assert!(all.contains(&KeyScope::Idea("lid".into())));
        // `comp` and `assay` are NOT rotated: dave never held them, so
        // rotating them protects nothing — and a version bump on `comp`
        // would tell anyone watching that dave had access to it.
        assert!(!all.contains(&KeyScope::Idea("comp".into())));
        assert!(!all.contains(&KeyScope::Idea("assay".into())));

        // carol, who holds only `comp`, gets the metadata key and nothing
        // else, because her only idea was not rotated.
        let carol_pk = hex::encode(p.carol.public.sig_pk.as_slice());
        let (_, carol_grants) = out.iter().find(|(m, _)| m.sig_pk == carol_pk).unwrap();
        assert_eq!(carol_grants.len(), 1);
        assert_eq!(carol_grants[0].scope, KeyScope::ProjectMeta);
    }

    #[test]
    fn a_replica_receives_no_grant_when_keys_rotate() {
        let p = Project::new();
        let dave = member(&p.dave, Role::Contributor, &["lid"]);
        let remaining = vec![
            member(&p.alice, Role::Lead, &["lid"]),
            member(&p.replica, Role::Replica, &[]),
        ];
        let out = rotate_after_removal(&dave, &remaining, &versions()).unwrap();
        let replica_pk = hex::encode(p.replica.public.sig_pk.as_slice());
        let (_, grants) = out.iter().find(|(m, _)| m.sig_pk == replica_pk).unwrap();
        assert!(
            grants.is_empty(),
            "a replica pins ciphertext and holds no content key, not even project metadata"
        );
    }

    #[test]
    fn removal_is_forward_only() {
        // The honest-limits test. The removed member's old key still opens
        // everything sealed under it before the rotation; the rotation only
        // ever protects what comes next.
        let p = Project::new();
        let dave = member(&p.dave, Role::Contributor, &["lid"]);
        let old_meta = key(1);
        let before = seal_key(&p.dave.public.seal_pk, KeyScope::ProjectMeta, 3, &old_meta).unwrap();
        assert_eq!(*unseal_key(&p.dave.seal_sk, &before).unwrap(), *old_meta);

        let out = rotate_after_removal(
            &dave,
            &[member(&p.alice, Role::Lead, &["lid"])],
            &versions(),
        )
        .unwrap();

        // Still readable afterwards — nothing can take that back.
        assert_eq!(*unseal_key(&p.dave.seal_sk, &before).unwrap(), *old_meta);
        // And nothing new is.
        for (_, grants) in &out {
            for g in grants {
                assert!(unseal_key(&p.dave.seal_sk, g).is_err());
            }
        }
    }

    #[test]
    fn rotation_refuses_a_missing_key_version_and_the_last_lead() {
        let p = Project::new();
        let dave = member(&p.dave, Role::Contributor, &["ghost"]);
        let alice = member(&p.alice, Role::Lead, &[]);
        // An idea with no current version: a membership record that claims
        // access to a key the project does not have is an inconsistency, not
        // something to paper over with a new key.
        let err =
            rotate_after_removal(&dave, std::slice::from_ref(&alice), &versions()).unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");

        // Removing the only lead leaves nobody able to administer.
        let err = rotate_after_removal(
            &alice,
            &[member(&p.carol, Role::Reviewer, &[])],
            &versions(),
        )
        .unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");

        // Removing the only lead is fine when another lead remains.
        rotate_after_removal(
            &alice,
            &[member(&p.bob, Role::Lead, &[])],
            &versions(),
        )
        .unwrap();

        // And a caller that forgot to drop the removed member is refused.
        let err = rotate_after_removal(
            &member(&p.dave, Role::Contributor, &[]),
            &[member(&p.dave, Role::Contributor, &[]), alice],
            &versions(),
        )
        .unwrap_err();
        assert!(matches!(err, KeyError::Refused(_)), "got {err:?}");
    }

    #[test]
    fn a_rotated_project_metadata_key_is_not_derivable_from_the_old_one() {
        let p = Project::new();
        let dave = member(&p.dave, Role::Contributor, &[]);
        let out = rotate_after_removal(
            &dave,
            &[member(&p.alice, Role::Lead, &[])],
            &versions(),
        )
        .unwrap();
        let (_, grants) = &out[0];
        let new_meta = unseal_key(&p.alice.seal_sk, &grants[0]).unwrap();
        // Independent random bytes, not a function of the old key — the
        // amended hierarchy (docs/11 §5).
        assert_ne!(*new_meta, *key(1));
        assert_ne!(*new_meta, [0u8; 32]);
    }

    // ---------------------------------------------------------- membership

    #[test]
    fn a_reviewer_may_not_write_and_may_not_approve() {
        use olb_vcs::dag::MemberPolicy;
        let p = Project::new();
        let m = Membership {
            members: vec![
                member(&p.alice, Role::Lead, &[]),
                member(&p.bob, Role::Contributor, &["lid"]),
                member(&p.carol, Role::Reviewer, &["lid"]),
                member(&p.replica, Role::Replica, &[]),
            ],
            approvals_required: [("main".to_string(), 2usize)].into_iter().collect(),
        };
        let pk = |id: &SecretIdentity| hex::encode(id.public.sig_pk.as_slice());

        assert!(m.may_write(&pk(&p.alice), None));
        assert!(m.may_write(&pk(&p.bob), None));
        assert!(!m.may_write(&pk(&p.carol), None), "a reviewer is read-only");
        assert!(!m.may_write(&pk(&p.replica), None), "a replica serves ciphertext");
        assert!(!m.may_write(&pk(&p.dave), None), "a stranger");

        assert!(m.may_approve(&pk(&p.alice), None));
        assert!(m.may_approve(&pk(&p.bob), None));
        assert!(
            !m.may_approve(&pk(&p.carol), None),
            "docs/05 §3: reviewers cannot approve merges"
        );
        assert!(!m.may_approve(&pk(&p.replica), None));

        assert_eq!(m.approvals_required("main"), 2);
        assert_eq!(m.approvals_required("idea/lid"), 0);
    }

    #[test]
    fn a_removed_member_may_no_longer_write() {
        use olb_vcs::dag::MemberPolicy;
        let p = Project::new();
        let mut dave = member(&p.dave, Role::Contributor, &["lid"]);
        let pk = dave.sig_pk.clone();
        let m = Membership {
            members: vec![dave.clone()],
            approvals_required: Default::default(),
        };
        assert!(m.may_write(&pk, None));

        dave.removed_at = Some(1_761_600_000);
        let m = Membership {
            members: vec![dave],
            approvals_required: Default::default(),
        };
        assert!(!m.may_write(&pk, None));
        assert!(!m.may_approve(&pk, None));
        // Upper-case hex from another code path still matches.
        assert!(!m.may_write(&pk.to_uppercase(), None));
    }

    #[test]
    fn membership_matches_a_key_case_insensitively() {
        use olb_vcs::dag::MemberPolicy;
        let p = Project::new();
        let m = Membership {
            members: vec![member(&p.bob, Role::Contributor, &[])],
            approvals_required: Default::default(),
        };
        let pk = hex::encode(p.bob.public.sig_pk.as_slice());
        assert!(m.may_write(&pk.to_uppercase(), None));
        assert!(m.active(&pk).is_some());
    }
}
