//! The commit DAG (docs/10 §1.4, §2).
//!
//! CONTRACT FILE — signatures are fixed; other workstreams call them.

use crate::hashing;
use crate::record;
use crate::store::{self, CommitStore};
use crate::types::{
    Change, CommitEnvelope, CommitHeader, CommitMeta, ObjectId, RefTarget, VcsError,
};
use crate::Result;
use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

/// The only commit format version this crate writes or accepts. `v` is inside
/// the hashed header on purpose: docs/10 §1.2 notes that leaving it outside
/// the signature makes it strippable, which is a version-rollback lever.
pub const COMMIT_V: u32 = 1;

/// How many lanes the S6 graph gutter draws before it collapses the rest into
/// a `+n branches` marker. Lane indices at or above this are reported as this
/// value, so the UI never has to invent a lane it cannot draw.
pub const MAX_LANES: usize = 3;

/// Domain tag for a signed tag ref. docs/10 §2.1 requires a tag to carry a
/// `tagger` and a `sig` but does not fix the preimage; this does, in the same
/// shape as every other signature in the system.
pub const TAG_DOMAIN: &[u8] = b"olb.tag.v1\0";

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
///
/// The argument list is the fixed contract; other workstreams call it.
#[allow(clippy::too_many_arguments)]
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
    build_commit_cosigned(
        signing,
        project,
        parents,
        tree,
        message,
        changes,
        files_added,
        files_removed,
        timestamp,
        &[],
    )
}

/// `build_commit` plus the approver keys a merge into a protected branch needs
/// (docs/10 §4.5). `build_commit` delegates here with an empty list.
///
/// ADDED, not a change to the contract: `build_commit` has no `co_signers`
/// parameter, and §4.5 requires the approver keys to be inside the hashed
/// header so a peer can check the threshold itself. Without this a merge
/// commit into a protected branch could not be built at all. `co_sigs` are
/// attached afterwards by `attach_cosig`, because a co-signer signs the id,
/// which does not exist until the header is sealed.
#[allow(clippy::too_many_arguments)]
pub fn build_commit_cosigned(
    signing: &ed25519_dalek::SigningKey,
    project: &str,
    parents: Vec<String>,
    tree: String,
    message: String,
    changes: Vec<Change>,
    files_added: Vec<String>,
    files_removed: Vec<String>,
    timestamp: i64,
    co_signers: &[String],
) -> Result<CommitEnvelope> {
    let mut parents = parents;
    if parents.len() > 1 {
        // parents[0] keeps its place — it is the merge target and the whole
        // reason the first-parent chain is the mainline. The rest are sorted
        // so two nodes building the same merge build the same id.
        let (first, rest) = parents.split_at_mut(1);
        rest.sort();
        let target = first[0].clone();
        let mut tail: Vec<String> = rest.to_vec();
        tail.dedup();
        tail.retain(|p| p != &target);
        parents = std::iter::once(target).chain(tail).collect();
    }

    let mut changes = changes;
    changes.sort_by(|a, b| a.object.cmp(&b.object));
    for pair in changes.windows(2) {
        if pair[0].object == pair[1].object {
            return Err(VcsError::Malformed(format!(
                "one commit records one transition per object; {} appears twice",
                pair[0].object
            )));
        }
    }

    let mut files_added = files_added;
    files_added.sort();
    files_added.dedup();
    let mut files_removed = files_removed;
    files_removed.sort();
    files_removed.dedup();

    let mut co_signers = co_signers.to_vec();
    co_signers.sort();
    co_signers.dedup();

    let header = CommitHeader {
        v: COMMIT_V,
        project: project.to_string(),
        parents,
        tree,
        author: hashing::hex32(&signing.verifying_key().to_bytes()),
        co_signers,
        message,
        changes,
        files_added,
        files_removed,
        timestamp,
    };

    // ONE buffer. The id hashes it and the signature covers it; serialising
    // twice is how an id and a signature silently drift apart.
    let preimage = hashing::preimage(hashing::COMMIT, &header)?;
    let id = hashing::hex32(&hashing::digest(&preimage));
    let sig = hex::encode(signing.sign(&preimage).to_bytes());

    Ok(CommitEnvelope {
        id,
        header,
        sig,
        co_sigs: Vec::new(),
        meta: CommitMeta::default(),
    })
}

/// Attach one co-signature, keeping `co_sigs` parallel to `co_signers`.
///
/// ADDED alongside `build_commit_cosigned`: a co-signature covers the commit
/// id, which only exists once the header is sealed, so it cannot be a
/// parameter of the builder.
pub fn attach_cosig(env: &mut CommitEnvelope, signing: &ed25519_dalek::SigningKey) -> Result<()> {
    let key = hashing::hex32(&signing.verifying_key().to_bytes());
    let slot = env
        .header
        .co_signers
        .iter()
        .position(|k| k == &key)
        .ok_or_else(|| VcsError::Refused(format!("{key} is not a co-signer of {}", env.id)))?;
    let sig = cosign(signing, &env.id)?;
    if env.co_sigs.len() < env.header.co_signers.len() {
        env.co_sigs
            .resize(env.header.co_signers.len(), String::new());
    }
    env.co_sigs[slot] = sig;
    Ok(())
}

/// Recompute the id from the header and check the author signature.
///
/// Both must be derived from ONE preimage buffer — recomputing the canonical
/// bytes twice is how an id and a signature drift apart.
pub fn verify_commit(env: &CommitEnvelope) -> Result<()> {
    if env.header.v != COMMIT_V {
        return Err(VcsError::Malformed(format!(
            "unsupported commit version {}",
            env.header.v
        )));
    }
    let preimage = hashing::preimage(hashing::COMMIT, &env.header)?;
    let computed = hashing::hex32(&hashing::digest(&preimage));
    if computed != env.id {
        return Err(VcsError::IdMismatch {
            claimed: env.id.clone(),
            computed,
        });
    }
    let key = verifying_key(&env.header.author)?;
    let sig = parse_sig(&env.sig)?;
    key.verify(&preimage, &sig)
        .map_err(|e| VcsError::BadSignature(format!("commit {}: {e}", env.id)))
}

/// A co-signature over the 32-byte commit id (docs/10 §1.4).
pub fn cosign(signing: &ed25519_dalek::SigningKey, commit_id: &str) -> Result<String> {
    let id = hashing::unhex32(commit_id)?;
    let mut buf = hashing::COMMIT_COSIGN.to_vec();
    buf.extend_from_slice(&id);
    Ok(hex::encode(signing.sign(&buf).to_bytes()))
}

/// Verify one co-signature against a key.
pub fn verify_cosig(commit_id: &str, signer_hex: &str, sig_hex: &str) -> Result<()> {
    let id = hashing::unhex32(commit_id)?;
    let mut buf = hashing::COMMIT_COSIGN.to_vec();
    buf.extend_from_slice(&id);
    let key = verifying_key(signer_hex)?;
    let sig = parse_sig(sig_hex)?;
    key.verify(&buf, &sig)
        .map_err(|e| VcsError::BadSignature(format!("co-signature by {signer_hex}: {e}")))
}

fn verifying_key(hex_pk: &str) -> Result<VerifyingKey> {
    let bytes = hashing::unhex32(hex_pk)?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|e| VcsError::BadSignature(format!("bad public key {hex_pk}: {e}")))
}

fn parse_sig(hex_sig: &str) -> Result<Signature> {
    let raw =
        hex::decode(hex_sig).map_err(|e| VcsError::Malformed(format!("bad signature hex: {e}")))?;
    let bytes: [u8; 64] = raw
        .try_into()
        .map_err(|_| VcsError::Malformed("a signature is 64 bytes".to_string()))?;
    Ok(Signature::from_bytes(&bytes))
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
    if let Some(existing) = store.commit(&env.id) {
        if existing.header == env.header {
            return Ok(Received::Duplicate);
        }
        return Err(VcsError::Refused(format!(
            "commit {} already stored with different content",
            env.id
        )));
    }

    // 1 + 2: canon round-trips to the claimed id, and the author's signature
    // covers that same preimage.
    verify_commit(env)?;

    // 3: the author may write at the parent state.
    let at_parent = env.header.parents.first().map(|s| s.as_str());
    if !policy.may_write(&env.header.author, at_parent) {
        return Err(VcsError::Refused(format!(
            "{} may not write to {}",
            env.header.author, env.header.project
        )));
    }

    // 4: every parent is known, else hold on the FIRST missing one.
    for parent in &env.header.parents {
        if !store.has_commit(parent) {
            return hold(store, env, parent);
        }
    }

    // 5: from_version matches the object's head on the parent state, and a
    // version moves by exactly one. For a merge the parent state is the union
    // over all parents: a merged version follows max(ours, theirs).
    let mut state = parent_state(store, &env.header.parents)?;
    for ch in &env.header.changes {
        let head = state.get(&ch.object).map(|(v, _)| *v).unwrap_or(0);
        if ch.from_version != head {
            return Err(VcsError::VersionGap {
                object: ch.object.clone(),
                expected: head,
                found: ch.from_version,
            });
        }
        if ch.to_version != ch.from_version + 1 {
            return Err(VcsError::VersionGap {
                object: ch.object.clone(),
                expected: ch.from_version + 1,
                found: ch.to_version,
            });
        }
    }

    // 6: every named record blob is present, hashes correctly and agrees
    // about its own version. A blob that has not arrived yet is not a failure
    // — the commit waits for it exactly as it waits for a parent.
    for ch in &env.header.changes {
        let blob = match store.record(&ch.record) {
            Some(b) => b,
            None => return hold(store, env, &ch.record),
        };
        let actual = record::content_hash(&blob)?;
        if actual != ch.record {
            return Err(VcsError::ChainDisagreement(format!(
                "record stored as {} hashes to {actual}",
                ch.record
            )));
        }
        if blob.version != ch.to_version {
            return Err(VcsError::VersionGap {
                object: ch.object.clone(),
                expected: ch.to_version,
                found: blob.version,
            });
        }
        if blob.id != ch.object {
            return Err(VcsError::Malformed(format!(
                "record {} belongs to {}, not {}",
                ch.record, blob.id, ch.object
            )));
        }
    }

    // 7: the recomputed state root equals the header's.
    for ch in &env.header.changes {
        state.insert(ch.object.clone(), (ch.to_version, ch.record.clone()));
    }
    let live: Vec<(ObjectId, u64, String)> = state
        .iter()
        .map(|(o, (v, h))| (o.clone(), *v, h.clone()))
        .collect();
    let computed = record::state_root(&live)?;
    if computed != env.header.tree {
        return Err(VcsError::TreeMismatch {
            claimed: env.header.tree.clone(),
            computed,
        });
    }

    // 8: a merge into a protected branch needs its approvals, and the rule is
    // the same on every node — which is what makes the threshold hold without
    // trusting any one of them.
    check_merge_approvals(store, policy, env)?;

    let branch = branch_hint(store, env);
    let mut stored = env.clone();
    stored.meta = rebuild_meta(store, &env.header, &branch, 0)?;
    store.put_commit(&stored)?;

    // Anything that was waiting on this commit, or on a record blob this
    // commit brought with it, can now be retried.
    let mut also = Vec::new();
    let mut keys: Vec<String> = vec![env.id.clone()];
    keys.extend(env.header.changes.iter().map(|c| c.record.clone()));
    for key in keys {
        wake(store, policy, &key, &mut also);
    }
    Ok(Received::Applied { also_applied: also })
}

/// Hold a commit on a key it is waiting for — a parent id or a record hash.
/// Idempotent: re-receiving an already-held commit must not queue it twice,
/// or `pending_all` starts double-counting.
fn hold(store: &mut dyn CommitStore, env: &CommitEnvelope, key: &str) -> Result<Received> {
    let already = store
        .pending_all()
        .iter()
        .any(|(k, e)| k == key && e.id == env.id);
    if !already {
        store.pending_put(key, env)?;
    }
    Ok(Received::Pending {
        missing_parent: key.to_string(),
    })
}

fn wake(
    store: &mut dyn CommitStore,
    policy: &dyn MemberPolicy,
    key: &str,
    also: &mut Vec<String>,
) {
    for env in store.pending_take(key) {
        // A failure here is dropped and logged by the caller of `receive`,
        // never propagated: one bad pending commit must not fail the apply of
        // the perfectly good commit that woke it.
        if let Ok(Received::Applied { also_applied }) = receive(store, policy, &env) {
            also.push(env.id.clone());
            also.extend(also_applied);
        }
    }
}

/// The union of every parent's state, highest version winning. For a single
/// parent this is just that commit's state; for a merge it is what a merged
/// version's `from_version` is measured against.
fn parent_state(
    store: &dyn CommitStore,
    parents: &[String],
) -> Result<BTreeMap<ObjectId, (u64, String)>> {
    let mut out: BTreeMap<ObjectId, (u64, String)> = BTreeMap::new();
    for parent in parents {
        for (object, version, hash) in record::live_objects(store, parent)? {
            match out.get(&object) {
                Some((v, _)) if *v > version => {}
                Some((v, h)) if *v == version && h.as_str() <= hash.as_str() => {}
                _ => {
                    out.insert(object, (version, hash));
                }
            }
        }
    }
    Ok(out)
}

/// Which branches a merge commit would land on, and therefore whether it is
/// gated. docs/10 §4.5 says the rule must be checkable by a peer, and the
/// header carries no branch (§1.5), so the target is derived: a merge whose
/// `parents[0]` is contained in a protected branch is a merge into it.
fn check_merge_approvals(
    store: &dyn CommitStore,
    policy: &dyn MemberPolicy,
    env: &CommitEnvelope,
) -> Result<()> {
    if env.header.parents.len() < 2 {
        // docs/10 §7 call 9: docs/05 protects MERGES. A direct commit onto
        // protected main is allowed.
        return Ok(());
    }
    let target = &env.header.parents[0];
    let mut needed = 0usize;
    for (name, refs) in store.refs() {
        if let RefTarget::Branch { commit } = refs {
            let required = policy.approvals_required(&name);
            if required == 0 {
                continue;
            }
            if &commit == target || is_ancestor(store, target, &commit)? {
                needed = needed.max(required);
            }
        }
    }
    if needed == 0 {
        return Ok(());
    }

    // Eligibility is evaluated at the merge base, so a membership change
    // landing mid-review cannot retroactively validate or invalidate an
    // approval (docs/10 §4.5).
    let bases = merge_base(store, &env.header.parents[0], &env.header.parents[1])?;
    let at = bases.first().map(|s| s.as_str());

    let mut counted: BTreeSet<&str> = BTreeSet::new();
    for (i, signer) in env.header.co_signers.iter().enumerate() {
        let Some(sig) = env.co_sigs.get(i) else {
            continue;
        };
        if verify_cosig(&env.id, signer, sig).is_err() {
            continue;
        }
        if signer == &env.header.author {
            continue;
        }
        if !policy.may_approve(signer, at) {
            continue;
        }
        counted.insert(signer.as_str());
    }
    if counted.len() < needed {
        return Err(VcsError::Refused(format!(
            "a merge into a protected branch needs {needed} approval(s); {} valid",
            counted.len()
        )));
    }
    Ok(())
}

/// A display hint only, and never trusted from a peer: the branch whose head
/// is this commit's first parent, if any.
fn branch_hint(store: &dyn CommitStore, env: &CommitEnvelope) -> String {
    let Some(parent) = env.header.parents.first() else {
        return "main".to_string();
    };
    for (name, refs) in store.refs() {
        if let RefTarget::Branch { commit } = refs {
            if &commit == parent {
                return name
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&name)
                    .to_string();
            }
        }
    }
    String::new()
}

/// Every commit reachable from `from`, including itself.
pub(crate) fn reachable(store: &dyn CommitStore, from: &str) -> Result<Vec<String>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![from.to_string()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        for parent in env.header.parents {
            stack.push(parent);
        }
    }
    Ok(seen.into_iter().collect())
}

/// `1 + max(gen of parents)`, `gen(root) = 0` (docs/10 §2.4).
///
/// Computed from the DAG rather than read out of `meta`: `meta` is local and
/// rebuilt on receipt, so trusting it here would make the merge base depend on
/// a field a peer could have influenced.
pub fn generation(store: &dyn CommitStore, id: &str) -> Result<u64> {
    let mut memo = HashMap::new();
    gen_of(store, id, &mut memo)
}

fn gen_of(
    store: &dyn CommitStore,
    id: &str,
    memo: &mut HashMap<String, u64>,
) -> Result<u64> {
    // Iterative so a long history cannot blow the stack.
    let mut stack: Vec<String> = vec![id.to_string()];
    while let Some(top) = stack.last().cloned() {
        if memo.contains_key(&top) {
            stack.pop();
            continue;
        }
        let env = store
            .commit(&top)
            .ok_or_else(|| VcsError::NotFound(format!("commit {top}")))?;
        let mut waiting = Vec::new();
        let mut gen = 0u64;
        for parent in &env.header.parents {
            match memo.get(parent) {
                Some(g) => gen = gen.max(g + 1),
                None => waiting.push(parent.clone()),
            }
        }
        if waiting.is_empty() {
            memo.insert(top, gen);
            stack.pop();
        } else {
            stack.extend(waiting);
        }
    }
    Ok(memo[id])
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
    const P1: u8 = 1;
    const P2: u8 = 2;
    const RESULT: u8 = 4;
    const STALE: u8 = 8;
    const BOTH: u8 = P1 | P2;

    if a == b {
        if store.commit(a).is_none() {
            return Err(VcsError::NotFound(format!("commit {a}")));
        }
        return Ok(vec![a.to_string()]);
    }

    let mut memo: HashMap<String, u64> = HashMap::new();
    let mut flags: HashMap<String, u8> = HashMap::new();
    let mut heap: BinaryHeap<(u64, Reverse<String>)> = BinaryHeap::new();

    flags.insert(a.to_string(), P1);
    heap.push((gen_of(store, a, &mut memo)?, Reverse(a.to_string())));
    flags.insert(b.to_string(), P2);
    heap.push((gen_of(store, b, &mut memo)?, Reverse(b.to_string())));

    let mut candidates: Vec<String> = Vec::new();
    while !heap.is_empty() {
        // Stop once nothing but settled both-coloured commits is left.
        if heap
            .iter()
            .all(|(_, Reverse(id))| flags.get(id).copied().unwrap_or(0) & (BOTH | STALE) == (BOTH | STALE))
        {
            break;
        }
        let (_, Reverse(id)) = heap.pop().expect("non-empty");
        let mut colour = flags.get(&id).copied().unwrap_or(0) & (BOTH | STALE);
        if colour & BOTH == BOTH {
            let entry = flags.entry(id.clone()).or_insert(0);
            if *entry & RESULT == 0 {
                *entry |= RESULT;
                candidates.push(id.clone());
            }
            colour |= STALE;
        }
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        for parent in &env.header.parents {
            // A parent's gen is strictly lower than its child's, and the heap
            // pops by gen, so every colouring of a commit happens before it
            // is popped. No commit ever needs re-visiting.
            let before = flags.get(parent).copied().unwrap_or(0);
            if before & colour == colour {
                continue;
            }
            flags.insert(parent.clone(), before | colour);
            if before == 0 {
                let g = gen_of(store, parent, &mut memo)?;
                heap.push((g, Reverse(parent.clone())));
            }
        }
    }

    // "Best" means maximal: drop any candidate that is an ancestor of
    // another, which is what the paint can leave behind.
    let mut best: Vec<String> = Vec::new();
    for x in &candidates {
        let mut redundant = false;
        for y in &candidates {
            if x != y && is_ancestor(store, x, y)? {
                redundant = true;
                break;
            }
        }
        if !redundant {
            best.push(x.clone());
        }
    }
    best.sort();
    best.dedup();
    Ok(best)
}

/// True when `ancestor` is reachable from `descendant`.
pub fn is_ancestor(store: &dyn CommitStore, ancestor: &str, descendant: &str) -> Result<bool> {
    if ancestor == descendant {
        return Ok(true);
    }
    let mut memo = HashMap::new();
    let target_gen = gen_of(store, ancestor, &mut memo)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![descendant.to_string()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if id == ancestor {
            return Ok(true);
        }
        if gen_of(store, &id, &mut memo)? <= target_gen {
            // Nothing below this can have a higher generation, so the
            // ancestor cannot be there.
            continue;
        }
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        stack.extend(env.header.parents);
    }
    Ok(false)
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
///
/// Order is `(gen, timestamp, id)` descending, which is a valid topological
/// order because a child's generation is always strictly greater than its
/// parents'. It is also total, so two nodes holding the same DAG draw the same
/// graph. `limit` 0 means no limit.
pub fn log(store: &dyn CommitStore, from: &str, limit: usize) -> Result<Vec<LogRow>> {
    let mut memo: HashMap<String, u64> = HashMap::new();
    let mut heap: BinaryHeap<(u64, i64, Reverse<String>)> = BinaryHeap::new();
    let mut queued: HashSet<String> = HashSet::new();

    let push = |store: &dyn CommitStore,
                heap: &mut BinaryHeap<(u64, i64, Reverse<String>)>,
                queued: &mut HashSet<String>,
                memo: &mut HashMap<String, u64>,
                id: &str|
     -> Result<()> {
        if !queued.insert(id.to_string()) {
            return Ok(());
        }
        let env = store
            .commit(id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        let g = gen_of(store, id, memo)?;
        heap.push((g, env.header.timestamp, Reverse(id.to_string())));
        Ok(())
    };

    push(store, &mut heap, &mut queued, &mut memo, from)?;

    let mut ordered: Vec<CommitEnvelope> = Vec::new();
    while let Some((_, _, Reverse(id))) = heap.pop() {
        let env = store
            .commit(&id)
            .ok_or_else(|| VcsError::NotFound(format!("commit {id}")))?;
        for parent in &env.header.parents {
            push(store, &mut heap, &mut queued, &mut memo, parent)?;
        }
        ordered.push(env);
        if limit > 0 && ordered.len() >= limit {
            break;
        }
    }

    // Lane 0 starts at `from` and is always handed on to parents[0], so lane
    // 0 IS the first-parent chain.
    let mut lanes: Vec<Option<String>> = vec![Some(from.to_string())];
    let mut rows = Vec::with_capacity(ordered.len());
    for env in ordered {
        let expecting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.as_deref() == Some(env.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let lane = match expecting.first() {
            Some(i) => *i,
            None => free_lane(&mut lanes),
        };
        // A commit more than one lane was waiting for is where a branch forks
        // off (walking newest-first, that reads as two lines joining).
        let fork = expecting.len() > 1;
        let through: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| l.is_some() && *i != lane && !expecting.contains(i))
            .map(|(i, _)| i.min(MAX_LANES))
            .collect::<BTreeSet<usize>>()
            .into_iter()
            .collect();
        for i in expecting.iter().skip(1) {
            lanes[*i] = None;
        }
        lanes[lane] = env.header.parents.first().cloned();
        for parent in env.header.parents.iter().skip(1) {
            if lanes.iter().any(|l| l.as_deref() == Some(parent.as_str())) {
                continue;
            }
            let slot = free_lane(&mut lanes);
            lanes[slot] = Some(parent.clone());
        }
        let merge = env.header.parents.len() > 1;
        rows.push(LogRow {
            commit: env,
            lane: lane.min(MAX_LANES),
            through,
            fork,
            merge,
        });
    }
    Ok(rows)
}

fn free_lane(lanes: &mut Vec<Option<String>>) -> usize {
    match lanes.iter().position(|l| l.is_none()) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn full_ref(name: &str) -> String {
    if name.starts_with("refs/") || name == store::HEAD {
        name.to_string()
    } else {
        store::branch_ref(name)
    }
}

/// Create a branch at a commit. Fails if the name already exists.
pub fn branch_create(store: &mut dyn CommitStore, name: &str, at: &str) -> Result<()> {
    let full = full_ref(name);
    if store.ref_get(&full).is_some() {
        return Err(VcsError::Refused(format!("{full} already exists")));
    }
    if store.commit(at).is_none() {
        return Err(VcsError::NotFound(format!("commit {at}")));
    }
    store.ref_set(
        &full,
        RefTarget::Branch {
            commit: at.to_string(),
        },
    )
}

/// Delete a branch ref. Commits become unreachable, not gone — history is
/// never rewritten and another ref may still reach them. `main` is protected.
pub fn branch_delete(store: &mut dyn CommitStore, name: &str) -> Result<()> {
    let full = full_ref(name);
    if full == store::MAIN {
        return Err(VcsError::Refused("main cannot be deleted".into()));
    }
    if store.ref_get(&full).is_none() {
        return Err(VcsError::NotFound(full));
    }
    store.ref_remove(&full)
}

/// Create a signed tag. Immutable: re-tagging an existing name is an error.
pub fn tag_create(
    store: &mut dyn CommitStore,
    signing: &ed25519_dalek::SigningKey,
    name: &str,
    commit: &str,
) -> Result<()> {
    let full = if name.starts_with("refs/tags/") {
        name.to_string()
    } else {
        store::tag_ref(name)
    };
    if store.ref_get(&full).is_some() {
        return Err(VcsError::Refused(format!("tag {full} already exists")));
    }
    if store.commit(commit).is_none() {
        return Err(VcsError::NotFound(format!("commit {commit}")));
    }
    let mut buf = TAG_DOMAIN.to_vec();
    buf.extend_from_slice(full.as_bytes());
    buf.push(0);
    buf.extend_from_slice(&hashing::unhex32(commit)?);
    let sig = hex::encode(signing.sign(&buf).to_bytes());
    store.ref_set(
        &full,
        RefTarget::Tag {
            commit: commit.to_string(),
            tagger: hashing::hex32(&signing.verifying_key().to_bytes()),
            sig,
        },
    )
}

/// Verify a tag's signature.
pub fn verify_tag(name: &str, target: &RefTarget) -> Result<()> {
    let RefTarget::Tag {
        commit,
        tagger,
        sig,
    } = target
    else {
        return Err(VcsError::Malformed(format!("{name} is not a tag")));
    };
    let mut buf = TAG_DOMAIN.to_vec();
    buf.extend_from_slice(name.as_bytes());
    buf.push(0);
    buf.extend_from_slice(&hashing::unhex32(commit)?);
    verifying_key(tagger)?
        .verify(&buf, &parse_sig(sig)?)
        .map_err(|e| VcsError::BadSignature(format!("tag {name}: {e}")))
}

/// Resolve `HEAD`, following one level of symbolic indirection.
pub fn resolve_head(store: &dyn CommitStore) -> Result<Option<String>> {
    match store.ref_get(store::HEAD) {
        None => Ok(None),
        Some(RefTarget::Detached { commit }) => Ok(Some(commit)),
        Some(RefTarget::Branch { commit }) => Ok(Some(commit)),
        Some(RefTarget::Symbolic { to }) => match store.ref_get(&to) {
            Some(RefTarget::Branch { commit }) => Ok(Some(commit)),
            // An unborn branch: HEAD names it, but nothing points at a commit
            // yet. Not an error — it is the state of a project between
            // `init` and its first commit.
            None => Ok(None),
            Some(other) => Err(VcsError::Malformed(format!(
                "HEAD points at {to}, which is {other:?}"
            ))),
        },
        Some(RefTarget::Tag { commit, .. }) => Ok(Some(commit)),
    }
}

/// Resolve any ref name or raw commit id to a commit id.
pub fn resolve(store: &dyn CommitStore, name_or_id: &str) -> Result<Option<String>> {
    if name_or_id == store::HEAD {
        return resolve_head(store);
    }
    if store.commit(name_or_id).is_some() {
        return Ok(Some(name_or_id.to_string()));
    }
    for candidate in [
        name_or_id.to_string(),
        store::branch_ref(name_or_id),
        store::tag_ref(name_or_id),
    ] {
        match store.ref_get(&candidate) {
            Some(RefTarget::Branch { commit }) | Some(RefTarget::Tag { commit, .. }) => {
                return Ok(Some(commit))
            }
            Some(RefTarget::Detached { commit }) => return Ok(Some(commit)),
            Some(RefTarget::Symbolic { to }) => return resolve(store, &to),
            None => continue,
        }
    }
    Ok(None)
}

/// Move a branch ref, refusing anything that is not a fast-forward unless
/// `force` is set. Used by merge and by sync's head exchange.
pub fn set_branch(
    store: &mut dyn CommitStore,
    name: &str,
    to: &str,
    force: bool,
) -> Result<RefTarget> {
    let full = full_ref(name);
    if store.commit(to).is_none() {
        return Err(VcsError::NotFound(format!("commit {to}")));
    }
    if let Some(RefTarget::Branch { commit }) = store.ref_get(&full) {
        if !force && !is_ancestor(store, &commit, to)? {
            return Err(VcsError::Refused(format!(
                "moving {full} from {commit} to {to} is not a fast-forward"
            )));
        }
    }
    let target = RefTarget::Branch {
        commit: to.to_string(),
    };
    store.ref_set(&full, target.clone())?;
    Ok(target)
}

/// Re-derive `CommitMeta` locally. A peer's `meta` is never trusted: it is
/// discarded on receipt and rebuilt here (docs/10 §1.5).
pub fn rebuild_meta(
    store: &dyn CommitStore,
    header: &CommitHeader,
    branch: &str,
    received_at: i64,
) -> Result<CommitMeta> {
    let mut memo = HashMap::new();
    let mut gen = 0u64;
    for parent in &header.parents {
        gen = gen.max(gen_of(store, parent, &mut memo)? + 1);
    }
    Ok(CommitMeta {
        branch: branch.to_string(),
        received_at,
        gen,
        // Anchoring is `anchor::AnchorQueue`'s business and is never carried
        // across from a peer.
        anchor: None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    //! Shared fixtures live here (`pub(crate)`) because `lib.rs` is fixed and
    //! a test-only module cannot be added to it. `record` and `merge` build
    //! their histories with these.

    use super::*;
    use crate::store::{MemStore, MAIN};
    use crate::types::{ChangeKind, Record};
    use ed25519_dalek::SigningKey;
    use serde_json::{json, Value};

    pub(crate) const PROJECT: &str = "olb:proj:mfrgg2lo";
    /// One fixed timestamp everywhere: `log`'s order is total without it
    /// (`(gen, timestamp, id)` falls through to the id), and a clock in a test
    /// fixture only buys nondeterminism.
    pub(crate) const TS: i64 = 1_761_480_000_000;

    pub(crate) fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub(crate) fn pk(k: &SigningKey) -> String {
        hashing::hex32(&k.verifying_key().to_bytes())
    }

    /// A version-1 record with its object id derived from its own content.
    ///
    /// Note the two-step: the id is the hash of version 1 (docs/10 §3.1), and
    /// the record carries its own id, so the hash is taken over the draft with
    /// an empty `id` and then stamped in. That is the only way the identity
    /// can be a function of the content without being circular, and it is what
    /// `object_id_of_first_version` is for.
    pub(crate) fn create(kind: &str, fields: Value) -> Record {
        let mut draft = Record {
            id: ObjectId(String::new()),
            kind: kind.to_string(),
            version: 1,
            project: PROJECT.to_string(),
            idea: None,
            authors: Vec::new(),
            agents: Vec::new(),
            parents: Vec::new(),
            prev: Vec::new(),
            fields: fields.as_object().cloned().unwrap_or_default(),
            attachments: Vec::new(),
            flags: serde_json::Map::new(),
            visibility: "project".to_string(),
            created_at: "2026-10-01T00:00:00Z".to_string(),
        };
        draft.id = record::object_id_of_first_version(&draft).expect("version 1");
        draft
    }

    pub(crate) fn edit(previous: &Record, mutate: impl FnOnce(&mut Record)) -> Record {
        record::next_version(previous, mutate).expect("next version")
    }

    pub(crate) fn set_field(rec: &mut Record, name: &str, value: Value) {
        rec.fields.insert(name.to_string(), value);
    }

    /// Build a commit that writes `writes`, with the tree and the version
    /// transitions computed the way `receive` will recompute them. Does not
    /// apply it.
    pub(crate) fn build_on(
        store: &mut MemStore,
        signer: &SigningKey,
        parents: &[&str],
        message: &str,
        writes: &[(Record, ChangeKind)],
        cosigners: &[&SigningKey],
    ) -> CommitEnvelope {
        let parents: Vec<String> = parents.iter().map(|p| p.to_string()).collect();
        let mut state = parent_state(store, &parents).expect("parent state");
        let mut changes = Vec::new();
        for (rec, kind) in writes {
            store.put_record(rec).expect("put record");
            let head = state.get(&rec.id).map(|(v, _)| *v).unwrap_or(0);
            changes.push(Change {
                object: rec.id.clone(),
                from_version: head,
                to_version: rec.version,
                kind: *kind,
                record: record::content_hash(rec).expect("hash"),
                restored_from: None,
            });
        }
        for ch in &changes {
            state.insert(ch.object.clone(), (ch.to_version, ch.record.clone()));
        }
        let live: Vec<(ObjectId, u64, String)> = state
            .iter()
            .map(|(o, (v, h))| (o.clone(), *v, h.clone()))
            .collect();
        let tree = record::state_root(&live).expect("state root");
        let co: Vec<String> = cosigners.iter().map(|k| pk(k)).collect();
        let mut env = build_commit_cosigned(
            signer,
            PROJECT,
            parents,
            tree,
            message.to_string(),
            changes,
            Vec::new(),
            Vec::new(),
            TS,
            &co,
        )
        .expect("build");
        for k in cosigners {
            attach_cosig(&mut env, k).expect("cosign");
        }
        env
    }

    /// Build and apply, asserting it landed. Returns the commit id.
    pub(crate) fn commit_on(
        store: &mut MemStore,
        signer: &SigningKey,
        parents: &[&str],
        message: &str,
        writes: &[(Record, ChangeKind)],
    ) -> String {
        let env = build_on(store, signer, parents, message, writes, &[]);
        let outcome = receive(store, &OpenPolicy, &env).expect("receive");
        assert!(
            matches!(outcome, Received::Applied { .. }),
            "{message}: {outcome:?}"
        );
        env.id
    }

    /// A store with one root commit creating one hypothesis, `main` and HEAD.
    pub(crate) fn scene() -> (MemStore, SigningKey, String, Record) {
        let alice = key(1);
        let mut store = MemStore::new();
        let h = create("hypothesis", json!({"statement": "Kinase X gates entry"}));
        let root = commit_on(
            &mut store,
            &alice,
            &[],
            "Open the hypothesis",
            &[(h.clone(), ChangeKind::Create)],
        );
        set_branch(&mut store, MAIN, &root, true).expect("main");
        store
            .ref_set(
                crate::store::HEAD,
                RefTarget::Symbolic {
                    to: MAIN.to_string(),
                },
            )
            .expect("head");
        (store, alice, root, h)
    }

    /// `approvals_required` on `main` only; everything else open. `reviewers`
    /// are members who may write but may NOT approve (docs/05 §3
    /// Protection), and `joined_late` may only approve at commits at or after
    /// the commit they joined at.
    pub(crate) struct ProtectedMain {
        pub needed: usize,
        pub reviewers: Vec<String>,
        pub joined_late: Vec<(String, String)>,
    }

    impl ProtectedMain {
        pub(crate) fn new(needed: usize) -> Self {
            Self {
                needed,
                reviewers: Vec::new(),
                joined_late: Vec::new(),
            }
        }
    }

    impl MemberPolicy for ProtectedMain {
        fn may_write(&self, _author: &str, _at: Option<&str>) -> bool {
            true
        }
        fn may_approve(&self, key_hex: &str, at: Option<&str>) -> bool {
            if self.reviewers.iter().any(|r| r == key_hex) {
                return false;
            }
            for (k, joined_at) in &self.joined_late {
                if k == key_hex {
                    return at == Some(joined_at.as_str());
                }
            }
            true
        }
        fn approvals_required(&self, branch: &str) -> usize {
            if branch == MAIN {
                self.needed
            } else {
                0
            }
        }
    }

    // ---- docs/10 §6.1: id and signatures ----

    #[test]
    fn test2_field_insertion_order_does_not_change_the_id() {
        // Two spellings of one header. The struct fixes Rust's field order,
        // so the risk this guards is on the JSON side: `canon` sorting keys,
        // which is what the id is actually taken over.
        let header = json!({
            "v": 1, "project": PROJECT, "parents": [], "tree": "00",
            "author": "ab", "co_signers": [], "message": "m",
            "changes": [], "files_added": [], "files_removed": [], "timestamp": TS
        });
        let shuffled = json!({
            "timestamp": TS, "message": "m", "author": "ab", "tree": "00",
            "files_removed": [], "files_added": [], "changes": [],
            "co_signers": [], "parents": [], "project": PROJECT, "v": 1
        });
        let a = hashing::digest(&hashing::preimage(hashing::COMMIT, &header).unwrap());
        let b = hashing::digest(&hashing::preimage(hashing::COMMIT, &shuffled).unwrap());
        assert_eq!(hashing::hex32(&a), hashing::hex32(&b));
    }

    #[test]
    fn test2_input_order_of_parents_changes_and_files_does_not_change_the_id() {
        let alice = key(1);
        let x = create("experiment", json!({"title": "Run 14"}));
        let y = create("experiment", json!({"title": "Run 15"}));
        let mk = |flip: bool| {
            let mut changes = vec![
                Change {
                    object: x.id.clone(),
                    from_version: 0,
                    to_version: 1,
                    kind: ChangeKind::Create,
                    record: record::content_hash(&x).unwrap(),
                    restored_from: None,
                },
                Change {
                    object: y.id.clone(),
                    from_version: 0,
                    to_version: 1,
                    kind: ChangeKind::Create,
                    record: record::content_hash(&y).unwrap(),
                    restored_from: None,
                },
            ];
            let mut parents = vec!["a".repeat(64), "b".repeat(64), "c".repeat(64)];
            let mut files = vec!["logos:z".to_string(), "logos:a".to_string()];
            if flip {
                changes.reverse();
                parents.swap(1, 2);
                files.reverse();
            }
            build_commit(
                &alice,
                PROJECT,
                parents,
                "t".repeat(64),
                "Run 14".into(),
                changes,
                files,
                vec![],
                TS,
            )
            .unwrap()
        };
        assert_eq!(mk(false).id, mk(true).id);
        // parents[0] is NOT sorted: it is the merge target, and the
        // first-parent chain is the mainline.
        assert_eq!(mk(true).header.parents[0], "a".repeat(64));
        assert_eq!(mk(true).header.parents[1], "b".repeat(64));
    }

    #[test]
    fn test3_known_answer_commit_id() {
        // The regression net for docs/10 §1.2's `preserve_order` hazard: if
        // the canonical encoding moves, this fails loudly instead of
        // silently rewriting every id in every store.
        //
        // Lives here rather than in `fixtures/vcs-vectors.json` because that
        // file does not exist yet and is outside this crate.
        let alice = key(1);
        let env = build_commit(
            &alice,
            "olb:proj:mfrgg2lo",
            vec![],
            "0".repeat(64),
            "Run 14: add plate 3 melt curves".into(),
            vec![],
            vec!["logos:cid1".into()],
            vec![],
            1_761_480_000_000,
        )
        .unwrap();
        assert_eq!(
            env.header.author,
            "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
        );
        assert_eq!(
            env.id,
            "4e9c356803a4318404505fc01db196c9346eb4a8e8c24b70c8d482bd0ca48e8d"
        );
        // Ed25519 is deterministic (RFC 8032), so the signature is a
        // known answer too: it pins the preimage, not just the digest.
        assert_eq!(
            env.sig,
            "9d84c3bbfdea122b5a7bcc94b9148db1c4d3ead96fcb066bf0e51bcf107e7a95\
             4ab0c573595b53ab39a24be23433c61c3ae98e34366989cbc34902a9b78f7f0c"
        );
        // And the canonical bytes themselves, which is where the hazard is.
        assert_eq!(
            String::from_utf8(crate::canon::to_vec_of(&env.header).unwrap()).unwrap(),
            r#"{"author":"8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c","changes":[],"co_signers":[],"files_added":["logos:cid1"],"files_removed":[],"message":"Run 14: add plate 3 melt curves","parents":[],"project":"olb:proj:mfrgg2lo","timestamp":1761480000000,"tree":"0000000000000000000000000000000000000000000000000000000000000000","v":1}"#
        );
    }

    #[test]
    fn test6_a_signature_covers_every_header_field() {
        let alice = key(1);
        let env = build_commit(
            &alice,
            PROJECT,
            vec!["a".repeat(64)],
            "t".repeat(64),
            "Run 14".into(),
            vec![],
            vec![],
            vec![],
            TS,
        )
        .unwrap();
        verify_commit(&env).expect("the commit as built verifies");

        let mut flipped = env.clone();
        flipped.header.message = "Run 15".into();
        assert!(verify_commit(&flipped).is_err(), "message is covered");

        let mut flipped = env.clone();
        flipped.header.parents = vec!["b".repeat(64)];
        assert!(verify_commit(&flipped).is_err(), "parents are covered");

        let mut flipped = env.clone();
        flipped.header.timestamp += 1;
        assert!(verify_commit(&flipped).is_err(), "timestamp is covered");

        let mut flipped = env.clone();
        flipped.header.v = 2;
        assert!(verify_commit(&flipped).is_err(), "v is inside the header");

        // Swapping the author and recomputing the id — the one attack an
        // unauthenticated author field would allow — fails on the signature.
        let bob = key(2);
        let mut swapped = env.clone();
        swapped.header.author = pk(&bob);
        let preimage = hashing::preimage(hashing::COMMIT, &swapped.header).unwrap();
        swapped.id = hashing::hex32(&hashing::digest(&preimage));
        assert!(
            matches!(verify_commit(&swapped), Err(VcsError::BadSignature(_))),
            "a re-hashed author swap must fail on the signature"
        );
    }

    #[test]
    fn test7_meta_is_not_hashed_but_the_header_is() {
        let alice = key(1);
        let mut env = build_commit(
            &alice,
            PROJECT,
            vec![],
            "t".repeat(64),
            "Run 14".into(),
            vec![],
            vec![],
            vec![],
            TS,
        )
        .unwrap();
        let id = env.id.clone();
        env.meta.branch = "h2-computational".into();
        env.meta.gen = 97;
        env.meta.received_at = 42;
        verify_commit(&env).expect("meta is not covered by the id or the signature");
        assert_eq!(env.id, id);

        env.header.tree = "f".repeat(64);
        assert!(verify_commit(&env).is_err(), "the tree IS covered");
    }

    #[test]
    fn test8_cosignatures_are_domain_separated_and_bound_to_one_commit() {
        let alice = key(1);
        let priya = key(3);
        let a = build_commit(
            &alice,
            PROJECT,
            vec![],
            "t".repeat(64),
            "A".into(),
            vec![],
            vec![],
            vec![],
            TS,
        )
        .unwrap();
        let b = build_commit(
            &alice,
            PROJECT,
            vec![],
            "t".repeat(64),
            "B".into(),
            vec![],
            vec![],
            vec![],
            TS,
        )
        .unwrap();
        let sig = cosign(&priya, &a.id).unwrap();
        verify_cosig(&a.id, &pk(&priya), &sig).expect("verifies for A");
        assert!(
            verify_cosig(&b.id, &pk(&priya), &sig).is_err(),
            "a co-signature for A must not verify for B"
        );
        // The author's own signature is over the preimage, not the id, so it
        // can never be replayed as a co-signature.
        assert!(verify_cosig(&a.id, &a.header.author, &a.sig).is_err());

        // The same key twice counts once, at build time.
        let env = build_commit_cosigned(
            &alice,
            PROJECT,
            vec![],
            "t".repeat(64),
            "A".into(),
            vec![],
            vec![],
            vec![],
            TS,
            &[pk(&priya), pk(&priya)],
        )
        .unwrap();
        assert_eq!(env.header.co_signers.len(), 1);
    }

    // ---- docs/10 §6.3: the DAG ----

    #[test]
    fn test18_one_root_with_no_parents() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("Kinase X is required")));
        let c2 = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Sharpen the statement",
            &[(v2, ChangeKind::Edit)],
        );
        set_branch(&mut store, MAIN, &c2, false).unwrap();

        assert!(store.commit(&root).unwrap().header.parents.is_empty());
        let roots: Vec<String> = reachable(&store, &c2)
            .unwrap()
            .into_iter()
            .filter(|id| store.commit(id).unwrap().header.parents.is_empty())
            .collect();
        assert_eq!(roots, vec![root], "exactly one root is reachable");
    }

    #[test]
    fn test19_generations_and_merge_base_on_a_chain() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)]);
        let v3 = edit(&v2, |r| set_field(r, "statement", json!("three")));
        let c3 = commit_on(&mut store, &alice, &[&c2], "three", &[(v3, ChangeKind::Edit)]);

        assert_eq!(generation(&store, &root).unwrap(), 0);
        assert_eq!(generation(&store, &c2).unwrap(), 1);
        assert_eq!(generation(&store, &c3).unwrap(), 2);
        assert_eq!(store.commit(&c3).unwrap().meta.gen, 2, "meta carries it too");

        assert_eq!(merge_base(&store, &c3, &c3).unwrap(), vec![c3.clone()]);
        assert_eq!(merge_base(&store, &c3, &root).unwrap(), vec![root.clone()]);
        assert_eq!(merge_base(&store, &root, &c3).unwrap(), vec![root.clone()]);
        assert!(is_ancestor(&store, &root, &c3).unwrap());
        assert!(!is_ancestor(&store, &c3, &root).unwrap());
    }

    /// root -> (b edits the hypothesis) and (c creates an experiment), then a
    /// merge of c into b.
    pub(crate) fn forked() -> (MemStore, SigningKey, String, String, String, Record) {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("Kinase X is required")));
        let ours = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Sharpen the hypothesis",
            &[(v2, ChangeKind::Edit)],
        );
        let run = create("experiment", json!({"title": "Run 14"}));
        let theirs = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Plan run 14",
            &[(run, ChangeKind::Create)],
        );
        (store, alice, root, ours, theirs, h)
    }

    #[test]
    fn test20_a_merge_has_two_parents_and_the_target_comes_first() {
        let (mut store, alice, root, ours, theirs, _h) = forked();
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        let merge = commit_on(&mut store, &alice, &[&ours, &theirs], "Merge run 14", &[]);

        let env = store.commit(&merge).unwrap();
        assert_eq!(env.header.parents.len(), 2);
        assert_eq!(env.header.parents[0], ours, "parents[0] is the target");
        assert_eq!(env.header.parents[1], theirs);

        // The first-parent walk is the mainline and nothing else.
        let mut spine = vec![merge.clone()];
        let mut cursor = merge.clone();
        while let Some(p) = store.commit(&cursor).unwrap().header.parents.first().cloned() {
            spine.push(p.clone());
            cursor = p;
        }
        assert_eq!(spine, vec![merge, ours, root]);
    }

    #[test]
    fn test20_lane_zero_is_the_first_parent_chain() {
        let (mut store, alice, root, ours, theirs, _h) = forked();
        let merge = commit_on(&mut store, &alice, &[&ours, &theirs], "Merge run 14", &[]);
        let rows = log(&store, &merge, 0).unwrap();
        let lane0: Vec<&str> = rows
            .iter()
            .filter(|r| r.lane == 0)
            .map(|r| r.commit.id.as_str())
            .collect();
        assert_eq!(lane0, vec![merge.as_str(), ours.as_str(), root.as_str()]);
        assert!(rows[0].merge, "the newest row is the merge");
        assert!(rows.iter().any(|r| r.lane == 1 && r.commit.id == theirs));
    }

    #[test]
    fn test21_criss_cross_reports_conflicts_rather_than_inventing_a_base() {
        let (mut store, alice, root, h) = scene();
        let hb = edit(&h, |r| set_field(r, "statement", json!("from the b side")));
        let hc = edit(&h, |r| set_field(r, "statement", json!("from the c side")));
        let b = commit_on(&mut store, &alice, &[&root], "b", &[(hb.clone(), ChangeKind::Edit)]);
        let c = commit_on(&mut store, &alice, &[&root], "c", &[(hc.clone(), ChangeKind::Edit)]);

        let mut m1_rec = record::merged_version(&[&hb, &hc], hb.fields.clone()).unwrap();
        set_field(&mut m1_rec, "statement", json!("reconciled one way"));
        let m1 = commit_on(
            &mut store,
            &alice,
            &[&b, &c],
            "Merge c into b",
            &[(m1_rec, ChangeKind::Merge)],
        );
        let mut m2_rec = record::merged_version(&[&hc, &hb], hc.fields.clone()).unwrap();
        set_field(&mut m2_rec, "statement", json!("reconciled the other way"));
        let m2 = commit_on(
            &mut store,
            &alice,
            &[&c, &b],
            "Merge b into c",
            &[(m2_rec, ChangeKind::Merge)],
        );

        let bases = merge_base(&store, &m1, &m2).unwrap();
        assert_eq!(bases.len(), 2, "criss-cross: {bases:?}");
        let mut expected = vec![b, c];
        expected.sort();
        assert_eq!(bases, expected);

        let outcome = crate::merge::merge(&store, &m1, &m2, false).unwrap();
        match outcome {
            crate::merge::MergeOutcome::Conflicted(conflicts) => {
                assert!(
                    conflicts.iter().any(|c| c.reason.contains("common ancestors")),
                    "the criss-cross itself must be reported: {conflicts:?}"
                );
            }
            other => panic!("multiple bases must never auto-resolve: {other:?}"),
        }
    }

    #[test]
    fn test22_a_child_delivered_before_its_parent_waits_for_it() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let parent = build_on(&mut store, &alice, &[&root], "two", &[(v2.clone(), ChangeKind::Edit)], &[]);
        // The child is built against a store that has the parent, then the
        // parent is removed from the picture by replaying into a fresh store.
        let mut authoring = store.clone();
        receive(&mut authoring, &OpenPolicy, &parent).unwrap();
        let v3 = edit(&v2, |r| set_field(r, "statement", json!("three")));
        let child = build_on(
            &mut authoring,
            &alice,
            &[&parent.id],
            "three",
            &[(v3.clone(), ChangeKind::Edit)],
            &[],
        );

        // Delivered out of order into the original store.
        store.put_record(&v2).unwrap();
        store.put_record(&v3).unwrap();
        let outcome = receive(&mut store, &OpenPolicy, &child).unwrap();
        assert_eq!(
            outcome,
            Received::Pending {
                missing_parent: parent.id.clone()
            }
        );
        assert!(
            !store.has_commit(&child.id),
            "the store must never hold a commit with an unknown parent"
        );

        let outcome = receive(&mut store, &OpenPolicy, &parent).unwrap();
        assert_eq!(
            outcome,
            Received::Applied {
                also_applied: vec![child.id.clone()]
            },
            "landing the parent must land the child too"
        );
        assert!(store.has_commit(&child.id));
        assert!(store.pending_all().is_empty());
    }

    #[test]
    fn test23_an_orphan_waits_forever_without_blocking_anything() {
        let (mut store, alice, root, h) = scene();
        let orphan_parent = "f".repeat(64);
        let stray = create("experiment", json!({"title": "from a node we cannot see"}));
        store.put_record(&stray).unwrap();
        let orphan = build_commit(
            &alice,
            PROJECT,
            vec![orphan_parent.clone()],
            "0".repeat(64),
            "orphan".into(),
            vec![Change {
                object: stray.id.clone(),
                from_version: 0,
                to_version: 1,
                kind: ChangeKind::Create,
                record: record::content_hash(&stray).unwrap(),
                restored_from: None,
            }],
            vec![],
            vec![],
            TS,
        )
        .unwrap();
        assert_eq!(
            receive(&mut store, &OpenPolicy, &orphan).unwrap(),
            Received::Pending {
                missing_parent: orphan_parent.clone()
            }
        );
        // Re-delivery does not double-count it.
        receive(&mut store, &OpenPolicy, &orphan).unwrap();
        assert_eq!(store.pending_all().len(), 1);

        // An unrelated commit lands normally.
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)]);
        assert!(store.has_commit(&c2));
        assert_eq!(
            store.pending_all().len(),
            1,
            "the orphan is still reported as pending"
        );
    }

    #[test]
    fn test24_branches_are_refs_and_deleting_one_deletes_no_commits() {
        let (mut store, alice, root, h) = scene();
        let before = store.commit_count();
        branch_create(&mut store, "h2-computational", &root).unwrap();
        branch_create(&mut store, "docking", &root).unwrap();
        assert_eq!(store.commit_count(), before, "a branch creates no commit");
        assert!(
            branch_create(&mut store, "h2-computational", &root).is_err(),
            "a name is taken once"
        );

        // Two branches at one commit are independent.
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let c2 = commit_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)]);
        set_branch(&mut store, "h2-computational", &c2, false).unwrap();
        assert!(matches!(
            store.ref_get(&crate::store::branch_ref("docking")),
            Some(RefTarget::Branch { commit }) if commit == root
        ));

        tag_create(&mut store, &alice, "preprint-1", &c2).unwrap();
        branch_delete(&mut store, "h2-computational").unwrap();
        assert!(store
            .ref_get(&crate::store::branch_ref("h2-computational"))
            .is_none());
        assert!(store.has_commit(&c2), "history is never rewritten");
        assert_eq!(
            resolve(&store, "preprint-1").unwrap(),
            Some(c2),
            "a tag still reaches it"
        );
        assert!(branch_delete(&mut store, "main").is_err(), "main is protected");
    }

    #[test]
    fn test25_a_tag_cannot_be_moved_and_outlives_its_branch() {
        let (mut store, alice, root, _h) = scene();
        branch_create(&mut store, "side", &root).unwrap();
        tag_create(&mut store, &alice, "v1", &root).unwrap();
        assert!(
            tag_create(&mut store, &alice, "v1", &root).is_err(),
            "re-tagging an existing name is an error, not an overwrite"
        );
        let target = store.ref_get(&crate::store::tag_ref("v1")).unwrap();
        verify_tag("refs/tags/v1", &target).expect("a tag is signed");
        branch_delete(&mut store, "side").unwrap();
        assert_eq!(resolve(&store, "v1").unwrap(), Some(root));
    }

    #[test]
    fn test26_a_direct_commit_onto_protected_main_is_allowed() {
        // docs/10 §7 call 9: docs/05 protects MERGES into main. A direct,
        // non-merge commit is not gated. Flagged there as a one-line change
        // if that reading is wrong.
        let (mut store, alice, root, h) = scene();
        let policy = ProtectedMain::new(1);
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let env = build_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)], &[]);
        let outcome = receive(&mut store, &policy, &env).expect("a direct commit is allowed");
        assert!(matches!(outcome, Received::Applied { .. }));
    }

    #[test]
    fn test27_lanes_are_deterministic_and_collapse_past_the_fourth() {
        let (mut store, alice, root, _h) = scene();
        let mut heads = vec![];
        for i in 0..5u8 {
            let rec = create("experiment", json!({"title": format!("Run {i}")}));
            heads.push(commit_on(
                &mut store,
                &alice,
                &[&root],
                "branch",
                &[(rec, ChangeKind::Create)],
            ));
        }
        let mut parents: Vec<&str> = vec![&root];
        parents.extend(heads.iter().map(|h| h.as_str()));
        let merge = commit_on(&mut store, &alice, &parents, "Merge everything", &[]);

        let rows = log(&store, &merge, 0).unwrap();
        assert_eq!(rows, log(&store, &merge, 0).unwrap(), "deterministic");
        assert_eq!(rows[0].lane, 0);
        assert!(rows[0].merge);
        assert!(
            rows.iter().all(|r| r.lane <= MAX_LANES),
            "lanes past the gutter collapse to the +n marker"
        );
        assert!(
            rows.iter().all(|r| r.through.iter().all(|t| *t <= MAX_LANES)),
            "and so does the gutter"
        );
        assert_eq!(rows.last().unwrap().commit.id, root, "the root is oldest");
        assert_eq!(rows.last().unwrap().lane, 0, "the root is on the spine");
        assert_eq!(log(&store, &merge, 2).unwrap().len(), 2, "limit applies");
    }

    #[test]
    fn test41_a_peer_drops_a_merge_into_main_without_enough_cosignatures() {
        let (mut store, alice, _root, ours, theirs, _h) = forked();
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        let priya = key(3);
        let sam = key(4);
        let policy = ProtectedMain::new(2);

        let one = build_on(&mut store, &alice, &[&ours, &theirs], "Merge", &[], &[&priya]);
        assert!(
            matches!(receive(&mut store, &policy, &one), Err(VcsError::Refused(_))),
            "one approval is not two"
        );
        assert!(!store.has_commit(&one.id), "and it is not stored");

        let two = build_on(
            &mut store,
            &alice,
            &[&ours, &theirs],
            "Merge",
            &[],
            &[&priya, &sam],
        );
        assert!(matches!(
            receive(&mut store, &policy, &two).unwrap(),
            Received::Applied { .. }
        ));

        // The author's own co-signature does not count.
        let (mut store, alice, _root, ours, theirs, _h) = forked();
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        let policy = ProtectedMain::new(1);
        let selfie = build_on(&mut store, &alice, &[&ours, &theirs], "Merge", &[], &[&alice]);
        assert!(matches!(
            receive(&mut store, &policy, &selfie),
            Err(VcsError::Refused(_))
        ));

        // A reviewer may write but may not approve (docs/05 §3).
        let (mut store, alice, _root, ours, theirs, _h) = forked();
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        let reviewer = key(5);
        let policy = ProtectedMain {
            needed: 1,
            reviewers: vec![pk(&reviewer)],
            joined_late: vec![],
        };
        let env = build_on(&mut store, &alice, &[&ours, &theirs], "Merge", &[], &[&reviewer]);
        assert!(matches!(receive(&mut store, &policy, &env), Err(VcsError::Refused(_))));

        // A forged co-signature does not count either.
        let (mut store, alice, _root, ours, theirs, _h) = forked();
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        let policy = ProtectedMain::new(1);
        let mut forged = build_on(&mut store, &alice, &[&ours, &theirs], "Merge", &[], &[&priya]);
        forged.co_sigs[0] = cosign(&priya, &"ab".repeat(32)).unwrap();
        assert!(matches!(receive(&mut store, &policy, &forged), Err(VcsError::Refused(_))));
    }

    #[test]
    fn test43_a_fast_forward_into_a_protected_branch_still_writes_a_merge() {
        let (mut store, alice, root, h) = scene();
        branch_create(&mut store, "h2", &root).unwrap();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let ahead = commit_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)]);
        set_branch(&mut store, "h2", &ahead, false).unwrap();

        // Unprotected: the ref just moves.
        match crate::merge::merge(&store, &root, &ahead, false).unwrap() {
            crate::merge::MergeOutcome::FastForward { to } => assert_eq!(to, ahead),
            other => panic!("expected a fast-forward, got {other:?}"),
        }
        // Protected: a merge commit is written instead, so the co-signatures
        // have somewhere to live and a peer can check the threshold.
        match crate::merge::merge(&store, &root, &ahead, true).unwrap() {
            crate::merge::MergeOutcome::Merged {
                records,
                taken_theirs,
                ..
            } => {
                assert!(records.is_empty(), "nothing to rewrite, just a merge commit");
                assert_eq!(taken_theirs, 1);
            }
            other => panic!("a protected target never fast-forwards: {other:?}"),
        }

        let priya = key(3);
        let policy = ProtectedMain::new(1);
        set_branch(&mut store, MAIN, &root, true).unwrap();
        let env = build_on(&mut store, &alice, &[&root, &ahead], "Merge h2", &[], &[&priya]);
        assert!(matches!(
            receive(&mut store, &policy, &env).unwrap(),
            Received::Applied { .. }
        ));
        set_branch(&mut store, MAIN, &env.id, false).unwrap();
    }

    #[test]
    fn set_branch_refuses_a_non_fast_forward_unless_forced() {
        let (mut store, _alice, root, ours, theirs, _h) = forked();
        assert_eq!(resolve_head(&store).unwrap(), Some(root), "HEAD follows main");
        set_branch(&mut store, MAIN, &ours, true).unwrap();
        assert!(
            set_branch(&mut store, MAIN, &theirs, false).is_err(),
            "a sideways move is not a fast-forward"
        );
        set_branch(&mut store, MAIN, &theirs, true).unwrap();
        assert_eq!(resolve(&store, "main").unwrap(), Some(theirs.clone()));
        assert_eq!(resolve_head(&store).unwrap(), Some(theirs));
        assert_eq!(resolve(&store, "no-such-branch").unwrap(), None);
    }

    #[test]
    fn a_duplicate_commit_is_not_an_error() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let env = build_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)], &[]);
        receive(&mut store, &OpenPolicy, &env).unwrap();
        assert_eq!(
            receive(&mut store, &OpenPolicy, &env).unwrap(),
            Received::Duplicate
        );
    }

    #[test]
    fn a_non_member_author_is_refused() {
        struct NoOne;
        impl MemberPolicy for NoOne {
            fn may_write(&self, _a: &str, _at: Option<&str>) -> bool {
                false
            }
            fn may_approve(&self, _k: &str, _at: Option<&str>) -> bool {
                false
            }
            fn approvals_required(&self, _b: &str) -> usize {
                0
            }
        }
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let env = build_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)], &[]);
        assert!(matches!(receive(&mut store, &NoOne, &env), Err(VcsError::Refused(_))));
        assert!(!store.has_commit(&env.id));
    }

    #[test]
    fn a_tampered_tree_is_refused() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let mut env = build_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)], &[]);
        env.header.tree = "0".repeat(64);
        let preimage = hashing::preimage(hashing::COMMIT, &env.header).unwrap();
        env.id = hashing::hex32(&hashing::digest(&preimage));
        env.sig = hex::encode(
            ed25519_dalek::Signer::sign(&alice, &preimage).to_bytes(),
        );
        // Perfectly signed, perfectly self-consistent, and still refused:
        // step 7 recomputes the state root from the records themselves.
        assert!(matches!(
            receive(&mut store, &OpenPolicy, &env),
            Err(VcsError::TreeMismatch { .. })
        ));
    }
}
