//! The commit DAG (docs/10 §1.4, §2).
//!
//! CONTRACT FILE — signatures are fixed; other workstreams call them.

use crate::store::CommitStore;
use crate::types::{Change, CommitEnvelope, CommitHeader, RefTarget};
use crate::Result;

/// Who may write, evaluated at the parent state. Supplied by the caller
/// because membership lives in `olb-keys`, not here — the DAG must not grow a
/// dependency on the invite system to validate a commit.
pub trait MemberPolicy {
    /// May this key author a commit on this project at this point in history?
    fn may_write(&self, author_hex: &str, at_commit: Option<&str>) -> bool;
    /// May this key co-sign an approval?
    fn may_approve(&self, key_hex: &str, at_commit: Option<&str>) -> bool;
    /// How many approvals a merge into this branch needs. 0 = unprotected.
    fn approvals_required(&self, branch: &str) -> usize;
}

/// Everything permitted, for tests and for a single-member project.
pub struct OpenPolicy;
impl MemberPolicy for OpenPolicy {
    fn may_write(&self, _author_hex: &str, _at: Option<&str>) -> bool {
        true
    }
    fn may_approve(&self, _key_hex: &str, _at: Option<&str>) -> bool {
        true
    }
    fn approvals_required(&self, _branch: &str) -> usize {
        0
    }
}

/// Build, hash and sign a commit.
///
/// `parents[0]` must already be the target side of a merge; `parents[1..]`
/// are sorted here. `changes` is sorted by object here. The author signs the
/// preimage directly — ed25519 hashes internally, so there is no
/// hash-then-sign ambiguity to worry about (docs/10 §1.4).
pub fn build_commit(
    signing: &ed25519_dalek::SigningKey,
    project: &str,
    parents: Vec<String>,
    tree: String,
    message: String,
    changes: Vec<Change>,
    files_added: Vec<String>,
    files_removed: Vec<String>,
    timestamp: i64,
) -> Result<CommitEnvelope> {
    let _ = (
        signing, project, parents, tree, message, changes, files_added, files_removed, timestamp,
    );
    todo!("docs/10 §1.3 and §1.4")
}

/// Recompute the id from the header and check the author signature.
///
/// Both must be derived from ONE preimage buffer — recomputing the canonical
/// bytes twice is how an id and a signature drift apart.
pub fn verify_commit(env: &CommitEnvelope) -> Result<()> {
    let _ = env;
    todo!("docs/10 §2.5 steps 1-2")
}

/// A co-signature over the 32-byte commit id (docs/10 §1.4).
pub fn cosign(signing: &ed25519_dalek::SigningKey, commit_id: &str) -> Result<String> {
    let _ = (signing, commit_id);
    todo!("docs/10 §1.4 — sign COMMIT_COSIGN || id_bytes")
}

/// Verify one co-signature against a key.
pub fn verify_cosig(commit_id: &str, signer_hex: &str, sig_hex: &str) -> Result<()> {
    let _ = (commit_id, signer_hex, sig_hex);
    todo!("docs/10 §1.4")
}

/// What `receive` did with a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Received {
    /// Stored and applied; these previously-pending commits also landed.
    Applied { also_applied: Vec<String> },
    /// Already present, nothing to do.
    Duplicate,
    /// Held until this parent arrives. Not an error.
    Pending { missing_parent: String },
}

/// Validate and apply an incoming commit, in the order docs/10 §2.5 fixes.
///
/// All eight steps are deterministic and identical on every node, which is
/// what makes two members' stores converge. Any failure means dropped and
/// logged, never partially applied — and a pending commit must never block an
/// unrelated one.
///
/// On success, re-check `pending` for commits waiting on this id and apply
/// them too, transitively.
pub fn receive(
    store: &mut dyn CommitStore,
    policy: &dyn MemberPolicy,
    env: &CommitEnvelope,
) -> Result<Received> {
    let _ = (store, policy, env);
    todo!("docs/10 §2.5 — all eight checks, in order")
}

/// `1 + max(gen of parents)`, `gen(root) = 0` (docs/10 §2.4).
pub fn generation(store: &dyn CommitStore, id: &str) -> Result<u64> {
    let _ = (store, id);
    todo!("docs/10 §2.4")
}

/// Best common ancestors of two heads.
///
/// Standard two-colour paint with a max-heap keyed by `(gen, id)`: push both
/// heads, pop highest, propagate colours to parents, a commit coloured by
/// both and not yet superseded is a candidate; stop when the heap holds only
/// commits already coloured by both.
///
/// May return more than one (criss-cross). Callers must NOT merge the bases
/// recursively — docs/10 §2.4 chooses to report conflicts instead, because a
/// virtual base is content nobody authored and a lab record is not the place
/// to synthesise one.
pub fn merge_base(store: &dyn CommitStore, a: &str, b: &str) -> Result<Vec<String>> {
    let _ = (store, a, b);
    todo!("docs/10 §2.4")
}

/// True when `ancestor` is reachable from `descendant`.
pub fn is_ancestor(store: &dyn CommitStore, ancestor: &str, descendant: &str) -> Result<bool> {
    let _ = (store, ancestor, descendant);
    todo!()
}

/// One row of the history graph S6 draws.
///
/// `lane` 0 is the first-parent spine — S6 requires that, which is why
/// `parents[0]` is the merge target rather than the merged-in head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogRow {
    pub commit: CommitEnvelope,
    pub lane: usize,
    /// Lanes that have a line passing through this row, for the gutter.
    pub through: Vec<usize>,
    /// Set on a row where a branch forks off or merges back in.
    pub fork: bool,
    pub merge: bool,
}

/// Topologically ordered history from a head, newest first, with lanes
/// assigned for the S6 graph gutter.
pub fn log(store: &dyn CommitStore, from: &str, limit: usize) -> Result<Vec<LogRow>> {
    let _ = (store, from, limit);
    todo!("docs/10 §2.3 — lane 0 is the first-parent chain")
}

/// Create a branch at a commit. Fails if the name already exists.
pub fn branch_create(store: &mut dyn CommitStore, name: &str, at: &str) -> Result<()> {
    let _ = (store, name, at);
    todo!("docs/10 §2.1")
}

/// Delete a branch ref. Commits become unreachable, not gone — history is
/// never rewritten and another ref may still reach them. `main` is protected.
pub fn branch_delete(store: &mut dyn CommitStore, name: &str) -> Result<()> {
    let _ = (store, name);
    todo!("docs/10 §2.1")
}

/// Create a signed tag. Immutable: re-tagging an existing name is an error.
pub fn tag_create(
    store: &mut dyn CommitStore,
    signing: &ed25519_dalek::SigningKey,
    name: &str,
    commit: &str,
) -> Result<()> {
    let _ = (store, signing, name, commit);
    todo!("docs/10 §2.1")
}

/// Resolve `HEAD`, following one level of symbolic indirection.
pub fn resolve_head(store: &dyn CommitStore) -> Result<Option<String>> {
    let _ = store;
    todo!("docs/10 §2.1")
}

/// Resolve any ref name or raw commit id to a commit id.
pub fn resolve(store: &dyn CommitStore, name_or_id: &str) -> Result<Option<String>> {
    let _ = (store, name_or_id);
    todo!()
}

/// Move a branch ref, refusing anything that is not a fast-forward unless
/// `force` is set. Used by merge and by sync's head exchange.
pub fn set_branch(
    store: &mut dyn CommitStore,
    name: &str,
    to: &str,
    force: bool,
) -> Result<RefTarget> {
    let _ = (store, name, to, force);
    todo!("docs/10 §4.6")
}

/// Re-derive `CommitMeta` locally. A peer's `meta` is never trusted: it is
/// discarded on receipt and rebuilt here (docs/10 §1.5).
pub fn rebuild_meta(
    store: &dyn CommitStore,
    header: &CommitHeader,
    branch: &str,
    received_at: i64,
) -> Result<crate::types::CommitMeta> {
    let _ = (store, header, branch, received_at);
    todo!("docs/10 §1.5")
}
