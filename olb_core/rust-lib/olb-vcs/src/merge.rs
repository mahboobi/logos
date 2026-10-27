//! Three-way merge (docs/10 §4).
//!
//! CONTRACT FILE — signatures are fixed.
//!
//! The governing rule, from docs/05 §3 and restated here because it is the
//! one thing in this file that must never be "improved": **conflicting fields
//! are never auto-resolved.** Text fields in particular are opaque at merge
//! time — no three-way text merge, not even for non-overlapping lines.
//! docs/05's "text fields diffed by line/word" is a presentation rule for S6,
//! not a merge rule. Auto-stitching two scientists' prose can fabricate an
//! observation that neither of them wrote, and a lab record is the last place
//! that should be possible.

use crate::store::CommitStore;
use crate::types::{Conflict, ObjectId, Record};
use crate::Result;

/// How an array merges, which depends entirely on whether its elements have
/// a natural identity (docs/10 §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayRule {
    /// Merged as a set keyed by `cid`. Adds from both sides union cleanly and
    /// order is canonical, so order is never a conflict. Removing on one side
    /// while editing metadata on the other IS a conflict.
    SetByCid,
    /// Merged element-wise by a natural key, recursing into the element:
    /// `predictions[]` by `label`, `milestones[]` by `id`, `authors[]` and
    /// `reviewers[]` by `key`.
    KeyedBy(&'static str),
    /// No element identity, so the whole array is one opaque value:
    /// `stages_enabled[]`, `reveals[]`. Both sides changing it differently
    /// is a conflict.
    Opaque,
}

/// Which rule applies to a given dotted path. Keeping this in one function
/// means the merge policy is auditable in one place rather than inferred from
/// scattered special cases.
pub fn array_rule(path: &str) -> ArrayRule {
    match path {
        "attachments" => ArrayRule::SetByCid,
        "fields.predictions" => ArrayRule::KeyedBy("label"),
        "fields.milestones" => ArrayRule::KeyedBy("id"),
        "authors" => ArrayRule::KeyedBy("key"),
        "fields.reviewers" => ArrayRule::KeyedBy("key"),
        _ => ArrayRule::Opaque,
    }
}

/// The result of attempting a merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Nothing to do — `theirs` is already an ancestor of `ours`.
    UpToDate,
    /// `target` is an ancestor of `source`, so the ref can simply move.
    ///
    /// Only offered for an UNPROTECTED target. On a protected branch a merge
    /// commit is always written even when a fast-forward is possible, because
    /// the co-signatures need somewhere to live for peers to check the
    /// threshold (docs/10 §4.6).
    FastForward { to: String },
    /// Clean merge. Apply these record versions, then one merge commit with
    /// `parents = [target_head, source_head]`.
    Merged {
        records: Vec<Record>,
        /// Objects taken wholesale from one side, for the summary line.
        taken_ours: usize,
        taken_theirs: usize,
    },
    /// Needs a human. Every conflict is reported; a partial resolution is
    /// rejected by `apply_resolution`.
    Conflicted(Vec<Conflict>),
}

/// One resolution decision from the UI.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Resolution {
    pub object: ObjectId,
    pub path: String,
    /// `ours` | `theirs` | `custom`
    pub choose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// Three-way merge of two heads.
///
/// If `merge_base` returns more than one base (criss-cross), do NOT merge the
/// bases recursively. Report every object whose content differs across the
/// bases as a conflict (docs/10 §2.4) — auditable beats clever.
pub fn merge(
    store: &dyn CommitStore,
    ours_head: &str,
    theirs_head: &str,
    target_protected: bool,
) -> Result<MergeOutcome> {
    let _ = (store, ours_head, theirs_head, target_protected);
    todo!("docs/10 §4.1")
}

/// Field-level merge of one object's three versions.
///
/// `None` means the object does not exist on that side. Comparison between
/// `ours` and `theirs` is by record hash, so "unchanged" is exact and free.
pub fn merge_record(
    base: Option<&Record>,
    ours: Option<&Record>,
    theirs: Option<&Record>,
) -> Result<std::result::Result<Record, Vec<Conflict>>> {
    let _ = (base, ours, theirs);
    todo!("docs/10 §4.2")
}

/// Apply a complete set of resolutions and produce the merged record
/// versions.
///
/// Rejects a resolution set that does not cover every reported conflict — a
/// partial resolution silently dropping a conflict is how a scientist loses
/// an observation without being told.
pub fn apply_resolution(
    conflicts: &[Conflict],
    resolutions: &[Resolution],
    ours: &[Record],
    theirs: &[Record],
) -> Result<Vec<Record>> {
    let _ = (conflicts, resolutions, ours, theirs);
    todo!("docs/10 §4.3 — reject a partial resolution")
}

/// A merge request, as docs/10 §4.4 specifies it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeRequest {
    pub id: String,
    pub project: String,
    pub title: String,
    pub source: String,
    pub target: String,
    pub author: String,
    /// Merge base pinned at open time.
    pub base: String,
    /// The heads the summary was computed against. An approval is bound to
    /// these, and goes stale the moment either moves.
    pub source_head: String,
    pub target_head: String,
    #[serde(default)]
    pub reviewers: Vec<serde_json::Value>,
    #[serde(default)]
    pub approvals: Vec<Approval>,
    /// Snapshotted at open time so changing the project setting mid-review
    /// cannot lower an in-flight bar.
    pub approvals_needed: usize,
    pub summary: serde_json::Value,
    pub state: String,
}

/// An approval signature bound to a specific MR and a specific pair of heads.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Approval {
    pub key: String,
    pub source_head: String,
    pub target_head: String,
    pub sig: String,
}

/// Domain tag for an approval signature. Binding the heads in is what stops
/// "get approved, then push" from defeating review entirely.
pub const MR_APPROVE_DOMAIN: &[u8] = b"olb.mr.approve.v1\0";

/// Sign an approval over `mr_id || source_head || target_head`.
pub fn sign_approval(
    signing: &ed25519_dalek::SigningKey,
    mr_id: &str,
    source_head: &str,
    target_head: &str,
) -> Result<Approval> {
    let _ = (signing, mr_id, source_head, target_head);
    todo!("docs/10 §4.4")
}

/// Count approvals that are still valid AND still fresh.
///
/// An approval counts only when its signature verifies, its `source_head` and
/// `target_head` both still match the MR's current heads, its key is a
/// project member in a role permitted to approve **at the merge base**, and
/// it is not the merge author's own key. Eligibility is evaluated at the base
/// so a membership change mid-review cannot retroactively validate or
/// invalidate an approval (docs/10 §4.5).
pub fn count_fresh_approvals(
    mr: &MergeRequest,
    policy: &dyn crate::dag::MemberPolicy,
) -> Result<usize> {
    let _ = (mr, policy);
    todo!("docs/10 §4.5 — distinct keys, not the author, fresh heads, eligible at base")
}
