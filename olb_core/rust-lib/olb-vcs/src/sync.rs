//! Sync protocol: what travels between nodes, and when (docs/09, docs/05 §5).
//!
//! CONTRACT FILE — signatures are fixed.
//!
//! Only the PROTOCOL lives here — message shapes, head comparison, what to
//! request, when to snapshot. The Delivery and Storage calls live in
//! `olb_core`, which cannot be unit-tested. Keeping the decisions here and
//! the I/O there is what makes any of this testable.
//!
//! Two platform facts shape the whole design:
//!
//! - **Delivery Store is temporary** (CLAUDE.md constraint 4), so a node that
//!   has been offline cannot catch up from message history. State is
//!   recovered by snapshotting to Storage and fetching the snapshot.
//! - **One `send` has a hard payload ceiling.** logos.test caps a message at
//!   150 KiB and applies it to the ENCODED message, not the payload, so the
//!   usable budget is lower. An oversize publish is dropped inside GossipSub
//!   and surfaces as NoPeersToPublish — indistinguishable from having no
//!   peers — so it must be refused here rather than discovered as a phantom
//!   connectivity problem.

use crate::canon;
use crate::types::{CommitEnvelope, VcsError};
use crate::Result;
use std::collections::BTreeSet;

/// Usable payload budget for one Delivery `send`.
pub const SEND_BUDGET: usize = 144 * 1024;

/// How many commit ids one `want` frame asks for (docs/09 §5, `"max": 64`).
/// Packing a want is the caller's job; the number lives here so the wire
/// shape has one definition.
pub const WANT_BATCH: usize = 64;

/// Default `max_walk` for [`react`], from docs/09 §6 step 5: a peer more than
/// 200 commits ahead of us is a snapshot, not a walk.
pub const MAX_WALK: usize = 200;

/// A snapshot is due every this many commits (docs/05 §5, docs/09 §6).
pub const SNAPSHOT_EVERY_COMMITS: usize = 50;

/// …or this long after the last one, whichever comes first. 24 h.
pub const SNAPSHOT_MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// Bucket count. 16, per docs/01 §5 and docs/09 §4.
pub const BUCKETS: u8 = 16;

/// Which of the 16 buckets a value falls in: the low nibble of its SHA-256.
fn bucket(bytes: &[u8]) -> char {
    let h = crate::hashing::digest(bytes);
    // Low nibble of the digest, as a lowercase hex character.
    std::char::from_digit((h[31] & 0x0f) as u32, 16).expect("nibble is 0-15")
}

/// The sync topic for a project: `/olb/1/sync-<b>/proto` (docs/09 §4).
///
/// Projects share 16 buckets rather than getting a topic each. A topic per
/// project would let anyone watching the shard count the projects on it and
/// attribute every message to one of them; bucketing hides which labs
/// collaborate, and reduces how much other traffic a node decrypts and
/// throws away. It does NOT spread network load — auto-sharding ignores the
/// `name` segment (docs/09 §1), so all 16 land on one shard by design.
///
/// `encoding` is `proto`, not `json`: frames on the wire are ciphertext.
pub fn project_topic(project_id: &str) -> String {
    format!("/olb/1/sync-{}/proto", bucket(project_id.as_bytes()))
}

/// A member's invite topic: `/olb/1/invite-<b>/proto`, bucketed by the low
/// nibble of SHA-256 over their ML-KEM-768 public key (docs/04 §3, §09 §4).
pub fn inbox_topic(seal_pk: &[u8]) -> String {
    format!("/olb/1/invite-{}/proto", bucket(seal_pk))
}

/// Replica announcements and pin acknowledgements. Unbucketed; a replica
/// subscribes to all 16 sync buckets plus this one (docs/09 §4).
pub fn replica_topic() -> String {
    "/olb/1/replica/proto".to_string()
}

/// Every sync bucket, for a replica node.
pub fn all_sync_topics() -> Vec<String> {
    (0..BUCKETS)
        .map(|b| {
            format!(
                "/olb/1/sync-{}/proto",
                std::char::from_digit(b as u32, 16).expect("nibble is 0-15")
            )
        })
        .collect()
}

/// Everything a node says or asks on a project topic.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum SyncMessage {
    /// "These are my branch heads." Sent on connect and after every local
    /// commit.
    Heads {
        project: String,
        heads: Vec<(String, String)>,
        /// Newest anchored (root, count) this node has seen, so peers can
        /// check an anchor extends rather than replaces (docs/10 §5.5).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<(String, u64)>,
    },
    /// "Send me these commits."
    Want { project: String, ids: Vec<String> },
    /// A batch of commit envelopes, sized to stay under SEND_BUDGET.
    Have {
        project: String,
        commits: Vec<CommitEnvelope>,
    },
    /// "The full state up to this commit is in Storage at this CID." How a
    /// node that was offline too long catches up at all.
    Snapshot {
        project: String,
        at_commit: String,
        cid: String,
        /// SHA-256 of the plaintext, so a fetched snapshot is verifiable
        /// before it is trusted.
        sha256: String,
    },
}

/// What this node should do after hearing a peer's heads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reaction {
    /// Nothing to do — same heads.
    InSync,
    /// Ask for these commits.
    Want(Vec<String>),
    /// Offer these; the peer is behind.
    Offer(Vec<String>),
    /// Too far apart to close by walking parents. Point them at a snapshot.
    NeedSnapshot,
    /// The peer's anchor does not extend ours. Do NOT sync; surface it.
    /// A peer presenting a root that replaces rather than extends history is
    /// either broken or hostile, and quietly merging from it would launder a
    /// rewritten history into this node's store.
    AnchorDivergence { ours: String, theirs: String },
}

/// The newest anchored `(root, count)` a node has seen — the `anchor` field of
/// a [`SyncMessage::Heads`] frame, and the thing [`check_anchor`] compares.
pub type AnchorView = (String, u64);

/// Compare heads and decide what to do.
///
/// Heads are `(branch, commit id)` pairs. The branch NAME is deliberately not
/// consulted: what has to be exchanged is commits, and a branch can hold two
/// heads at once while a project is diverged (docs/09 §8 — the one place a ref
/// is a list), so the same name may legitimately appear twice in one frame.
/// Refs are reconciled by `dag::set_branch` after the commits land, not here.
///
/// The decision, in order:
///
/// 1. An empty local store is always a snapshot. docs/09 §6 step 6: a brand
///    new member enters at step 5 and pays one Storage download plus a delta,
///    never a message replay — which is the only thing that works when the
///    Delivery Store has already expired (CLAUDE.md constraint 4).
/// 2. Any advertised head we do not hold is wanted. More of them than
///    `max_walk` means the gap is wider than one head exchange can close by
///    walking parents, so it is a snapshot instead.
/// 3. Otherwise, anything we hold that the peer did not advertise is offered:
///    the peer is behind. Note this is only reached when we want nothing, so
///    it cannot offer a commit to a peer who is ahead of us.
/// 4. Nothing either way is `InSync`.
///
/// Wanting beats offering when both are true (each side holds something the
/// other lacks). Pulling first is what lets this node merge; the peer reaches
/// the mirror-image conclusion from our own heads frame and pulls too, so
/// nothing is lost by not offering in the same breath.
///
/// `known` is the store membership test — `CommitStore::has_commit`. It is a
/// callback rather than a `&dyn CommitStore` so head comparison stays a pure
/// decision with no store and no DAG walk behind it.
pub fn react(
    ours: &[(String, String)],
    theirs: &[(String, String)],
    known: &dyn Fn(&str) -> bool,
    max_walk: usize,
) -> Result<Reaction> {
    for (_, id) in ours.iter().chain(theirs.iter()) {
        check_commit_id(id)?;
    }

    if ours.is_empty() {
        return Ok(if theirs.is_empty() {
            Reaction::InSync
        } else {
            // Nothing local to walk back from.
            Reaction::NeedSnapshot
        });
    }

    let their_ids: BTreeSet<&str> = theirs.iter().map(|(_, id)| id.as_str()).collect();

    let mut want = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, id) in theirs {
        if !known(id) && seen.insert(id.as_str()) {
            want.push(id.clone());
        }
    }
    if !want.is_empty() {
        if want.len() > max_walk {
            return Ok(Reaction::NeedSnapshot);
        }
        return Ok(Reaction::Want(want));
    }

    let mut offer = Vec::new();
    let mut seen = BTreeSet::new();
    for (_, id) in ours {
        if !their_ids.contains(id.as_str()) && seen.insert(id.as_str()) {
            offer.push(id.clone());
        }
    }
    Ok(if offer.is_empty() {
        Reaction::InSync
    } else {
        Reaction::Offer(offer)
    })
}

/// [`react`], applied to a peer's `Heads` frame, with the anchor check in
/// front of it.
///
/// The anchor check comes FIRST and short-circuits: a peer whose anchored root
/// does not extend ours is not a peer we take commits from at all, so there is
/// no point deciding what to want from it.
///
/// Routing by `project` is the caller's job — `olb_core` holds one head set
/// per project and must not hand this a frame for a different one.
pub fn react_to_heads(
    ours: &[(String, String)],
    our_anchor: Option<&AnchorView>,
    peer: &SyncMessage,
    known: &dyn Fn(&str) -> bool,
    max_walk: usize,
) -> Result<Reaction> {
    let (heads, their_anchor) = match peer {
        SyncMessage::Heads { heads, anchor, .. } => (heads, anchor.as_ref()),
        _ => {
            return Err(VcsError::Malformed(
                "react_to_heads expects a Heads frame".into(),
            ))
        }
    };
    if let Some(divergence) = check_anchor(our_anchor, their_anchor)? {
        return Ok(divergence);
    }
    react(ours, heads, known, max_walk)
}

/// Anchor divergence detectable from the two advertisements alone, with no
/// consistency proof.
///
/// `Some(AnchorDivergence)` means refuted: the peer's anchor cannot be a view
/// of the same history as ours, and this node must not sync from it. This is
/// not a transient error to retry past — retrying is how a rewritten history
/// gets laundered into the store, which is the exact thing anchoring exists to
/// prevent (docs/10 §5.5).
///
/// `None` means "not refuted here", which is weaker than verified. Two cases
/// are left open and both need [`merkle::verify_consistency`]; see
/// [`check_anchor_with_proof`].
///
/// [`merkle::verify_consistency`]: crate::merkle::verify_consistency
pub fn check_anchor(
    ours: Option<&AnchorView>,
    theirs: Option<&AnchorView>,
) -> Result<Option<Reaction>> {
    let (ours, theirs) = match (ours, theirs) {
        (Some(a), Some(b)) => (a, b),
        // A project with nothing anchored yet, or a peer that has seen no
        // anchor, says nothing either way. Absence is not evidence.
        _ => return Ok(None),
    };
    // A root that is not 64 lowercase hex is not a root. Refusing it here
    // keeps a junk advertisement from being mistaken for a difference, and a
    // difference from being mistaken for an attack.
    check_root(&ours.0)?;
    check_root(&theirs.0)?;

    let diverged = || {
        Some(Reaction::AnchorDivergence {
            ours: ours.0.clone(),
            theirs: theirs.0.clone(),
        })
    };

    if ours.1 == theirs.1 {
        // Same leaf count. One history has one root at one size, so a
        // difference here is unambiguous: the trees hold different commits.
        return Ok(if ours.0 == theirs.0 { None } else { diverged() });
    }
    if ours.0 == theirs.0 {
        // The same root at two different sizes. The RFC 6962 root is a
        // function of the leaves AND the split at every level, so two sizes
        // sharing a root needs a SHA-256 collision. Someone is lying about
        // `count`.
        return Ok(diverged());
    }

    // What is left is the ordinary case — one side is ahead — and it is
    // exactly the case that cannot be decided from two roots and two counts.
    // A legitimately larger tree has a different root, and so does a rewritten
    // one. Deciding between them needs the proof, so it is deferred rather
    // than guessed at: guessing "diverged" would stop all sync the moment a
    // peer anchors, and guessing "fine" is the laundering hole.
    //
    // TODO(merkle): close this in `check_anchor_with_proof` by calling
    // `crate::merkle::verify_consistency`, once it is implemented.
    Ok(None)
}

/// The full RFC 6962 check: does the larger of the two anchored trees EXTEND
/// the smaller, rather than replace it (docs/10 §5.5)?
///
/// **This does not verify anything yet.** `merkle::verify_consistency` is
/// `todo!()`, and calling it would panic, so for now this function performs
/// the cheap checks of [`check_anchor`], validates that the roots and the
/// proof decode, and returns `Ok(None)` for the ahead/behind case. Treat an
/// `Ok(None)` from here as UNVERIFIED until the `TODO(merkle)` below is
/// closed.
pub fn check_anchor_with_proof(
    ours: &AnchorView,
    theirs: &AnchorView,
    proof: &[[u8; 32]],
) -> Result<Option<Reaction>> {
    if let Some(divergence) = check_anchor(Some(ours), Some(theirs))? {
        return Ok(Some(divergence));
    }
    if ours.1 == theirs.1 {
        // Identical root and count: nothing to prove.
        return Ok(None);
    }
    // The smaller tree is the one that must be extended.
    let (old, m, new, n) = if ours.1 < theirs.1 {
        (&ours.0, ours.1, &theirs.0, theirs.1)
    } else {
        (&theirs.0, theirs.1, &ours.0, ours.1)
    };
    let old_root = root_bytes(old)?;
    let new_root = root_bytes(new)?;
    let _ = (old_root, new_root, m, n, proof);

    // TODO(merkle): replace this with
    //
    //     let ok = crate::merkle::verify_consistency(
    //         crate::merkle::ANCHOR, &old_root, m as usize,
    //         &new_root, n as usize, proof);
    //
    // and return `Reaction::AnchorDivergence { ours: ours.0, theirs: theirs.0 }`
    // when `ok` is false. Not called today: `merkle::verify_consistency` is
    // `todo!()` and would panic the module.
    Ok(None)
}

/// Split commits into `Have` messages that each stay under SEND_BUDGET.
///
/// A single commit that cannot fit alone is an error, not something to
/// truncate: dropping part of a commit produces a message that fails
/// signature verification on arrival and looks like corruption.
pub fn pack(project: &str, commits: &[CommitEnvelope]) -> Result<Vec<SyncMessage>> {
    // Sizes are measured in canonical JSON, the same encoding the frame
    // carries (docs/09 §5), so the number here is the number on the wire
    // before AEAD expansion — which SEND_BUDGET's 6 KiB of headroom under the
    // 150 KiB network cap already covers.
    let overhead = canon_len(&SyncMessage::Have {
        project: project.to_string(),
        commits: Vec::new(),
    })?;

    let mut out: Vec<SyncMessage> = Vec::new();
    let mut batch: Vec<CommitEnvelope> = Vec::new();
    // Bytes the current batch's commits occupy inside the array, separators
    // included. `overhead + used` is the exact canonical length of the
    // message the batch would become.
    let mut used = 0usize;

    for commit in commits {
        let size = canon_len(commit)?;
        if overhead + size > SEND_BUDGET {
            return Err(VcsError::Refused(format!(
                "commit {} is {size} bytes and does not fit one send \
                 (budget {SEND_BUDGET}, framing {overhead}); it must travel as a \
                 manifest with its records in Storage (docs/09 §5). Refusing \
                 rather than truncating: a partial commit fails signature \
                 verification on arrival and looks like corruption.",
                commit.id
            )));
        }
        if !batch.is_empty() && overhead + used + 1 + size > SEND_BUDGET {
            out.push(SyncMessage::Have {
                project: project.to_string(),
                commits: std::mem::take(&mut batch),
            });
            used = 0;
        }
        used += if batch.is_empty() { 0 } else { 1 } + size;
        batch.push(commit.clone());
    }
    if !batch.is_empty() {
        out.push(SyncMessage::Have {
            project: project.to_string(),
            commits: batch,
        });
    }

    // Measured, not trusted. The arithmetic above is exact for canonical
    // JSON, but the cost of being wrong is a message GossipSub drops and
    // Delivery reports as NoPeersToPublish — a day spent chasing a phantom
    // connectivity problem (docs/09 §1).
    for message in &out {
        let len = canon_len(message)?;
        if len > SEND_BUDGET {
            return Err(VcsError::Refused(format!(
                "packed frame is {len} bytes, over the {SEND_BUDGET} byte send budget"
            )));
        }
    }
    Ok(out)
}

/// Whether a snapshot is due (docs/05 §5).
///
/// Every 50 commits or 24 h, whichever comes first — with one qualification:
/// zero commits since the last snapshot means the snapshot would be
/// byte-identical, hence the same CID, so the age trigger does not fire on an
/// idle project. The immediate triggers docs/09 §6 lists — project creation,
/// a membership change, pre-registration, release, publication — are events,
/// not counters, so they are the caller's to raise and not expressible here.
pub fn should_snapshot(commits_since_last: usize, secs_since_last: u64) -> bool {
    if commits_since_last == 0 {
        return false;
    }
    commits_since_last >= SNAPSHOT_EVERY_COMMITS || secs_since_last >= SNAPSHOT_MAX_AGE_SECS
}

fn canon_len<T: serde::Serialize>(value: &T) -> Result<usize> {
    Ok(canon::to_vec_of(value)?.len())
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Commit ids are 64 LOWERCASE hex (`hashing::hex32`). The spelling is
/// checked, not normalised: accepting `ABC…` would give one commit two ids,
/// and two ids mean a commit that is wanted forever because the id that
/// arrives never matches the id that was asked for.
fn check_commit_id(id: &str) -> Result<()> {
    if is_hex64(id) {
        Ok(())
    } else {
        Err(VcsError::Malformed(format!(
            "advertised head {id:?} is not a 64 character lowercase hex commit id"
        )))
    }
}

fn check_root(root: &str) -> Result<()> {
    if is_hex64(root) {
        Ok(())
    } else {
        Err(VcsError::Malformed(format!(
            "anchored root {root:?} is not a 64 character lowercase hex digest"
        )))
    }
}

fn root_bytes(root: &str) -> Result<[u8; 32]> {
    check_root(root)?;
    let mut out = [0u8; 32];
    hex::decode_to_slice(root, &mut out).map_err(|e| VcsError::Malformed(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CommitHeader, CommitMeta};
    use std::collections::BTreeSet;

    /// A distinct, well-formed commit id per test fixture.
    fn cid(n: u64) -> String {
        format!("{n:064x}")
    }

    fn root(n: u64) -> String {
        format!("{n:064x}")
    }

    fn head(branch: &str, id: &str) -> (String, String) {
        (branch.to_string(), id.to_string())
    }

    /// `known` over a fixed set, which is all `react` ever asks of a store.
    fn store(ids: &[String]) -> impl Fn(&str) -> bool {
        let set: BTreeSet<String> = ids.iter().cloned().collect();
        move |id: &str| set.contains(id)
    }

    fn envelope(id: &str, message: &str) -> CommitEnvelope {
        CommitEnvelope {
            id: id.to_string(),
            header: CommitHeader {
                v: 1,
                project: "olb:proj:p".into(),
                parents: vec![],
                tree: cid(0),
                author: cid(1),
                co_signers: vec![],
                message: message.to_string(),
                changes: vec![],
                files_added: vec![],
                files_removed: vec![],
                timestamp: 1_700_000_000_000,
            },
            sig: "00".repeat(64),
            co_sigs: vec![],
            meta: CommitMeta::default(),
        }
    }

    // ---- react: head comparison ----------------------------------------

    #[test]
    fn identical_heads_are_in_sync() {
        let ours = vec![head("main", &cid(1)), head("alt-buffer", &cid(2))];
        let theirs = ours.clone();
        let known = store(&[cid(1), cid(2)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::InSync
        );
    }

    #[test]
    fn two_empty_stores_are_in_sync() {
        let known = store(&[]);
        assert_eq!(react(&[], &[], &known, MAX_WALK).unwrap(), Reaction::InSync);
    }

    #[test]
    fn a_peer_ahead_on_one_branch_is_wanted_exactly() {
        // Same `main`, their `alt-buffer` has moved on to a commit we do not
        // hold. Exactly one id should be asked for.
        let ours = vec![head("main", &cid(1)), head("alt-buffer", &cid(2))];
        let theirs = vec![head("main", &cid(1)), head("alt-buffer", &cid(3))];
        let known = store(&[cid(1), cid(2)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(3)])
        );
    }

    #[test]
    fn a_branch_we_have_never_heard_of_is_wanted() {
        let ours = vec![head("main", &cid(1))];
        let theirs = vec![head("main", &cid(1)), head("h2-rerun", &cid(9))];
        let known = store(&[cid(1)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(9)])
        );
    }

    #[test]
    fn a_peer_behind_gets_an_offer() {
        // We hold their head, so we are ahead: offer ours.
        let ours = vec![head("main", &cid(5))];
        let theirs = vec![head("main", &cid(1))];
        let known = store(&[cid(1), cid(5)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Offer(vec![cid(5)])
        );
    }

    #[test]
    fn a_branch_the_peer_does_not_have_is_offered() {
        let ours = vec![head("main", &cid(1)), head("alt-buffer", &cid(2))];
        let theirs = vec![head("main", &cid(1))];
        let known = store(&[cid(1), cid(2)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Offer(vec![cid(2)])
        );
    }

    #[test]
    fn a_peer_with_no_heads_at_all_is_offered_ours() {
        let ours = vec![head("main", &cid(1))];
        let known = store(&[cid(1)]);
        assert_eq!(
            react(&ours, &[], &known, MAX_WALK).unwrap(),
            Reaction::Offer(vec![cid(1)])
        );
    }

    #[test]
    fn wanting_beats_offering_when_both_sides_have_something() {
        // Diverged: each side holds a commit the other lacks. We pull first;
        // the peer reaches the mirror conclusion from our own heads frame.
        let ours = vec![head("main", &cid(1)), head("alt-buffer", &cid(2))];
        let theirs = vec![head("main", &cid(1)), head("h2-rerun", &cid(7))];
        let known = store(&[cid(1), cid(2)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(7)])
        );
    }

    #[test]
    fn a_diverged_branch_advertising_two_heads_wants_only_the_missing_one() {
        // docs/09 §8: while a project is diverged a branch genuinely holds
        // two heads, so the same name appears twice in one frame.
        let ours = vec![head("main", &cid(1))];
        let theirs = vec![head("main", &cid(1)), head("main", &cid(2))];
        let known = store(&[cid(1)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(2)])
        );
    }

    #[test]
    fn a_repeated_head_is_asked_for_once() {
        let ours = vec![head("main", &cid(1))];
        let theirs = vec![head("main", &cid(4)), head("mirror", &cid(4))];
        let known = store(&[cid(1)]);
        assert_eq!(
            react(&ours, &theirs, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(4)])
        );
    }

    #[test]
    fn a_gap_wider_than_max_walk_needs_a_snapshot() {
        let ours = vec![head("main", &cid(1))];
        let theirs: Vec<_> = (10..15)
            .map(|n| head(&format!("b{n}"), &cid(n)))
            .collect();
        let known = store(&[cid(1)]);
        // Five unknown heads, a walk budget of two.
        assert_eq!(
            react(&ours, &theirs, &known, 2).unwrap(),
            Reaction::NeedSnapshot
        );
        // The same exchange with the budget raised closes by walking.
        assert!(matches!(
            react(&ours, &theirs, &known, 5).unwrap(),
            Reaction::Want(ids) if ids.len() == 5
        ));
    }

    #[test]
    fn a_zero_walk_budget_always_snapshots() {
        let ours = vec![head("main", &cid(1))];
        let theirs = vec![head("main", &cid(2))];
        let known = store(&[cid(1)]);
        assert_eq!(react(&ours, &theirs, &known, 0).unwrap(), Reaction::NeedSnapshot);
    }

    #[test]
    fn a_brand_new_member_always_needs_a_snapshot() {
        // docs/09 §6 step 6: a join costs one Storage download plus a delta,
        // never a message replay — the Delivery Store cannot be relied on to
        // still hold the history (CLAUDE.md constraint 4).
        let theirs = vec![head("main", &cid(1))];
        let known = store(&[]);
        assert_eq!(
            react(&[], &theirs, &known, MAX_WALK).unwrap(),
            Reaction::NeedSnapshot
        );
    }

    #[test]
    fn a_malformed_head_is_refused_not_wanted() {
        let ours = vec![head("main", &cid(1))];
        let known = store(&[cid(1)]);
        let upper = cid(0xab).to_uppercase();
        for bad in ["", "deadbeef", upper.as_str()] {
            let theirs = vec![head("main", bad)];
            assert!(
                matches!(
                    react(&ours, &theirs, &known, MAX_WALK),
                    Err(VcsError::Malformed(_))
                ),
                "{bad:?} should be refused"
            );
        }
    }

    // ---- anchors ---------------------------------------------------------

    #[test]
    fn an_identical_anchor_is_no_divergence() {
        let a = (root(0xaa), 205);
        assert_eq!(check_anchor(Some(&a), Some(&a.clone())).unwrap(), None);
    }

    #[test]
    fn a_missing_anchor_on_either_side_proves_nothing() {
        let a = (root(0xaa), 205);
        assert_eq!(check_anchor(None, Some(&a)).unwrap(), None);
        assert_eq!(check_anchor(Some(&a), None).unwrap(), None);
        assert_eq!(check_anchor(None, None).unwrap(), None);
    }

    #[test]
    fn the_same_count_with_a_different_root_is_divergence() {
        let ours = (root(0xaa), 205);
        let theirs = (root(0xbb), 205);
        assert_eq!(
            check_anchor(Some(&ours), Some(&theirs)).unwrap(),
            Some(Reaction::AnchorDivergence {
                ours: root(0xaa),
                theirs: root(0xbb),
            })
        );
    }

    #[test]
    fn the_same_root_at_a_different_count_is_divergence() {
        // One history has one root per size, so this needs a SHA-256
        // collision. The peer is lying about `count`.
        let ours = (root(0xaa), 205);
        let theirs = (root(0xaa), 300);
        assert!(matches!(
            check_anchor(Some(&ours), Some(&theirs)).unwrap(),
            Some(Reaction::AnchorDivergence { .. })
        ));
    }

    #[test]
    fn a_peer_simply_ahead_is_deferred_to_the_consistency_proof() {
        // Not refutable from two roots and two counts: a bigger legitimate
        // tree and a rewritten one look identical here. TODO(merkle).
        let ours = (root(0xaa), 205);
        let theirs = (root(0xbb), 300);
        assert_eq!(check_anchor(Some(&ours), Some(&theirs)).unwrap(), None);
        assert_eq!(
            check_anchor_with_proof(&ours, &theirs, &[[0u8; 32]]).unwrap(),
            None
        );
    }

    #[test]
    fn a_cheap_divergence_is_caught_even_when_a_proof_is_offered() {
        // A proof must not be able to talk us out of a refutation we already
        // have.
        let ours = (root(0xaa), 205);
        let theirs = (root(0xbb), 205);
        assert!(matches!(
            check_anchor_with_proof(&ours, &theirs, &[[0u8; 32]]).unwrap(),
            Some(Reaction::AnchorDivergence { .. })
        ));
    }

    #[test]
    fn a_junk_root_is_malformed_not_divergence() {
        let ours = (root(0xaa), 205);
        let theirs = ("not-a-root".to_string(), 205);
        assert!(matches!(
            check_anchor(Some(&ours), Some(&theirs)),
            Err(VcsError::Malformed(_))
        ));
    }

    #[test]
    fn anchor_divergence_short_circuits_the_head_exchange() {
        // The peer is also plainly ahead on `main`, and we must still not ask
        // it for anything.
        let ours = vec![head("main", &cid(1))];
        let known = store(&[cid(1)]);
        let frame = SyncMessage::Heads {
            project: "olb:proj:p".into(),
            heads: vec![head("main", &cid(2))],
            anchor: Some((root(0xbb), 205)),
        };
        assert!(matches!(
            react_to_heads(&ours, Some(&(root(0xaa), 205)), &frame, &known, MAX_WALK).unwrap(),
            Reaction::AnchorDivergence { .. }
        ));
    }

    #[test]
    fn a_heads_frame_without_an_anchor_still_syncs() {
        let ours = vec![head("main", &cid(1))];
        let known = store(&[cid(1)]);
        let frame = SyncMessage::Heads {
            project: "olb:proj:p".into(),
            heads: vec![head("main", &cid(2))],
            anchor: None,
        };
        assert_eq!(
            react_to_heads(&ours, Some(&(root(0xaa), 205)), &frame, &known, MAX_WALK).unwrap(),
            Reaction::Want(vec![cid(2)])
        );
    }

    #[test]
    fn react_to_heads_refuses_anything_but_a_heads_frame() {
        let known = store(&[]);
        let frame = SyncMessage::Want {
            project: "olb:proj:p".into(),
            ids: vec![cid(1)],
        };
        assert!(matches!(
            react_to_heads(&[], None, &frame, &known, MAX_WALK),
            Err(VcsError::Malformed(_))
        ));
    }

    // ---- pack ------------------------------------------------------------

    #[test]
    fn packing_nothing_sends_nothing() {
        assert!(pack("olb:proj:p", &[]).unwrap().is_empty());
    }

    #[test]
    fn a_small_batch_is_one_message() {
        let commits: Vec<_> = (1..=4).map(|n| envelope(&cid(n), "ok")).collect();
        let out = pack("olb:proj:p", &commits).unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            SyncMessage::Have { project, commits: c } => {
                assert_eq!(project, "olb:proj:p");
                assert_eq!(c.len(), 4);
            }
            other => panic!("expected Have, got {other:?}"),
        }
    }

    #[test]
    fn packing_splits_at_the_budget_and_never_exceeds_it() {
        // ~40 KiB each: three fit a 144 KiB frame, four do not.
        let filler = "x".repeat(40_000);
        let commits: Vec<_> = (1..=7).map(|n| envelope(&cid(n), &filler)).collect();
        let out = pack("olb:proj:p", &commits).unwrap();

        assert_eq!(out.len(), 3, "expected 3+3+1");
        let mut seen = Vec::new();
        for message in &out {
            let len = canon_len(message).unwrap();
            assert!(len <= SEND_BUDGET, "{len} over budget");
            // Not just under the budget — actually filled, or the splitting
            // is cautious rather than correct.
            match message {
                SyncMessage::Have { commits, .. } => {
                    seen.extend(commits.iter().map(|c| c.id.clone()))
                }
                other => panic!("expected Have, got {other:?}"),
            }
        }
        // Every commit travels exactly once, in order.
        assert_eq!(
            seen,
            (1..=7).map(cid).collect::<Vec<_>>(),
            "pack must not drop, duplicate or reorder a commit"
        );
        assert_eq!(
            out.iter()
                .map(|m| match m {
                    SyncMessage::Have { commits, .. } => commits.len(),
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            vec![3, 3, 1]
        );
    }

    #[test]
    fn a_commit_filling_the_budget_exactly_still_fits_one_message() {
        let overhead = canon_len(&SyncMessage::Have {
            project: "olb:proj:p".into(),
            commits: Vec::new(),
        })
        .unwrap();
        let bare = canon_len(&envelope(&cid(1), "")).unwrap();
        // Canonical JSON spends one byte per ASCII character of `message`.
        let pad = SEND_BUDGET - overhead - bare;
        let exact = envelope(&cid(1), &"x".repeat(pad));

        let out = pack("olb:proj:p", std::slice::from_ref(&exact)).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(canon_len(&out[0]).unwrap(), SEND_BUDGET);

        // One byte more is refused, not shipped and not shortened.
        let over = envelope(&cid(1), &"x".repeat(pad + 1));
        assert!(matches!(
            pack("olb:proj:p", std::slice::from_ref(&over)),
            Err(VcsError::Refused(_))
        ));
    }

    #[test]
    fn a_single_oversize_commit_is_refused_never_truncated() {
        let commits = vec![envelope(&cid(1), &"x".repeat(200_000))];
        match pack("olb:proj:p", &commits) {
            Err(VcsError::Refused(message)) => {
                assert!(message.contains(&cid(1)), "name the commit: {message}");
                assert!(
                    message.contains("manifest"),
                    "say what to do instead: {message}"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn one_oversize_commit_does_not_ship_a_partial_batch() {
        // The refusal must be total. Shipping the commits before the oversize
        // one would leave a peer with a gap it can never close by `want`,
        // because the id it is missing never arrives.
        let commits = vec![
            envelope(&cid(1), "ok"),
            envelope(&cid(2), &"x".repeat(200_000)),
            envelope(&cid(3), "ok"),
        ];
        assert!(pack("olb:proj:p", &commits).is_err());
    }

    // ---- snapshots -------------------------------------------------------

    #[test]
    fn a_snapshot_is_due_every_fifty_commits() {
        assert!(!should_snapshot(49, 0));
        assert!(should_snapshot(50, 0));
        assert!(should_snapshot(51, 0));
    }

    #[test]
    fn a_snapshot_is_due_a_day_after_the_last_one() {
        assert!(!should_snapshot(1, SNAPSHOT_MAX_AGE_SECS - 1));
        assert!(should_snapshot(1, SNAPSHOT_MAX_AGE_SECS));
    }

    #[test]
    fn an_idle_project_is_not_snapshotted_again() {
        // Zero commits means a byte-identical snapshot under the same key
        // epoch, hence the same CID: an upload that buys nothing.
        assert!(!should_snapshot(0, SNAPSHOT_MAX_AGE_SECS * 7));
    }

    // ---- topics and the wire shape --------------------------------------

    #[test]
    fn topics_match_the_specified_bucket_scheme() {
        // These strings are a protocol constant: changing one partitions the
        // network. Auto-sharding keys off the application and version
        // segments only (docs/09 §1), so those two in particular are
        // load-bearing and every OLB topic must share them.
        let t = project_topic("olb:proj:7d1f");
        assert!(
            t.starts_with("/olb/1/sync-") && t.ends_with("/proto"),
            "{t}"
        );
        assert_eq!(t.len(), "/olb/1/sync-0/proto".len(), "{t}");

        let i = inbox_topic(&[0xAB; 1184]);
        assert!(i.starts_with("/olb/1/invite-") && i.ends_with("/proto"), "{i}");

        assert_eq!(replica_topic(), "/olb/1/replica/proto");

        for topic in [t, i, replica_topic()] {
            assert!(topic.starts_with("/olb/1/"), "{topic}");
            assert!(topic.ends_with("/proto"), "{topic}");
        }
    }

    #[test]
    fn there_are_exactly_sixteen_sync_buckets_and_they_are_all_distinct() {
        let all = all_sync_topics();
        assert_eq!(all.len(), 16);
        let uniq: std::collections::BTreeSet<_> = all.iter().collect();
        assert_eq!(uniq.len(), 16);
        assert_eq!(all[0], "/olb/1/sync-0/proto");
        assert_eq!(all[15], "/olb/1/sync-f/proto");
    }

    #[test]
    fn every_project_lands_in_a_real_bucket_and_they_spread() {
        // Not a uniformity proof — just that bucketing is actually happening
        // rather than everything collapsing onto one topic, which would undo
        // the privacy argument entirely.
        let seen: std::collections::BTreeSet<String> = (0..200)
            .map(|i| project_topic(&format!("olb:proj:{i:04x}")))
            .collect();
        assert!(seen.len() > 8, "only {} distinct buckets used", seen.len());
        for t in &seen {
            assert!(all_sync_topics().contains(t), "{t} is not a valid bucket");
        }
    }

    #[test]
    fn a_topic_carries_no_project_identity() {
        // A topic is derived from a hash and nothing else. This is the
        // property that keeps a Private project's identity off the shard,
        // and it is stronger than the old per-project topic: an observer
        // cannot even count how many projects share a bucket.
        let topic = project_topic("olb:proj:7d1f CRISPR off-target screen");
        for leak in ["CRISPR", "off-target", "olb:proj:7d1f", "alice"] {
            assert!(!topic.contains(leak), "{topic} leaks {leak}");
        }
    }

    #[test]
    fn the_heads_frame_omits_an_absent_anchor() {
        // `anchor` is optional on the wire: a node that has anchored nothing
        // must not advertise a null, because a peer reading one has to tell
        // "no anchor" from "root null" (`check_anchor` treats absence as no
        // evidence either way).
        let frame = SyncMessage::Heads {
            project: "olb:proj:p".into(),
            heads: vec![head("main", &cid(1))],
            anchor: None,
        };
        let json = canon::to_string(&serde_json::to_value(&frame).unwrap()).unwrap();
        assert!(!json.contains("anchor"), "{json}");
        assert!(json.contains(r#""t":"heads""#), "{json}");
    }

    #[test]
    fn every_message_survives_a_round_trip() {
        let messages = vec![
            SyncMessage::Heads {
                project: "olb:proj:p".into(),
                heads: vec![head("main", &cid(1))],
                anchor: Some((root(0xaa), 205)),
            },
            SyncMessage::Want {
                project: "olb:proj:p".into(),
                ids: vec![cid(1), cid(2)],
            },
            SyncMessage::Have {
                project: "olb:proj:p".into(),
                commits: vec![envelope(&cid(1), "first")],
            },
            SyncMessage::Snapshot {
                project: "olb:proj:p".into(),
                at_commit: cid(9),
                cid: "logos:bafy".into(),
                sha256: root(0xcd),
            },
        ];
        for message in messages {
            let bytes = canon::to_vec_of(&message).unwrap();
            let back: SyncMessage = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(back, message);
            // Canonical form is stable across a reparse, which is what makes
            // the size measured in `pack` the size that goes on the wire.
            assert_eq!(canon::to_vec_of(&back).unwrap(), bytes);
        }
    }
}
