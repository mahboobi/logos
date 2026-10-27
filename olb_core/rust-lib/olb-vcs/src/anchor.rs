//! The anchoring queue (docs/10 §5.2, §5.6; docs/05 §4).
//!
//! CONTRACT FILE — signatures are fixed.
//!
//! Two things here are easy to get wrong and expensive to get wrong:
//!
//! 1. **The tree is cumulative, not per-batch.** Each project's registry PDA
//!    is OVERWRITTEN on every anchor, so only the newest root is readable on
//!    chain. A per-batch root is unverifiable the moment the next batch
//!    lands. The anchored root therefore covers every commit anchored so far,
//!    and `count` is the total leaf count.
//! 2. **Paths are recomputed, never stored.** docs/05 §4 says to store each
//!    commit's Merkle path; that is wrong for a cumulative tree, because an
//!    older leaf's path GROWS as the tree grows and a stored path is stale
//!    after the next anchor. Store the leaf index and the batch; recompute
//!    the path on demand (microseconds at OLB's scale).

use crate::types::VcsError;
use crate::Result;

/// One commit's place in the cumulative tree.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Leaf {
    /// 0-based, assigned in the order commits were queued and never changed.
    pub index: u64,
    /// 64 hex commit id.
    pub commit: String,
}

/// A flush that reached the chain.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Batch {
    pub batch: u64,
    /// 64 hex root over leaves `0..count`.
    pub root: String,
    pub count: u64,
    /// LEZ transaction hash. Present only once the account has been READ
    /// BACK — a tx_hash alone is not a receipt, as the sequencer will accept
    /// a transaction, return a hash, and then skip it (docs/08 R12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lez_tx: Option<String>,
    pub at: i64,
}

/// What a proof check produced.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Verification {
    pub valid: bool,
    pub batch: u64,
    pub root: String,
    pub leaf_index: u64,
}

/// When to flush, from project settings (docs/05 §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FlushPolicy {
    EveryChange,
    /// Default: whichever comes first.
    Periodic { secs: u64, commits: usize },
    Daily,
    KeyMomentsOnly,
}

/// Events that always flush immediately regardless of policy: a
/// pre-registration, a milestone release and a publication are the moments a
/// timestamp is actually worth something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMoment {
    PreRegistration,
    MilestoneRelease,
    Publication,
}

/// The queue of commits waiting to be anchored, plus the batches already on
/// chain.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct AnchorQueue {
    pub leaves: Vec<Leaf>,
    pub batches: Vec<Batch>,
    /// Leaves below this index are covered by a batch already on chain.
    pub anchored_count: u64,
}

impl AnchorQueue {
    /// Queue a commit. Idempotent: a commit already queued keeps its index,
    /// because changing an index would invalidate every proof that cites it.
    pub fn enqueue(&mut self, commit_id: &str) -> Result<u64> {
        let _ = commit_id;
        todo!("docs/10 §5.2")
    }

    /// Whether policy says to flush now.
    pub fn should_flush(
        &self,
        policy: FlushPolicy,
        now: i64,
        key_moment: Option<KeyMoment>,
    ) -> bool {
        let _ = (policy, now, key_moment);
        todo!("docs/05 §4, docs/10 §5.6")
    }

    /// Build the next batch: the cumulative root over ALL leaves so far.
    ///
    /// Refused when there is nothing new to anchor — `anchor` with
    /// `count == 0` is refused on chain and resubmitting an unchanged root
    /// just burns a transaction.
    pub fn prepare_flush(&self) -> Result<(String, u64)> {
        todo!("docs/10 §5.6")
    }

    /// Record a batch as landed. Call this only after reading the registry
    /// account back and seeing the root (docs/08 R12).
    pub fn confirm(&mut self, root: String, count: u64, lez_tx: String, at: i64) -> Result<u64> {
        let _ = (root, count, lez_tx, at);
        todo!("docs/10 §5.6")
    }

    /// Inclusion proof for a commit against the batch that covers it.
    pub fn prove(&self, commit_id: &str) -> Result<(Verification, Vec<[u8; 32]>)> {
        let _ = commit_id;
        todo!("docs/10 §5.4 — recompute the path, never store it")
    }

    /// Check an observed on-chain root really EXTENDS the last one we saw.
    ///
    /// With a single overwritten PDA this is the only defence against an
    /// anchor that quietly rewrites history, so a failure is not a warning —
    /// it must raise the amber state in S1 and S6 (docs/10 §5.5).
    pub fn check_extends(&self, observed_root: &str, observed_count: u64) -> Result<bool> {
        let _ = (observed_root, observed_count);
        todo!("docs/10 §5.5")
    }
}

/// `project_tag = H(project_id || project_salt)`, the second PDA seed.
///
/// This is the only thing keeping project identity off-chain for a Private
/// or Invite-only project, so the salt must never be published (docs/05 §4).
pub fn project_tag(project_id: &str, project_salt: &[u8]) -> String {
    let _ = (project_id, project_salt);
    todo!("docs/05 §4")
}

#[allow(dead_code)]
fn _unused(e: VcsError) -> VcsError {
    e
}
