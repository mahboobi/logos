//! Record versioning (docs/10 §3).
//!
//! CONTRACT FILE — signatures here are fixed. `dag`, `merge` and `olb_core`
//! all call them; changing a signature breaks work happening in parallel.
//! Bodies are to be implemented; the shapes are not up for negotiation.

use crate::hashing;
use crate::store::CommitStore;
use crate::types::{ObjectId, Record, VcsError};
use crate::Result;

/// `SHA256(b"olb.record.v1\0" || canon(record))`, hex.
///
/// This is the only definition of a record's identity. Every `changes[].record`
/// in a commit header is one of these, which is what lets a peer verify a
/// commit without trusting whoever served it the record store.
pub fn content_hash(rec: &Record) -> Result<String> {
    let pre = hashing::preimage(hashing::RECORD, rec)?;
    Ok(hashing::hex32(&hashing::digest(&pre)))
}

/// The object id a version-1 record establishes (docs/10 §3.1).
///
/// Errors if `rec.version != 1` — an object id is the hash of the FIRST
/// version and asking for it from a later version is always a bug.
pub fn object_id_of_first_version(rec: &Record) -> Result<ObjectId> {
    if rec.version != 1 {
        return Err(VcsError::Malformed(format!(
            "object id comes from version 1, got version {}",
            rec.version
        )));
    }
    let h = hashing::unhex32(&content_hash(rec)?)?;
    Ok(hashing::object_id_from_v1_hash(&h))
}

/// Build the next version of a record.
///
/// Sets `version = prev.version + 1` and `prev = [content_hash(prev)]`, keeps
/// `id`, and applies `mutate` to the clone. `parents` (derivation) is NOT
/// touched — it means something different from `prev` (docs/10 §3.2).
pub fn next_version(prev: &Record, mutate: impl FnOnce(&mut Record)) -> Result<Record> {
    let _ = (prev, mutate);
    todo!("docs/10 §3.2")
}

/// A merged version with two or more predecessors.
///
/// `prev` carries every parent version's hash, sorted, which is why `prev` is
/// a `Vec` rather than an `Option` from day one.
pub fn merged_version(
    parents: &[&Record],
    merged_fields: serde_json::Map<String, serde_json::Value>,
) -> Result<Record> {
    let _ = (parents, merged_fields);
    todo!("docs/10 §3.2 and §4")
}

/// Restore: read version K, write a new version N+1 whose fields equal K's.
///
/// Version N stays readable. "Restoring creates a new commit. Nothing in the
/// history is ever rewritten" (docs/10 §3.5).
pub fn restore_from(head: &Record, restore_to: &Record) -> Result<Record> {
    let _ = (head, restore_to);
    todo!("docs/10 §3.5")
}

/// Retract: a new version with `flags.retracted = true`. There is no delete
/// (docs/10 §3.6), so a delete/edit conflict reduces to an ordinary field
/// conflict on `flags.retracted`.
pub fn retract(head: &Record) -> Result<Record> {
    let _ = head;
    todo!("docs/10 §3.6")
}

/// Follow `prev` from the head blob back to version 1, verifying at each step
/// that the loaded blob's hash equals the `prev` entry that pointed at it.
///
/// Returns versions in ascending order. This path uses no commit data at all,
/// which is the point: it is one of the two independent reconstructions that
/// must agree (docs/10 §3.3).
pub fn chain_from_blobs(store: &dyn CommitStore, head_hash: &str) -> Result<Vec<Record>> {
    let _ = (store, head_hash);
    todo!("docs/10 §3.3")
}

/// Walk the commit DAG back from `from_commit`, collect every `changes[]`
/// entry for `object`, ordered by `to_version`.
///
/// This path gives who changed it, when and in which commit.
pub fn chain_from_commits(
    store: &dyn CommitStore,
    from_commit: &str,
    object: &ObjectId,
) -> Result<Vec<crate::types::Change>> {
    let _ = (store, from_commit, object);
    todo!("docs/10 §3.3")
}

/// Compute both chains and fail if they disagree.
///
/// Disagreement is exactly what a tampered store or a buggy apply looks like,
/// so this is not a debug assertion — it is the check. Verify: versions dense
/// and strictly increasing from 1; each commit's `from_version` equals the
/// previous entry's `to_version`; `blob.version == change.to_version`.
pub fn history(
    store: &dyn CommitStore,
    from_commit: &str,
    object: &ObjectId,
) -> Result<Vec<(crate::types::Change, Record)>> {
    let _ = (store, from_commit, object);
    todo!("docs/10 §3.3 — must return ChainDisagreement when the two paths differ")
}

/// The state root after a commit (docs/10 §3.4).
///
/// ```text
/// leaves = for each live object, sorted by object_id:
///            SHA256(0x02 || canon([object_id, version, record_hash]))
/// tree   = merkle_root(leaves)
/// ```
///
/// Sorted by `object_id` because this tree is rebuilt per commit rather than
/// appended to. Note the `0x02` tag: a state leaf must never be presentable
/// as an anchor-tree leaf.
pub fn state_root(live: &[(ObjectId, u64, String)]) -> Result<String> {
    let _ = live;
    todo!("docs/10 §3.4 — use hashing::TREE_LEAF and merkle::root")
}

/// Every object that exists at a commit, with its head version and record
/// hash — the input `state_root` needs.
pub fn live_objects(
    store: &dyn CommitStore,
    commit: &str,
) -> Result<Vec<(ObjectId, u64, String)>> {
    let _ = (store, commit);
    todo!("docs/10 §3.4")
}
