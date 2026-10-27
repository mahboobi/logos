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
//!
//! **Time unit: seconds.** `now`, `Batch::at` and `FlushPolicy::Periodic.secs`
//! are all seconds since the Unix epoch. `CommitHeader::timestamp` is
//! milliseconds, so a caller passing a commit timestamp straight in would
//! turn a five-minute timer into a five-millisecond one. Nothing in the type
//! system catches it, hence this paragraph.

use crate::types::VcsError;
use crate::{hashing, merkle, Result};

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

/// The project-settings default from docs/05 §4: 5 minutes or 100 commits.
pub const DEFAULT_POLICY: FlushPolicy = FlushPolicy::Periodic {
    secs: 300,
    commits: 100,
};

/// Seconds in a day, for [`FlushPolicy::Daily`].
const DAY: i64 = 86_400;

/// Domain tag for [`project_tag`]. See that function for why docs/05's bare
/// `H(project_id || project_salt)` is not what is implemented.
pub const PROJECT_TAG: &[u8] = b"olb.project.tag.v1\0";

/// Events that always flush immediately regardless of policy: a
/// pre-registration, a milestone release and a publication are the moments a
/// timestamp is actually worth something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMoment {
    PreRegistration,
    MilestoneRelease,
    Publication,
}

/// What checking an observed on-chain root against local history proved.
///
/// Three states, not two. `Result<bool>` collapsed "we have not synced far
/// enough to judge" into the same answer as "history was replaced", and the
/// first happens constantly during ordinary peer catch-up. An alarm that
/// fires on every normal sync is an alarm nobody reads, so S1 and S6 must be
/// able to tell them apart.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Extends {
    /// The observed root is exactly what our own leaves produce at that
    /// count. Verified.
    Yes,
    /// The observed anchor covers commits we do not hold yet. Nothing is
    /// wrong and nothing is proven — re-check after sync. This is the normal
    /// state while catching up, and it must NOT be shown as a rewrite.
    NotYetVerifiable { observed_count: u64, ours: u64 },
    /// The chain disagrees with history we already hold. Someone replaced an
    /// anchor rather than extending it, which is the one thing the whole
    /// anchoring scheme exists to make visible. Amber in S1 and S6, and do
    /// not sync from the peer advertising it.
    Rewritten { reason: String },
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
        // Validate before admitting it. A commit id that is not 32 bytes of
        // hex cannot be hashed into a leaf, and discovering that at flush
        // time would wedge the whole queue behind one bad row.
        hashing::unhex32(commit_id)?;

        if let Some(existing) = self.leaves.iter().find(|l| l.commit == commit_id) {
            return Ok(existing.index);
        }
        let index = self.leaves.len() as u64;
        self.leaves.push(Leaf {
            index,
            commit: commit_id.to_owned(),
        });
        Ok(index)
    }

    /// Commits queued but not yet covered by a confirmed batch.
    pub fn pending(&self) -> u64 {
        (self.leaves.len() as u64).saturating_sub(self.anchored_count)
    }

    /// Whether policy says to flush now.
    ///
    /// Always false with nothing pending: `anchor` is refused for
    /// `count == 0`, and resubmitting an unchanged root just burns a
    /// transaction (and, with one flush in flight per signer, risks the nonce
    /// skip in docs/08 R12).
    ///
    /// The periodic and daily clocks run from the last confirmed batch. With
    /// no batch yet there is no start point — `Leaf` carries no timestamp —
    /// so the timer is treated as already elapsed and the first pending
    /// commit flushes. The alternative reading, "never flush until a batch
    /// exists", would leave a new project permanently unanchored, which is a
    /// correctness bug rather than one early transaction.
    pub fn should_flush(
        &self,
        policy: FlushPolicy,
        now: i64,
        key_moment: Option<KeyMoment>,
    ) -> bool {
        if self.pending() == 0 {
            return false;
        }
        // A pre-registration, a milestone release or a publication is the
        // moment the timestamp is worth something, so it overrides policy.
        if key_moment.is_some() {
            return true;
        }
        match policy {
            FlushPolicy::EveryChange => true,
            FlushPolicy::Periodic { secs, commits } => {
                self.pending() >= commits as u64 || self.elapsed(now) >= secs as i64
            }
            FlushPolicy::Daily => self.elapsed(now) >= DAY,
            FlushPolicy::KeyMomentsOnly => false,
        }
    }

    /// Seconds since the last confirmed batch, or `i64::MAX` when there has
    /// never been one.
    fn elapsed(&self, now: i64) -> i64 {
        match self.batches.last() {
            Some(b) => now.saturating_sub(b.at),
            None => i64::MAX,
        }
    }

    /// Build the next batch: the cumulative root over ALL leaves so far.
    ///
    /// Refused when there is nothing new to anchor — `anchor` with
    /// `count == 0` is refused on chain and resubmitting an unchanged root
    /// just burns a transaction.
    pub fn prepare_flush(&self) -> Result<(String, u64)> {
        let total = self.leaves.len() as u64;
        if total == 0 {
            return Err(VcsError::Refused(
                "nothing to anchor: the queue is empty and anchor is refused for count == 0"
                    .into(),
            ));
        }
        if total == self.anchored_count {
            return Err(VcsError::Refused(format!(
                "nothing new to anchor: all {total} leaves are already covered by batch {}",
                self.batches.last().map(|b| b.batch).unwrap_or(0)
            )));
        }
        let hashes = self.leaf_hashes(self.leaves.len())?;
        Ok((hashing::hex32(&merkle::root(merkle::ANCHOR, &hashes)), total))
    }

    /// Record a batch as landed. Call this only after reading the registry
    /// account back and seeing the root (docs/08 R12).
    ///
    /// The root is recomputed and must match. A sequencer that accepted a
    /// different root, or a queue that drifted between `prepare_flush` and
    /// the read-back, is a disagreement about history and must not be filed
    /// as a confirmation — every later proof would be built against a root
    /// the chain does not hold.
    pub fn confirm(&mut self, root: String, count: u64, lez_tx: String, at: i64) -> Result<u64> {
        if count == 0 {
            return Err(VcsError::Refused("anchor is refused for count == 0".into()));
        }
        if count > self.leaves.len() as u64 {
            return Err(VcsError::Refused(format!(
                "batch claims {count} leaves but only {} are queued",
                self.leaves.len()
            )));
        }
        if count <= self.anchored_count {
            return Err(VcsError::Refused(format!(
                "batch count {count} does not advance the anchored count {}; \
                 the cumulative tree only ever grows",
                self.anchored_count
            )));
        }
        let hashes = self.leaf_hashes(count as usize)?;
        let computed = hashing::hex32(&merkle::root(merkle::ANCHOR, &hashes));
        if computed != root {
            return Err(VcsError::TreeMismatch {
                claimed: root,
                computed,
            });
        }

        let batch = self.batches.len() as u64;
        self.batches.push(Batch {
            batch,
            root,
            count,
            lez_tx: Some(lez_tx),
            at,
        });
        self.anchored_count = count;
        Ok(batch)
    }

    /// Inclusion proof for a commit against the batch that covers it.
    ///
    /// The batch used is the NEWEST one, not the one that first carried the
    /// commit. Only the newest root is readable from the PDA — it is
    /// overwritten on every anchor — so a proof against an older batch cannot
    /// be checked against the account at all. A caller that wants to know
    /// which flush originally carried a commit reads `AnchorRef::batch` on
    /// the commit's `meta`.
    pub fn prove(&self, commit_id: &str) -> Result<(Verification, Vec<[u8; 32]>)> {
        let leaf = self
            .leaves
            .iter()
            .find(|l| l.commit == commit_id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {commit_id} is not queued")))?;

        let batch = self
            .batches
            .last()
            .ok_or_else(|| VcsError::NotFound("no batch has been anchored yet".into()))?;

        if leaf.index >= batch.count {
            return Err(VcsError::NotFound(format!(
                "commit {commit_id} is leaf {} and the newest anchor covers {} leaves; \
                 it is queued but not yet anchored",
                leaf.index, batch.count
            )));
        }

        // Recomputed, never stored: the path for this leaf is different from
        // the one it had at the previous batch size.
        let hashes = self.leaf_hashes(batch.count as usize)?;
        let path = merkle::inclusion_proof(merkle::ANCHOR, &hashes, leaf.index as usize);

        // Self-check against our own copy of the root. A false here means the
        // local queue and the confirmed batch disagree, which is worth seeing
        // before the proof is handed to a peer.
        let valid = hashing::unhex32(&batch.root).is_ok_and(|r| {
            merkle::verify_inclusion(
                merkle::ANCHOR,
                &hashes[leaf.index as usize],
                leaf.index as usize,
                batch.count as usize,
                &path,
                &r,
            )
        });

        Ok((
            Verification {
                valid,
                batch: batch.batch,
                root: batch.root.clone(),
                leaf_index: leaf.index,
            },
            path,
        ))
    }

    /// Check an observed on-chain root really EXTENDS the last one we saw.
    ///
    /// With a single overwritten PDA this is the only defence against an
    /// anchor that quietly rewrites history, so a failure is not a warning —
    /// it must raise the amber state in S1 and S6 (docs/10 §5.5).
    ///
    /// Three cases, in order of how much evidence is available:
    ///
    /// 1. `observed_count` is at or below our last confirmed batch: compare
    ///    roots directly. A smaller count is a rewrite by definition.
    /// 2. We hold at least `observed_count` leaves: recompute the root.
    ///    Stronger than any proof and it needs no cooperation from whoever
    ///    anchored.
    /// 3. The anchor covers commits we have not received yet: there is
    ///    nothing to check it against, so this returns `false`. That is
    ///    deliberate and it is the one place "not verified" and "rewritten"
    ///    are deliberately conflated — `Result<bool>` has no third state, and
    ///    of the two readings the safe one is to raise amber and re-check
    ///    after sync. Callers that can obtain a consistency proof should use
    ///    [`merkle::verify_consistency`] against the last batch's root
    ///    instead of this.
    pub fn check_extends(&self, observed_root: &str, observed_count: u64) -> Result<Extends> {
        let observed = hashing::unhex32(observed_root)?;

        if observed_count == 0 {
            // `anchor` refuses count == 0, so an account holding one is
            // either unwritten or zeroed. Either way it is not our history.
            return Ok(Extends::Rewritten {
                reason: "the anchored count is zero, which anchor never writes".into(),
            });
        }

        if let Some(last) = self.batches.last() {
            if observed_count < last.count {
                return Ok(Extends::Rewritten {
                    reason: format!(
                        "the chain went backwards: it anchors {observed_count} leaves, \
                         we already saw {} confirmed",
                        last.count
                    ),
                });
            }
            if observed_count == last.count {
                return Ok(if hashing::unhex32(&last.root)? == observed {
                    Extends::Yes
                } else {
                    Extends::Rewritten {
                        reason: format!(
                            "a different root is anchored at the same count ({observed_count}); \
                             one history has exactly one root per size"
                        ),
                    }
                });
            }
        }

        if observed_count <= self.leaves.len() as u64 {
            let hashes = self.leaf_hashes(observed_count as usize)?;
            return Ok(if merkle::root(merkle::ANCHOR, &hashes) == observed {
                Extends::Yes
            } else {
                Extends::Rewritten {
                    reason: format!(
                        "the anchored root at count {observed_count} does not match the \
                         commits we hold at that point"
                    ),
                }
            });
        }

        // Ahead of us. Nothing is wrong and nothing is proven.
        Ok(Extends::NotYetVerifiable {
            observed_count,
            ours: self.leaves.len() as u64,
        })
    }

    /// Leaf hashes for leaves `0..upto`, in insertion order.
    ///
    /// `SHA256(0x00 || commit_id_bytes)` over the RAW 32 bytes, not the hex
    /// spelling (docs/10 §5.3). Hashing the hex would still produce a
    /// self-consistent tree, which is what makes it the kind of mistake that
    /// is only found when a second implementation disagrees.
    fn leaf_hashes(&self, upto: usize) -> Result<Vec<[u8; 32]>> {
        self.leaves
            .get(..upto)
            .ok_or_else(|| {
                VcsError::Refused(format!(
                    "{upto} leaves requested but only {} are queued",
                    self.leaves.len()
                ))
            })?
            .iter()
            .map(|l| {
                let raw = hashing::unhex32(&l.commit)?;
                Ok(merkle::leaf(merkle::ANCHOR, &raw))
            })
            .collect()
    }
}

/// `project_tag = H(project_id || project_salt)`, the second PDA seed.
///
/// This is the only thing keeping project identity off-chain for a Private
/// or Invite-only project, so the salt must never be published (docs/05 §4).
///
/// Two departures from docs/05's literal `H(project_id || project_salt)`,
/// both of them strengthening it and neither changing what the field means:
///
/// - A domain tag, per the rule in `hashing` that every hash in OLB names
///   what it is hashing. Without it this digest and any other
///   `H(string || bytes)` in the system share a preimage space.
/// - The id is length-framed. `project_id` is variable length and is
///   concatenated with a secret salt, so bare concatenation makes
///   `("ab", b"c")` and `("a", b"bc")` the same tag — two projects sharing one
///   PDA, which is the exact collision the tag was added to prevent
///   (docs/05 §4, 26 Oct 2026).
///
/// Nothing on chain depends on the old spelling: the anchors made so far used
/// literal test tags, not a derived one.
pub fn project_tag(project_id: &str, project_salt: &[u8]) -> String {
    let id = project_id.as_bytes();
    let mut buf = Vec::with_capacity(8 + id.len() + project_salt.len());
    buf.extend_from_slice(&(id.len() as u64).to_le_bytes());
    buf.extend_from_slice(id);
    buf.extend_from_slice(project_salt);
    hashing::hex32(&hashing::tagged(PROJECT_TAG, &buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A commit id that looks like one: 64 hex characters.
    fn cid(n: u8) -> String {
        hashing::hex32(&hashing::tagged(b"test.commit\0", &[n]))
    }

    fn queue(n: u8) -> AnchorQueue {
        let mut q = AnchorQueue::default();
        for i in 0..n {
            q.enqueue(&cid(i)).unwrap();
        }
        q
    }

    /// Independently computed cumulative root over the first `n` test commits
    /// — built from `merkle::leaf` directly rather than from the queue, so
    /// the queue's own helper is not grading itself.
    fn expect_root(n: u8) -> String {
        let hashes: Vec<[u8; 32]> = (0..n)
            .map(|i| merkle::leaf(merkle::ANCHOR, &hashing::unhex32(&cid(i)).unwrap()))
            .collect();
        hashing::hex32(&merkle::root(merkle::ANCHOR, &hashes))
    }

    // ------------------------------------------------------------- enqueue

    #[test]
    fn a_commit_already_queued_keeps_its_index() {
        // Changing an index invalidates every proof that cites it, so this
        // has to be idempotent rather than "append again".
        let mut q = queue(3);
        assert_eq!(q.enqueue(&cid(1)).unwrap(), 1);
        assert_eq!(q.enqueue(&cid(0)).unwrap(), 0);
        assert_eq!(q.leaves.len(), 3, "re-enqueue appended a duplicate");
        assert_eq!(q.enqueue(&cid(9)).unwrap(), 3);
    }

    #[test]
    fn indexes_are_insertion_order_from_zero() {
        // Leaf order is insertion order, not sorted by id (docs/10 §5.2):
        // sorting would reshuffle the tree on every append.
        let q = queue(5);
        for (i, l) in q.leaves.iter().enumerate() {
            assert_eq!(l.index, i as u64);
            assert_eq!(l.commit, cid(i as u8));
        }
        // Enqueueing in descending id order must not reorder anything.
        let mut r = AnchorQueue::default();
        for i in (0..5u8).rev() {
            r.enqueue(&cid(i)).unwrap();
        }
        assert_eq!(r.leaves[0].commit, cid(4));
    }

    #[test]
    fn a_malformed_commit_id_is_refused_at_enqueue() {
        // Rejected on the way in. A row that cannot be hashed into a leaf
        // would otherwise wedge every later flush behind it.
        let mut q = AnchorQueue::default();
        assert!(matches!(q.enqueue("not hex"), Err(VcsError::Malformed(_))));
        assert!(matches!(q.enqueue("aabb"), Err(VcsError::Malformed(_))));
        assert!(matches!(q.enqueue(""), Err(VcsError::Malformed(_))));
        assert!(q.leaves.is_empty());
    }

    // -------------------------------------------------------- prepare_flush

    #[test]
    fn an_empty_queue_cannot_be_flushed() {
        // docs/10 §6.5 test 45, second half: anchor is refused for count == 0.
        let q = AnchorQueue::default();
        assert!(matches!(q.prepare_flush(), Err(VcsError::Refused(_))));
    }

    #[test]
    fn a_queue_with_nothing_new_cannot_be_flushed() {
        // Resubmitting an unchanged root burns a transaction, and with one
        // flush in flight per signer it risks the nonce skip in docs/08 R12.
        let mut q = queue(3);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        assert!(matches!(q.prepare_flush(), Err(VcsError::Refused(_))));
    }

    #[test]
    fn the_root_is_cumulative_over_every_leaf_not_just_the_new_ones() {
        // The whole point of §5.2. The PDA is overwritten, so a per-batch
        // root is unverifiable the moment the next batch lands.
        let mut q = queue(3);
        let (r1, c1) = q.prepare_flush().unwrap();
        assert_eq!(c1, 3);
        assert_eq!(r1, expect_root(3));
        q.confirm(r1, c1, "0xtx1".into(), 1_000).unwrap();

        for i in 3..5u8 {
            q.enqueue(&cid(i)).unwrap();
        }
        let (r2, c2) = q.prepare_flush().unwrap();
        assert_eq!(c2, 5, "count must be the TOTAL, not the batch size");
        assert_eq!(r2, expect_root(5));
        assert_ne!(r2, expect_root(2), "root covered only the new leaves");
    }

    // --------------------------------------------------------------- confirm

    #[test]
    fn confirm_files_a_batch_and_advances_the_anchored_count() {
        let mut q = queue(4);
        let (root, count) = q.prepare_flush().unwrap();
        assert_eq!(q.confirm(root.clone(), count, "0xtx".into(), 42).unwrap(), 0);
        assert_eq!(q.anchored_count, 4);
        assert_eq!(q.pending(), 0);
        let b = &q.batches[0];
        assert_eq!((b.batch, b.count, b.at), (0, 4, 42));
        assert_eq!(b.root, root);
        // A tx hash is only recorded because the caller read the account
        // back; the field exists to say "this was seen on chain", not "this
        // was submitted" (docs/08 R12).
        assert_eq!(b.lez_tx.as_deref(), Some("0xtx"));

        q.enqueue(&cid(4)).unwrap();
        let (r2, c2) = q.prepare_flush().unwrap();
        assert_eq!(q.confirm(r2, c2, "0xtx2".into(), 99).unwrap(), 1);
        assert_eq!(q.anchored_count, 5);
    }

    #[test]
    fn confirm_refuses_a_root_it_cannot_recompute() {
        // The read-back said something we did not build. Filing it would
        // make every later proof reference a root the chain does not hold.
        let mut q = queue(4);
        let wrong = expect_root(3);
        let err = q.confirm(wrong.clone(), 4, "0xtx".into(), 1).unwrap_err();
        assert!(
            matches!(&err, VcsError::TreeMismatch { claimed, .. } if claimed == &wrong),
            "got {err:?}"
        );
        assert!(q.batches.is_empty());
        assert_eq!(q.anchored_count, 0);
    }

    #[test]
    fn confirm_refuses_zero_regressing_and_over_long_counts() {
        let mut q = queue(4);
        assert!(matches!(
            q.confirm(expect_root(0), 0, "0xtx".into(), 1),
            Err(VcsError::Refused(_))
        ));
        assert!(matches!(
            q.confirm(expect_root(4), 9, "0xtx".into(), 1),
            Err(VcsError::Refused(_))
        ));

        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root.clone(), count, "0xtx".into(), 1).unwrap();
        // A replay of the same batch, and a smaller one: the cumulative tree
        // only ever grows.
        assert!(matches!(
            q.confirm(root, count, "0xtx".into(), 2),
            Err(VcsError::Refused(_))
        ));
        assert!(matches!(
            q.confirm(expect_root(2), 2, "0xtx".into(), 2),
            Err(VcsError::Refused(_))
        ));
        assert_eq!(q.batches.len(), 1);
    }

    // ----------------------------------------------------------------- prove

    #[test]
    fn a_path_is_recomputed_and_grows_with_the_tree() {
        // docs/05 §4 says store the path. This is why that is wrong: leaf 0's
        // path is a different length and different content after the next
        // batch, so a stored one would be stale and unverifiable.
        let mut q = queue(3);
        let (r1, c1) = q.prepare_flush().unwrap();
        q.confirm(r1, c1, "0xtx1".into(), 1).unwrap();
        let (v1, p1) = q.prove(&cid(0)).unwrap();
        assert!(v1.valid);
        assert_eq!((v1.batch, v1.leaf_index), (0, 0));
        assert_eq!(p1.len(), 2);

        for i in 3..9u8 {
            q.enqueue(&cid(i)).unwrap();
        }
        let (r2, c2) = q.prepare_flush().unwrap();
        q.confirm(r2, c2, "0xtx2".into(), 2).unwrap();

        let (v2, p2) = q.prove(&cid(0)).unwrap();
        assert!(v2.valid);
        assert_eq!(v2.leaf_index, 0, "the leaf index must never move");
        assert_eq!(v2.batch, 1);
        assert_eq!(p2.len(), 4, "a 9-leaf tree needs a longer path");
        assert_ne!(p1, p2, "the stored path would have been stale");
    }

    #[test]
    fn every_anchored_commit_proves_against_the_newest_root() {
        let mut q = queue(11);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root.clone(), count, "0xtx".into(), 1).unwrap();
        let r = hashing::unhex32(&root).unwrap();

        for i in 0..11u8 {
            let (v, path) = q.prove(&cid(i)).unwrap();
            assert!(v.valid, "leaf {i}");
            assert_eq!(v.root, root);
            assert_eq!(v.leaf_index, i as u64);
            // Re-verify through merkle directly, as a peer holding only
            // (commit id, index, count, root) would.
            let lh = merkle::leaf(merkle::ANCHOR, &hashing::unhex32(&cid(i)).unwrap());
            assert!(merkle::verify_inclusion(
                merkle::ANCHOR,
                &lh,
                i as usize,
                count as usize,
                &path,
                &r
            ));
        }
    }

    #[test]
    fn prove_refuses_what_it_cannot_prove() {
        let mut q = queue(3);
        // Nothing anchored yet.
        assert!(matches!(q.prove(&cid(0)), Err(VcsError::NotFound(_))));

        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1).unwrap();
        // Not queued at all.
        assert!(matches!(q.prove(&cid(77)), Err(VcsError::NotFound(_))));
        // Queued after the last batch: inside the tree we are building, but
        // not inside the tree that is on chain.
        q.enqueue(&cid(3)).unwrap();
        assert!(matches!(q.prove(&cid(3)), Err(VcsError::NotFound(_))));
        assert!(q.prove(&cid(2)).unwrap().0.valid);
    }

    #[test]
    fn prove_reports_invalid_when_the_queue_and_the_batch_disagree() {
        // Tampering with a leaf after confirmation must surface here rather
        // than produce a proof that fails only once a peer checks it.
        let mut q = queue(4);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1).unwrap();
        q.leaves[2].commit = cid(200);
        let (v, _) = q.prove(&cid(0)).unwrap();
        assert!(!v.valid, "a rewritten leaf still produced a valid proof");
    }

    // --------------------------------------------------------- check_extends

    #[test]
    fn a_genuine_extension_passes() {
        let mut q = queue(4);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root.clone(), count, "0xtx".into(), 1).unwrap();

        // Same count, same root: what a re-read of an unchanged account looks
        // like.
        assert_eq!(q.check_extends(&root, 4).unwrap(), Extends::Yes);

        // A peer anchored two more commits that we have already received.
        for i in 4..6u8 {
            q.enqueue(&cid(i)).unwrap();
        }
        assert_eq!(q.check_extends(&expect_root(6), 6).unwrap(), Extends::Yes);
    }

    #[test]
    fn a_rewritten_history_fails() {
        // The attack this exists for: the PDA holds one root, so a node that
        // only compares roots cannot tell "extended" from "replaced".
        let mut q = queue(4);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1).unwrap();
        for i in 4..6u8 {
            q.enqueue(&cid(i)).unwrap();
        }

        // Same count, different root.
        let mut forged = AnchorQueue::default();
        for i in [0u8, 1, 9, 3, 4, 5] {
            forged.enqueue(&cid(i)).unwrap();
        }
        let (bad, bad_count) = forged.prepare_flush().unwrap();
        assert_eq!(bad_count, 6);
        assert!(matches!(q.check_extends(&bad, 6).unwrap(), Extends::Rewritten { .. }));

        // A count that goes backwards.
        assert!(matches!(
            q.check_extends(&expect_root(2), 2).unwrap(),
            Extends::Rewritten { .. }
        ));
        // Our own count, someone else's root.
        assert!(matches!(
            q.check_extends(&expect_root(3), 4).unwrap(),
            Extends::Rewritten { .. }
        ));
        // A zeroed or unwritten account.
        assert!(matches!(
            q.check_extends(&hashing::hex32(&[0u8; 32]), 0).unwrap(),
            Extends::Rewritten { .. }
        ));
    }

    #[test]
    fn an_anchor_ahead_of_our_sync_is_reported_as_not_verified() {
        // This is the ORDINARY state during peer catch-up, so it must be
        // distinguishable from a rewrite. Reporting it as a rewrite would
        // raise amber on every normal sync, and an alarm that fires
        // constantly is one nobody reads when it finally matters.
        let mut q = queue(4);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1).unwrap();
        assert_eq!(
            q.check_extends(&expect_root(9), 9).unwrap(),
            Extends::NotYetVerifiable { observed_count: 9, ours: 4 }
        );
    }

    #[test]
    fn check_extends_rejects_a_malformed_root() {
        let q = queue(4);
        assert!(matches!(
            q.check_extends("not a root", 4),
            Err(VcsError::Malformed(_))
        ));
    }

    #[test]
    fn check_extends_agrees_with_a_consistency_proof() {
        // Two independent routes to the same verdict: recomputation (what a
        // node with the leaves does) and an RFC 6962 consistency proof (what
        // a node without them needs).
        let mut q = queue(5);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root.clone(), count, "0xtx".into(), 1).unwrap();
        for i in 5..12u8 {
            q.enqueue(&cid(i)).unwrap();
        }
        let (new_root, new_count) = q.prepare_flush().unwrap();

        assert_eq!(
            q.check_extends(&new_root, new_count).unwrap(),
            Extends::Yes
        );

        let hashes: Vec<[u8; 32]> = (0..12u8)
            .map(|i| merkle::leaf(merkle::ANCHOR, &hashing::unhex32(&cid(i)).unwrap()))
            .collect();
        let proof = merkle::consistency_proof(merkle::ANCHOR, &hashes, 5);
        assert!(merkle::verify_consistency(
            merkle::ANCHOR,
            &hashing::unhex32(&root).unwrap(),
            5,
            &hashing::unhex32(&new_root).unwrap(),
            12,
            &proof
        ));
    }

    // ---------------------------------------------------------- should_flush

    #[test]
    fn nothing_pending_never_flushes() {
        let mut q = queue(3);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        for p in [
            FlushPolicy::EveryChange,
            DEFAULT_POLICY,
            FlushPolicy::Daily,
            FlushPolicy::KeyMomentsOnly,
        ] {
            assert!(!q.should_flush(p, 10_000_000, None), "{p:?}");
            assert!(
                !q.should_flush(p, 10_000_000, Some(KeyMoment::Publication)),
                "{p:?} flushed a key moment with nothing to anchor"
            );
        }
        assert!(!AnchorQueue::default().should_flush(FlushPolicy::EveryChange, 0, None));
    }

    #[test]
    fn a_key_moment_overrides_every_policy() {
        // Pre-registration, milestone release and publication are the moments
        // a timestamp is actually worth something (docs/05 §4).
        let mut q = queue(3);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        q.enqueue(&cid(3)).unwrap();
        for m in [
            KeyMoment::PreRegistration,
            KeyMoment::MilestoneRelease,
            KeyMoment::Publication,
        ] {
            for p in [
                FlushPolicy::KeyMomentsOnly,
                FlushPolicy::Daily,
                DEFAULT_POLICY,
            ] {
                assert!(q.should_flush(p, 1_001, Some(m)), "{m:?} under {p:?}");
            }
        }
        // And without one, those same policies hold off.
        assert!(!q.should_flush(FlushPolicy::KeyMomentsOnly, 1_001, None));
        assert!(!q.should_flush(FlushPolicy::Daily, 1_001, None));
        assert!(!q.should_flush(DEFAULT_POLICY, 1_001, None));
    }

    #[test]
    fn every_change_flushes_on_the_first_pending_commit() {
        let q = queue(1);
        assert!(q.should_flush(FlushPolicy::EveryChange, 0, None));
    }

    #[test]
    fn periodic_fires_on_whichever_of_time_or_count_comes_first() {
        let mut q = queue(3);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        let policy = FlushPolicy::Periodic {
            secs: 300,
            commits: 5,
        };

        for i in 3..7u8 {
            q.enqueue(&cid(i)).unwrap();
        }
        assert_eq!(q.pending(), 4);
        // Neither arm yet: 4 < 5 commits, 299 < 300 seconds.
        assert!(!q.should_flush(policy, 1_299, None));
        // Time arm.
        assert!(q.should_flush(policy, 1_300, None));
        // Count arm, well inside the window.
        q.enqueue(&cid(7)).unwrap();
        assert_eq!(q.pending(), 5);
        assert!(q.should_flush(policy, 1_001, None));
    }

    #[test]
    fn the_periodic_clock_is_seconds_not_milliseconds() {
        // The unit hazard this module's header warns about. If `at`/`now`
        // were read as milliseconds, five minutes would elapse in five
        // milliseconds and every commit would anchor.
        let mut q = queue(1);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        q.enqueue(&cid(1)).unwrap();
        let policy = FlushPolicy::Periodic {
            secs: 300,
            commits: 100,
        };
        assert!(!q.should_flush(policy, 1_000 + 299, None));
        assert!(q.should_flush(policy, 1_000 + 300, None));
    }

    #[test]
    fn daily_waits_a_day_from_the_last_batch() {
        let mut q = queue(1);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 1_000).unwrap();
        q.enqueue(&cid(1)).unwrap();
        assert!(!q.should_flush(FlushPolicy::Daily, 1_000 + DAY - 1, None));
        assert!(q.should_flush(FlushPolicy::Daily, 1_000 + DAY, None));
    }

    #[test]
    fn with_no_batch_yet_the_timer_counts_as_elapsed() {
        // Documented choice: `Leaf` carries no timestamp, so there is no
        // start point for the clock. "Never flush until a batch exists" would
        // leave a new project permanently unanchored.
        let q = queue(2);
        assert!(q.batches.is_empty());
        assert!(q.should_flush(DEFAULT_POLICY, 0, None));
        assert!(q.should_flush(FlushPolicy::Daily, 0, None));
        // KeyMomentsOnly still means only key moments.
        assert!(!q.should_flush(FlushPolicy::KeyMomentsOnly, 0, None));
    }

    #[test]
    fn the_default_policy_is_the_one_docs_05_specifies() {
        assert_eq!(
            DEFAULT_POLICY,
            FlushPolicy::Periodic {
                secs: 300,
                commits: 100
            }
        );
    }

    // ----------------------------------------------------------- project_tag

    #[test]
    fn project_tag_is_deterministic_64_hex() {
        let t = project_tag("olb:proj:lattice", b"salt-16-bytes-ok");
        assert_eq!(t.len(), 64);
        assert!(hashing::unhex32(&t).is_ok());
        assert_eq!(t, project_tag("olb:proj:lattice", b"salt-16-bytes-ok"));
    }

    #[test]
    fn project_tag_depends_on_both_the_id_and_the_salt() {
        // The salt is what keeps project identity off-chain, so the tag must
        // not be derivable from the id alone.
        let a = project_tag("olb:proj:a", b"salt-one");
        assert_ne!(a, project_tag("olb:proj:b", b"salt-one"));
        assert_ne!(a, project_tag("olb:proj:a", b"salt-two"));
        assert_ne!(a, project_tag("olb:proj:a", b""));
    }

    #[test]
    fn length_framing_stops_two_projects_sharing_one_pda() {
        // Bare H(id || salt) makes these identical, and a shared PDA is the
        // exact failure the project_tag seed was added to fix.
        assert_ne!(project_tag("ab", b"c"), project_tag("a", b"bc"));
        assert_ne!(project_tag("", b"xy"), project_tag("x", b"y"));
    }

    #[test]
    fn project_tag_is_domain_separated() {
        let id = "olb:proj:a";
        let salt: &[u8] = b"s";
        let untagged = hashing::hex32(&hashing::digest(
            &[id.as_bytes(), salt].concat(),
        ));
        assert_ne!(project_tag(id, salt), untagged);
    }

    // -------------------------------------------------------- serialisation

    #[test]
    fn the_queue_round_trips_through_json() {
        // It is persisted in the encrypted store, so a reload has to come
        // back with the same indexes — a reassigned index invalidates every
        // proof that cites it.
        let mut q = queue(6);
        let (root, count) = q.prepare_flush().unwrap();
        q.confirm(root, count, "0xtx".into(), 7).unwrap();

        let json = serde_json::to_string(&q).unwrap();
        let back: AnchorQueue = serde_json::from_str(&json).unwrap();
        assert_eq!(back.leaves, q.leaves);
        assert_eq!(back.batches, q.batches);
        assert_eq!(back.anchored_count, q.anchored_count);
        assert_eq!(back.prove(&cid(4)).unwrap().0, q.prove(&cid(4)).unwrap().0);
    }

    #[test]
    fn a_batch_with_no_receipt_serialises_without_the_field() {
        // `lez_tx` is absent until the account has been read back, and the
        // wire shape has to say "not seen" rather than "seen as null".
        let b = Batch {
            batch: 0,
            root: expect_root(1),
            count: 1,
            lez_tx: None,
            at: 0,
        };
        let json = serde_json::to_string(&b).unwrap();
        assert!(!json.contains("lez_tx"), "{json}");
        assert_eq!(serde_json::from_str::<Batch>(&json).unwrap(), b);
    }
}
