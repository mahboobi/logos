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

use crate::types::CommitEnvelope;
use crate::Result;

/// Usable payload budget for one Delivery `send`.
pub const SEND_BUDGET: usize = 144 * 1024;

/// Content topic for a project's commit stream. Delivery's convention is
/// `/<app>/<version>/<name>/<encoding>`.
pub fn project_topic(project_tag: &str) -> String {
    format!("/open-labbook/1/p/{project_tag}/json")
}

/// A member's private inbox topic, for invites (docs/04 §3).
pub fn inbox_topic(inbox_tag_hex: &str) -> String {
    format!("/open-labbook/1/inbox/{inbox_tag_hex}/json")
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

/// Compare heads and decide what to do.
pub fn react(
    ours: &[(String, String)],
    theirs: &[(String, String)],
    known: &dyn Fn(&str) -> bool,
    max_walk: usize,
) -> Result<Reaction> {
    let _ = (ours, theirs, known, max_walk);
    todo!("docs/09")
}

/// Split commits into `Have` messages that each stay under SEND_BUDGET.
///
/// A single commit that cannot fit alone is an error, not something to
/// truncate: dropping part of a commit produces a message that fails
/// signature verification on arrival and looks like corruption.
pub fn pack(project: &str, commits: &[CommitEnvelope]) -> Result<Vec<SyncMessage>> {
    let _ = (project, commits);
    todo!("docs/09 — refuse an oversize single commit, never truncate")
}

/// Whether a snapshot is due (docs/05 §5).
pub fn should_snapshot(commits_since_last: usize, secs_since_last: u64) -> bool {
    let _ = (commits_since_last, secs_since_last);
    todo!("docs/05 §5")
}
