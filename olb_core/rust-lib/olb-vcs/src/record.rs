//! Record versioning (docs/10 §3).
//!
//! CONTRACT FILE — signatures here are fixed. `dag`, `merge` and `olb_core`
//! all call them; changing a signature breaks work happening in parallel.
//! Bodies are to be implemented; the shapes are not up for negotiation.

use crate::canon;
use crate::hashing;
use crate::merkle;
use crate::store::CommitStore;
use crate::types::{Change, ObjectId, Record, VcsError};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};

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
///
/// `mutate` runs BEFORE the version and `prev` are stamped on, deliberately:
/// there is exactly one way a version number and a `prev` pointer are allowed
/// to move, and a careless mutator must not be able to move them another way.
pub fn next_version(prev: &Record, mutate: impl FnOnce(&mut Record)) -> Result<Record> {
    let prev_hash = content_hash(prev)?;
    let mut next = prev.clone();
    mutate(&mut next);
    next.id = prev.id.clone();
    next.version = prev.version + 1;
    next.prev = vec![prev_hash];
    Ok(next)
}

/// A merged version with two or more predecessors.
///
/// `prev` carries every parent version's hash, sorted, which is why `prev` is
/// a `Vec` rather than an `Option` from day one.
///
/// Everything outside `fields` is taken from `parents[0]` — the merge caller
/// (`merge::merge_record`) merges the rest of the record itself and overwrites
/// it here, so this function is the single place the version arithmetic and
/// the `prev` set are decided.
pub fn merged_version(
    parents: &[&Record],
    merged_fields: serde_json::Map<String, serde_json::Value>,
) -> Result<Record> {
    let first = parents.first().ok_or_else(|| {
        VcsError::Malformed("a merged version needs at least one predecessor".into())
    })?;
    if parents.iter().any(|p| p.id != first.id) {
        return Err(VcsError::Malformed(
            "a merged version's predecessors must all be versions of one object".into(),
        ));
    }
    let mut prev: Vec<String> = parents
        .iter()
        .map(|p| content_hash(p))
        .collect::<Result<Vec<_>>>()?;
    prev.sort();
    prev.dedup();
    let version = parents.iter().map(|p| p.version).max().unwrap_or(0) + 1;

    let mut out = (*first).clone();
    out.fields = merged_fields;
    out.version = version;
    out.prev = prev;
    Ok(out)
}

/// Restore: read version K, write a new version N+1 whose fields equal K's.
///
/// Version N stays readable. "Restoring creates a new commit. Nothing in the
/// history is ever rewritten" (docs/10 §3.5).
///
/// Everything content-bearing comes from `restore_to`, so the new version
/// differs from K only in `version` and `prev`. The change entry carries
/// `kind: restore` and `restored_from: K` — that is the commit's business, not
/// the record's.
pub fn restore_from(head: &Record, restore_to: &Record) -> Result<Record> {
    if head.id != restore_to.id {
        return Err(VcsError::Malformed(format!(
            "cannot restore {} onto {}",
            restore_to.id, head.id
        )));
    }
    if restore_to.version > head.version {
        return Err(VcsError::Malformed(format!(
            "version {} is not in the history of version {}",
            restore_to.version, head.version
        )));
    }
    let mut out = restore_to.clone();
    out.version = head.version + 1;
    out.prev = vec![content_hash(head)?];
    Ok(out)
}

/// Retract: a new version with `flags.retracted = true`. There is no delete
/// (docs/10 §3.6), so a delete/edit conflict reduces to an ordinary field
/// conflict on `flags.retracted`.
pub fn retract(head: &Record) -> Result<Record> {
    next_version(head, |r| {
        r.flags
            .insert("retracted".to_string(), serde_json::Value::Bool(true));
    })
}

/// Follow `prev` from the head blob back to version 1, verifying at each step
/// that the loaded blob's hash equals the `prev` entry that pointed at it.
///
/// Returns versions in ascending order. This path uses no commit data at all,
/// which is the point: it is one of the two independent reconstructions that
/// must agree (docs/10 §3.3).
///
/// A merged version has more than one `prev`. The walk takes the highest
/// version among the predecessors actually present (ties broken by hash), so
/// the reconstructed chain is dense and deterministic; the other predecessor
/// is a sibling on a branch that was merged in, and `history` reaches it
/// through the commit DAG instead.
pub fn chain_from_blobs(store: &dyn CommitStore, head_hash: &str) -> Result<Vec<Record>> {
    let mut out: Vec<Record> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut cursor = head_hash.to_string();
    loop {
        if !seen.insert(cursor.clone()) {
            return Err(VcsError::ChainDisagreement(format!(
                "version chain revisits {cursor}"
            )));
        }
        let rec = store
            .record(&cursor)
            .ok_or_else(|| VcsError::MissingRecord(cursor.clone()))?;
        // The store is keyed by hash, so a mismatch here means the store is
        // lying about what it holds. That is exactly what this walk exists to
        // catch, so it is checked rather than assumed.
        let actual = content_hash(&rec)?;
        if actual != cursor {
            return Err(VcsError::ChainDisagreement(format!(
                "record stored as {cursor} hashes to {actual}"
            )));
        }
        let version = rec.version;
        let prev = rec.prev.clone();
        out.push(rec);

        if version == 1 {
            if !prev.is_empty() {
                return Err(VcsError::ChainDisagreement(
                    "version 1 must have no predecessor".into(),
                ));
            }
            break;
        }
        if prev.is_empty() {
            return Err(VcsError::ChainDisagreement(format!(
                "version {version} has no predecessor"
            )));
        }
        let mut candidates: Vec<(u64, String)> = prev
            .iter()
            .filter_map(|h| store.record(h).map(|r| (r.version, h.clone())))
            .collect();
        if candidates.is_empty() {
            return Err(VcsError::MissingRecord(prev[0].clone()));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let (pv, ph) = candidates.remove(0);
        if pv >= version {
            return Err(VcsError::ChainDisagreement(format!(
                "version {version} points back at version {pv}"
            )));
        }
        cursor = ph;
    }
    out.reverse();
    Ok(out)
}

/// Walk the commit DAG back from `from_commit`, collect every `changes[]`
/// entry for `object`, ordered by `to_version`.
///
/// This path gives who changed it, when and in which commit.
///
/// Two entries can share a `to_version`: divergence is legal, so a history
/// containing a merge genuinely reaches two version 3s. Ordering is therefore
/// `(to_version, record)` so the result is deterministic either way.
pub fn chain_from_commits(
    store: &dyn CommitStore,
    from_commit: &str,
    object: &ObjectId,
) -> Result<Vec<Change>> {
    let mut out: Vec<Change> = Vec::new();
    for id in crate::dag::reachable(store, from_commit)? {
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        for ch in env.header.changes {
            if &ch.object == object {
                out.push(ch);
            }
        }
    }
    out.sort_by(|a, b| {
        a.to_version
            .cmp(&b.to_version)
            .then(a.record.cmp(&b.record))
    });
    out.dedup();
    Ok(out)
}

/// Compute both chains and fail if they disagree.
///
/// Disagreement is exactly what a tampered store or a buggy apply looks like,
/// so this is not a debug assertion — it is the check. Verify: versions dense
/// and strictly increasing from 1; each commit's `from_version` equals the
/// previous entry's `to_version`; `blob.version == change.to_version`.
///
/// The two paths are aligned by record hash, not by position: a history that
/// contains a merge reaches change entries for versions the blob chain did not
/// walk through (the merged-in branch), and those are legal. What is not legal
/// is a blob in the chain that no commit records, a version that no blob
/// backs, a gap, or two different heads at the top version.
pub fn history(
    store: &dyn CommitStore,
    from_commit: &str,
    object: &ObjectId,
) -> Result<Vec<(Change, Record)>> {
    let changes = chain_from_commits(store, from_commit, object)?;
    if changes.is_empty() {
        return Err(VcsError::NotFound(format!(
            "{object} has no history at {from_commit}"
        )));
    }
    let top = changes.iter().map(|c| c.to_version).max().unwrap_or(0);
    let mut heads: Vec<&str> = changes
        .iter()
        .filter(|c| c.to_version == top)
        .map(|c| c.record.as_str())
        .collect();
    heads.sort();
    heads.dedup();
    if heads.len() > 1 {
        return Err(VcsError::ChainDisagreement(format!(
            "{object} has {} different version {top} heads at {from_commit}",
            heads.len()
        )));
    }

    let blobs = chain_from_blobs(store, heads[0])?;
    let mut out: Vec<(Change, Record)> = Vec::with_capacity(blobs.len());
    let mut previous_to = 0u64;
    for (i, blob) in blobs.into_iter().enumerate() {
        let expected = i as u64 + 1;
        if blob.version != expected {
            return Err(VcsError::ChainDisagreement(format!(
                "{object}: expected version {expected} in the blob chain, found {}",
                blob.version
            )));
        }
        let hash = content_hash(&blob)?;
        let change = changes
            .iter()
            .find(|c| c.record == hash)
            .ok_or_else(|| {
                VcsError::ChainDisagreement(format!(
                    "{object} version {} is in the record store but no commit records it",
                    blob.version
                ))
            })?
            .clone();
        if change.to_version != blob.version {
            return Err(VcsError::ChainDisagreement(format!(
                "{object}: commit says version {}, blob says version {}",
                change.to_version, blob.version
            )));
        }
        if change.from_version != previous_to {
            return Err(VcsError::ChainDisagreement(format!(
                "{object}: version {} follows {}, not {}",
                change.to_version, change.from_version, previous_to
            )));
        }
        previous_to = change.to_version;
        out.push((change, blob));
    }
    Ok(out)
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
    let mut sorted: Vec<&(ObjectId, u64, String)> = live.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for pair in sorted.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(VcsError::Malformed(format!(
                "{} appears twice in one state",
                pair[0].0
            )));
        }
    }
    let mut leaves: Vec<[u8; 32]> = Vec::with_capacity(sorted.len());
    for (object, version, hash) in sorted {
        let body = canon::to_vec(&serde_json::json!([object.0, version, hash]))?;
        leaves.push(merkle::leaf(merkle::STATE, &body));
    }
    Ok(hashing::hex32(&merkle::root(merkle::STATE, &leaves)))
}

/// Every object that exists at a commit, with its head version and record
/// hash — the input `state_root` needs.
///
/// Retracted objects are still live: retraction is a flag on a new version,
/// not a delete (docs/10 §3.6), and dropping a retracted object out of the
/// state tree would be a delete in all but name.
pub fn live_objects(
    store: &dyn CommitStore,
    commit: &str,
) -> Result<Vec<(ObjectId, u64, String)>> {
    // Collect every change first, then pick each object's top version. Doing
    // it in one pass would depend on the walk order, and the whole point of a
    // state root is that it does not.
    let mut seen: BTreeMap<ObjectId, BTreeMap<u64, BTreeSet<String>>> = BTreeMap::new();
    for id in crate::dag::reachable(store, commit)? {
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        for ch in env.header.changes {
            seen.entry(ch.object)
                .or_default()
                .entry(ch.to_version)
                .or_default()
                .insert(ch.record);
        }
    }
    let mut out = Vec::with_capacity(seen.len());
    for (object, versions) in seen {
        let (version, hashes) = versions
            .into_iter()
            .next_back()
            .expect("an object with no versions was never inserted");
        if hashes.len() > 1 {
            // Two different version-N records of one object reachable from a
            // single commit means the merge that was supposed to reconcile
            // them never happened, or the store was tampered with.
            return Err(VcsError::ChainDisagreement(format!(
                "{object} has {} different version {version} records reachable from {commit}",
                hashes.len()
            )));
        }
        let hash = hashes.into_iter().next().expect("non-empty by construction");
        out.push((object, version, hash));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! docs/10 §6.2. Fixtures come from `dag::tests`, which owns them because
    //! `lib.rs` is fixed and no test-only module can be added to it.

    use super::*;
    use crate::dag::tests::{build_on, commit_on, create, edit, scene, set_field};
    use crate::dag::{self, OpenPolicy, Received};
    use crate::store::{CommitStore, MemStore};
    use crate::types::ChangeKind;
    use serde_json::json;

    #[test]
    fn test9_a_created_record_is_version_one_and_names_itself() {
        let rec = create("hypothesis", json!({"statement": "Kinase X gates entry"}));
        assert_eq!(rec.version, 1);
        assert!(rec.prev.is_empty(), "version 1 has no predecessor");

        // The id IS the hash of version 1 (docs/10 §3.1), taken over the
        // draft before the id was stamped in — the only non-circular way a
        // record can carry its own content address.
        let mut draft = rec.clone();
        draft.id = ObjectId(String::new());
        assert_eq!(object_id_of_first_version(&draft).unwrap(), rec.id);
        let expected = data_encoding::BASE32_NOPAD
            .encode(&hashing::unhex32(&content_hash(&draft).unwrap()).unwrap())
            .to_lowercase();
        assert_eq!(rec.id.0, format!("olb:obj:{expected}"));

        // Different content can never produce the same object id, so two
        // members cannot create "the same object" differently.
        let other = create("hypothesis", json!({"statement": "Kinase Y gates entry"}));
        assert_ne!(rec.id, other.id);

        // Asking a later version for an object id is always a bug.
        let v2 = edit(&rec, |r| set_field(r, "statement", json!("two")));
        assert!(object_id_of_first_version(&v2).is_err());
    }

    #[test]
    fn test10_three_edits_give_a_dense_chain_that_walks_back_to_version_one() {
        let (mut store, alice, root, h) = scene();
        let mut head = h.clone();
        let mut parent = root;
        let mut hashes = vec![content_hash(&h).unwrap()];
        for n in 2..=4u64 {
            let next = edit(&head, |r| set_field(r, "statement", json!(format!("take {n}"))));
            assert_eq!(next.version, n);
            assert_eq!(next.prev, vec![content_hash(&head).unwrap()]);
            hashes.push(content_hash(&next).unwrap());
            parent = commit_on(
                &mut store,
                &alice,
                &[&parent],
                &format!("take {n}"),
                &[(next.clone(), ChangeKind::Edit)],
            );
            head = next;
        }

        let chain = chain_from_blobs(&store, &content_hash(&head).unwrap()).unwrap();
        assert_eq!(chain.len(), 4);
        for (i, rec) in chain.iter().enumerate() {
            assert_eq!(rec.version, i as u64 + 1, "versions are dense from 1");
            assert_eq!(content_hash(rec).unwrap(), hashes[i]);
            if i > 0 {
                assert_eq!(
                    rec.prev,
                    vec![hashes[i - 1].clone()],
                    "every prev hash matches the blob it points at"
                );
            }
        }
    }

    #[test]
    fn test11_the_commit_chain_and_the_blob_chain_agree() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)]);
        let v3 = edit(&v2, |r| set_field(r, "statement", json!("three")));
        let c3 = commit_on(&mut store, &alice, &[&c2], "three", &[(v3.clone(), ChangeKind::Edit)]);

        let from_commits = chain_from_commits(&store, &c3, &h.id).unwrap();
        let from_blobs = chain_from_blobs(&store, &content_hash(&v3).unwrap()).unwrap();
        assert_eq!(from_commits.len(), 3);
        assert_eq!(from_blobs.len(), 3);
        for (change, blob) in from_commits.iter().zip(from_blobs.iter()) {
            assert_eq!(change.to_version, blob.version);
            assert_eq!(change.record, content_hash(blob).unwrap());
        }

        let both = history(&store, &c3, &h.id).unwrap();
        assert_eq!(both.len(), 3);
        assert_eq!(both[0].0.kind, ChangeKind::Create);
        assert_eq!(both[0].0.from_version, 0);
        assert_eq!(both[2].0.from_version, 2);
        assert_eq!(both[2].1.version, 3);
    }

    #[test]
    fn test11_a_tampered_store_makes_the_two_chains_disagree() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)]);

        // A version 2 blob that no commit ever recorded, spliced in as v2's
        // predecessor. This is what a buggy apply looks like.
        let mut forged = v2.clone();
        set_field(&mut forged, "statement", json!("never committed"));
        let mut v3 = next_version(&forged, |_| {}).unwrap();
        v3.prev = vec![content_hash(&forged).unwrap()];
        store.put_record(&forged).unwrap();
        store.put_record(&v3).unwrap();
        // Point the head change at v3 without a commit for `forged`.
        let env = build_on(
            &mut store,
            &alice,
            &[&c2],
            "three",
            &[(v3.clone(), ChangeKind::Edit)],
            &[],
        );
        dag::receive(&mut store, &OpenPolicy, &env).unwrap();

        let err = history(&store, &env.id, &h.id).unwrap_err();
        assert!(
            matches!(err, VcsError::ChainDisagreement(_)),
            "a blob in the chain that no commit records must be caught: {err:?}"
        );
    }

    #[test]
    fn test12_restore_writes_a_new_version_and_leaves_the_old_one_readable() {
        let (mut store, alice, root, h) = scene();
        let mut head = h.clone();
        let mut parent = root;
        let mut versions = vec![h.clone()];
        for n in 2..=4u64 {
            let next = edit(&head, |r| set_field(r, "statement", json!(format!("take {n}"))));
            parent = commit_on(
                &mut store,
                &alice,
                &[&parent],
                "edit",
                &[(next.clone(), ChangeKind::Edit)],
            );
            versions.push(next.clone());
            head = next;
        }
        let v2 = versions[1].clone();
        let v4 = versions[3].clone();

        let v5 = restore_from(&v4, &v2).unwrap();
        assert_eq!(v5.version, 5);
        assert_eq!(v5.prev, vec![content_hash(&v4).unwrap()]);
        assert_eq!(v5.fields, v2.fields, "the fields are version 2's");

        let id = commit_on(
            &mut store,
            &alice,
            &[&parent],
            "Restore take 2",
            &[(v5.clone(), ChangeKind::Restore)],
        );

        // Version 4 is still readable: nothing in history was rewritten.
        assert!(store.record(&content_hash(&v4).unwrap()).is_some());
        assert_eq!(
            live_objects(&store, &id)
                .unwrap()
                .iter()
                .find(|(o, _, _)| o == &h.id)
                .map(|(_, v, _)| *v),
            Some(5)
        );
        let chain = chain_from_blobs(&store, &content_hash(&v5).unwrap()).unwrap();
        assert_eq!(chain.len(), 5);

        // Restoring onto a different object, or forward in time, is a bug.
        assert!(restore_from(&v2, &v4).is_err());
        let other = create("experiment", json!({"title": "Run 14"}));
        assert!(restore_from(&v4, &other).is_err());
    }

    #[test]
    fn test13_a_commit_whose_from_version_is_not_the_head_is_rejected() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)]);

        // A second commit claiming to take the object from 1 to 2 when the
        // head is already 2.
        let other_v2 = edit(&h, |r| set_field(r, "statement", json!("also two")));
        let env = build_on(
            &mut store,
            &alice,
            &[&root],
            "also two",
            &[(other_v2, ChangeKind::Edit)],
            &[],
        );
        // Built on `root`, where from_version 1 is correct — so re-parent it
        // onto c2, where it is not, and re-sign.
        let mut tampered = env.header.clone();
        tampered.parents = vec![c2.clone()];
        let env2 = build_commit_like(&alice, tampered);
        let err = dag::receive(&mut store, &OpenPolicy, &env2).unwrap_err();
        assert!(
            matches!(
                err,
                VcsError::VersionGap {
                    expected: 2,
                    found: 1,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(!store.has_commit(&env2.id));

        // A version that skips a number is refused too.
        let jump = {
            let mut r = edit(&v2, |_| {});
            r.version = 5;
            r
        };
        let env3 = build_on(&mut store, &alice, &[&c2], "jump", &[(jump, ChangeKind::Edit)], &[]);
        assert!(matches!(
            dag::receive(&mut store, &OpenPolicy, &env3),
            Err(VcsError::VersionGap { .. })
        ));
    }

    /// Re-sign a hand-edited header. Only a test needs this: the real writer
    /// never edits a sealed header.
    fn build_commit_like(
        signer: &ed25519_dalek::SigningKey,
        header: crate::types::CommitHeader,
    ) -> crate::types::CommitEnvelope {
        use ed25519_dalek::Signer;
        let preimage = hashing::preimage(hashing::COMMIT, &header).unwrap();
        crate::types::CommitEnvelope {
            id: hashing::hex32(&hashing::digest(&preimage)),
            header,
            sig: hex::encode(signer.sign(&preimage).to_bytes()),
            co_sigs: vec![],
            meta: Default::default(),
        }
    }

    #[test]
    fn test14_a_commit_whose_record_blob_is_missing_waits_for_it() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let env = build_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)], &[]);
        let hash = content_hash(&v2).unwrap();

        // Replay into a store that has the parent but not the blob.
        let mut fresh = MemStore::new();
        for id in dag::reachable(&store, &root).unwrap() {
            let env0 = store.commit(&id).unwrap();
            for ch in &env0.header.changes {
                fresh.put_record(&store.record(&ch.record).unwrap()).unwrap();
            }
            dag::receive(&mut fresh, &OpenPolicy, &env0).unwrap();
        }
        assert_eq!(
            dag::receive(&mut fresh, &OpenPolicy, &env).unwrap(),
            Received::Pending {
                missing_parent: hash.clone()
            },
            "a blob that has not arrived is held, exactly like a parent"
        );
        assert!(!fresh.has_commit(&env.id), "and nothing is applied");

        // The blob arrives.
        fresh.put_record(&v2).unwrap();
        assert!(matches!(
            dag::receive(&mut fresh, &OpenPolicy, &env).unwrap(),
            Received::Applied { .. }
        ));
        assert!(fresh.has_commit(&env.id));
    }

    #[test]
    fn test15_two_branches_can_each_produce_a_version_three() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)]);
        let ours = edit(&v2, |r| set_field(r, "statement", json!("ours three")));
        let theirs = edit(&v2, |r| set_field(r, "statement", json!("theirs three")));
        let a = commit_on(&mut store, &alice, &[&c2], "ours", &[(ours.clone(), ChangeKind::Edit)]);
        let b = commit_on(&mut store, &alice, &[&c2], "theirs", &[(theirs.clone(), ChangeKind::Edit)]);

        // Both blobs are stored and neither overwrote the other: `records` is
        // keyed by hash, with a NON-unique index on (object, version).
        let threes: Vec<_> = store
            .record_versions(&h.id)
            .into_iter()
            .filter(|r| r.version == 3)
            .collect();
        assert_eq!(threes.len(), 2);
        assert_ne!(
            content_hash(&threes[0]).unwrap(),
            content_hash(&threes[1]).unwrap()
        );

        // Each branch sees its own version 3 as the head.
        let head_of = |c: &str| {
            live_objects(&store, c)
                .unwrap()
                .into_iter()
                .find(|(o, _, _)| o == &h.id)
                .unwrap()
                .2
        };
        assert_eq!(head_of(&a), content_hash(&ours).unwrap());
        assert_eq!(head_of(&b), content_hash(&theirs).unwrap());
    }

    #[test]
    fn test16_the_state_root_does_not_depend_on_iteration_order() {
        let (mut store, alice, root, h) = scene();
        let run = create("experiment", json!({"title": "Run 14"}));
        let plate = create("dataset", json!({"title": "Plate 3"}));
        let id = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Add a run and a dataset",
            &[(run, ChangeKind::Create), (plate, ChangeKind::Create)],
        );

        let live = live_objects(&store, &id).unwrap();
        assert_eq!(live.len(), 3);
        let stored = store.commit(&id).unwrap().header.tree;
        assert_eq!(state_root(&live).unwrap(), stored);

        let mut shuffled = live.clone();
        shuffled.reverse();
        assert_eq!(state_root(&shuffled).unwrap(), stored);
        shuffled.rotate_left(1);
        assert_eq!(state_root(&shuffled).unwrap(), stored);

        // An empty state is still a root, and a different state is a
        // different root.
        assert_ne!(state_root(&[]).unwrap(), stored);
        let mut changed = live.clone();
        changed[0].1 += 1;
        assert_ne!(state_root(&changed).unwrap(), stored);

        // One object twice is never a state.
        let doubled = vec![live[0].clone(), live[0].clone()];
        assert!(state_root(&doubled).is_err());
        assert!(live.iter().any(|(o, _, _)| o == &h.id));
    }

    #[test]
    fn test17_retraction_is_a_new_version_not_a_delete() {
        let (mut store, alice, root, h) = scene();
        let retracted = retract(&h).unwrap();
        assert_eq!(retracted.version, 2);
        assert_eq!(retracted.flags.get("retracted"), Some(&json!(true)));
        assert_eq!(retracted.prev, vec![content_hash(&h).unwrap()]);

        let id = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Retract the hypothesis",
            &[(retracted.clone(), ChangeKind::Flag)],
        );
        // Still readable, and still live: dropping it out of the state tree
        // would be a delete in all but name.
        assert!(store.record(&content_hash(&h).unwrap()).is_some());
        assert!(live_objects(&store, &id)
            .unwrap()
            .iter()
            .any(|(o, v, _)| o == &h.id && *v == 2));
    }

    #[test]
    fn a_merged_version_has_every_predecessor_in_prev() {
        let h = create("hypothesis", json!({"statement": "one"}));
        let ours = edit(&h, |r| set_field(r, "statement", json!("ours")));
        let theirs = edit(&h, |r| set_field(r, "statement", json!("theirs")));
        let merged = merged_version(&[&ours, &theirs], ours.fields.clone()).unwrap();
        assert_eq!(merged.version, 3, "max(ours, theirs) + 1");
        let mut expected = vec![
            content_hash(&ours).unwrap(),
            content_hash(&theirs).unwrap(),
        ];
        expected.sort();
        assert_eq!(merged.prev, expected);

        // Versions of two different objects never merge.
        let other = create("experiment", json!({"title": "Run 14"}));
        assert!(merged_version(&[&ours, &other], ours.fields.clone()).is_err());
        assert!(merged_version(&[], ours.fields.clone()).is_err());
    }

    #[test]
    fn a_broken_version_chain_is_reported_not_followed() {
        let mut store = MemStore::new();
        let h = create("hypothesis", json!({"statement": "one"}));
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        store.put_record(&v2).unwrap();
        // v1 is absent.
        assert!(matches!(
            chain_from_blobs(&store, &content_hash(&v2).unwrap()),
            Err(VcsError::MissingRecord(_))
        ));

        // A version 2 that claims no predecessor.
        let mut orphaned = v2.clone();
        orphaned.prev.clear();
        store.put_record(&orphaned).unwrap();
        assert!(matches!(
            chain_from_blobs(&store, &content_hash(&orphaned).unwrap()),
            Err(VcsError::ChainDisagreement(_))
        ));

        // A version 1 that claims one.
        let mut wrong = h.clone();
        wrong.prev = vec!["ab".repeat(32)];
        store.put_record(&wrong).unwrap();
        assert!(matches!(
            chain_from_blobs(&store, &content_hash(&wrong).unwrap()),
            Err(VcsError::ChainDisagreement(_))
        ));
    }

    #[test]
    fn next_version_never_touches_derivation_parents() {
        let idea = create("idea", json!({"title": "Kinase X"}));
        let mut h = create("hypothesis", json!({"statement": "one"}));
        h.parents = vec![idea.id.clone()];
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        assert_eq!(v2.parents, vec![idea.id], "`parents` means derivation");
        assert_eq!(v2.prev.len(), 1, "`prev` means version lineage");
    }
}
