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

use crate::record;
use crate::store::CommitStore;
use crate::types::{Conflict, ObjectId, Record, VcsError};
use crate::Result;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

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
    if ours_head == theirs_head {
        return Ok(MergeOutcome::UpToDate);
    }
    if crate::dag::is_ancestor(store, theirs_head, ours_head)? {
        return Ok(MergeOutcome::UpToDate);
    }
    if crate::dag::is_ancestor(store, ours_head, theirs_head)? && !target_protected {
        return Ok(MergeOutcome::FastForward {
            to: theirs_head.to_string(),
        });
    }

    let bases = crate::dag::merge_base(store, ours_head, theirs_head)?;
    let mut conflicts: Vec<Conflict> = Vec::new();

    if bases.len() > 1 {
        // Criss-cross. docs/10 §2.4: the bases are NOT merged into a virtual
        // base. Every object that differs across them is handed to a human.
        let states: Vec<BTreeMap<ObjectId, (u64, String)>> = bases
            .iter()
            .map(|b| state_at(store, b))
            .collect::<Result<Vec<_>>>()?;
        let mut objects: BTreeSet<ObjectId> = BTreeSet::new();
        for state in &states {
            objects.extend(state.keys().cloned());
        }
        for object in objects {
            let variants: BTreeSet<Option<&String>> = states
                .iter()
                .map(|s| s.get(&object).map(|(_, h)| h))
                .collect();
            if variants.len() > 1 {
                conflicts.push(Conflict {
                    object,
                    path: String::new(),
                    base: None,
                    ours: None,
                    theirs: None,
                    reason: format!(
                        "These two branches have {} equally close common ancestors, and this \
                         record is different in each of them. OLB will not invent a starting \
                         point nobody wrote, so this record has to be reconciled by hand.",
                        bases.len()
                    ),
                });
            }
        }
    }

    let base_state = match bases.first() {
        Some(b) => state_at(store, b)?,
        None => BTreeMap::new(),
    };
    let ours_state = state_at(store, ours_head)?;
    let theirs_state = state_at(store, theirs_head)?;

    let mut objects: BTreeSet<ObjectId> = BTreeSet::new();
    objects.extend(base_state.keys().cloned());
    objects.extend(ours_state.keys().cloned());
    objects.extend(theirs_state.keys().cloned());

    let mut records: Vec<Record> = Vec::new();
    let mut taken_ours = 0usize;
    let mut taken_theirs = 0usize;

    for object in objects {
        let b = base_state.get(&object).map(|(_, h)| h.as_str());
        let o = ours_state.get(&object).map(|(_, h)| h.as_str());
        let t = theirs_state.get(&object).map(|(_, h)| h.as_str());
        // Comparison is by record hash, so "unchanged" is exact and free.
        if o == t {
            continue;
        }
        if o == b {
            // Ours never moved: theirs wins outright, and it is already the
            // highest version in the union of the two parents, so the merge
            // commit needs no new record for it.
            taken_theirs += 1;
            continue;
        }
        if t == b {
            taken_ours += 1;
            continue;
        }
        let base_rec = b.map(|h| load(store, h)).transpose()?;
        let ours_rec = o.map(|h| load(store, h)).transpose()?;
        let theirs_rec = t.map(|h| load(store, h)).transpose()?;
        match merge_record(base_rec.as_ref(), ours_rec.as_ref(), theirs_rec.as_ref())? {
            Ok(merged) => records.push(merged),
            Err(found) => conflicts.extend(found),
        }
    }

    if !conflicts.is_empty() {
        return Ok(MergeOutcome::Conflicted(conflicts));
    }
    Ok(MergeOutcome::Merged {
        records,
        taken_ours,
        taken_theirs,
    })
}

fn state_at(store: &dyn CommitStore, commit: &str) -> Result<BTreeMap<ObjectId, (u64, String)>> {
    Ok(record::live_objects(store, commit)?
        .into_iter()
        .map(|(o, v, h)| (o, (v, h)))
        .collect())
}

fn load(store: &dyn CommitStore, hash: &str) -> Result<Record> {
    store
        .record(hash)
        .ok_or_else(|| VcsError::MissingRecord(hash.to_string()))
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
    merge_record_inner(base, ours, theirs, &BTreeMap::new())
}

fn merge_record_inner(
    base: Option<&Record>,
    ours: Option<&Record>,
    theirs: Option<&Record>,
    resolutions: &BTreeMap<String, Resolution>,
) -> Result<std::result::Result<Record, Vec<Conflict>>> {
    let (ours, theirs) = match (ours, theirs) {
        (Some(o), Some(t)) => (o, t),
        // There is no delete (docs/10 §3.6), so an object present on one side
        // and absent on the other was created there. Nothing to merge.
        (Some(o), None) => return Ok(Ok(o.clone())),
        (None, Some(t)) => return Ok(Ok(t.clone())),
        (None, None) => {
            return Err(VcsError::Malformed(
                "merge_record needs a record on at least one side".into(),
            ))
        }
    };
    if ours.id != theirs.id {
        return Err(VcsError::Malformed(format!(
            "cannot merge {} with {}",
            ours.id, theirs.id
        )));
    }
    if record::content_hash(ours)? == record::content_hash(theirs)? {
        return Ok(Ok(ours.clone()));
    }

    let base_doc = base.map(body).transpose()?;
    let ours_doc = body(ours)?;
    let theirs_doc = body(theirs)?;

    let ctx = Ctx {
        object: &ours.id,
        resolutions,
    };
    let mut conflicts = Vec::new();

    // Retraction against an edit, named explicitly (docs/10 §3.6 and §6.4
    // test 44). The table in §4.2 would resolve this one automatically —
    // `flags.retracted` is absent on the other side, so `t == b` takes ours
    // — and §6.4 requires a conflict instead. §6.4 wins: auto-resolving it
    // lands one person's edit onto a record the other person just withdrew,
    // and neither of them is told. The carve-out only ever ADDS a conflict,
    // so it cannot invent content; see the report note on this disagreement.
    fn flag(rec: Option<&Record>) -> Option<&Value> {
        rec.and_then(|r| r.flags.get("retracted"))
    }
    fn set(v: Option<&Value>) -> bool {
        v == Some(&Value::Bool(true))
    }
    let base_flag = flag(base);
    let ours_flag = flag(Some(ours));
    let theirs_flag = flag(Some(theirs));
    if (set(ours_flag) != set(base_flag)) != (set(theirs_flag) != set(base_flag)) {
        conflicts.push(Conflict {
            object: ours.id.clone(),
            path: "flags.retracted".to_string(),
            base: base_flag.cloned(),
            ours: ours_flag.cloned(),
            theirs: theirs_flag.cloned(),
            reason: "One side withdrew this record while the other was still editing it. \
                     A withdrawal is not a deletion, so both changes could stand — but \
                     nobody should find that out afterwards. Decide whether the \
                     withdrawal holds."
                .to_string(),
        });
    }
    let merged = merge_value(
        &ctx,
        "",
        base_doc.as_ref().map(|d| d as &Value),
        Some(&ours_doc),
        Some(&theirs_doc),
        &mut conflicts,
    );
    if !conflicts.is_empty() {
        return Ok(Err(conflicts));
    }
    let Some(Value::Object(mut doc)) = merged else {
        return Err(VcsError::Malformed(
            "a merged record must still be an object".into(),
        ));
    };

    // `merged_version` owns the version arithmetic and the `prev` set, so the
    // two cannot drift apart from the rest of the record.
    let scaffold = record::merged_version(&[ours, theirs], Map::new())?;
    doc.insert("version".into(), Value::from(scaffold.version));
    doc.insert(
        "prev".into(),
        serde_json::to_value(&scaffold.prev).map_err(|e| VcsError::Malformed(e.to_string()))?,
    );
    let merged: Record =
        serde_json::from_value(Value::Object(doc)).map_err(|e| VcsError::Malformed(e.to_string()))?;
    Ok(Ok(merged))
}

/// A record as a JSON object with `version` and `prev` removed: they are
/// derived, not merged.
fn body(rec: &Record) -> Result<Value> {
    let value = serde_json::to_value(rec).map_err(|e| VcsError::Malformed(e.to_string()))?;
    let Value::Object(mut map) = value else {
        return Err(VcsError::Malformed("a record serialises to an object".into()));
    };
    map.remove("version");
    map.remove("prev");
    Ok(Value::Object(map))
}

struct Ctx<'a> {
    object: &'a ObjectId,
    resolutions: &'a BTreeMap<String, Resolution>,
}

/// The whole merge policy, in one recursive function (docs/10 §4.2):
///
/// | Case | Result |
/// | --- | --- |
/// | `o == t` | take it, including both sides making the identical edit |
/// | `o == b` | take `t` |
/// | `t == b` | take `o` |
/// | all three differ | recurse if both sides are objects or mergeable arrays, otherwise CONFLICT |
fn merge_value(
    ctx: &Ctx<'_>,
    path: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
    out: &mut Vec<Conflict>,
) -> Option<Value> {
    // A human decision for this exact path beats everything below it,
    // including the automatic rules: `flags.retracted` is reported as a
    // conflict even though the table would resolve it, so honouring the
    // resolution has to come first or the auto rule would quietly overrule
    // the person who made the call.
    if let Some(resolution) = ctx.resolutions.get(path) {
        return match resolution.choose.as_str() {
            "ours" => ours.cloned(),
            "theirs" => theirs.cloned(),
            _ => resolution.value.clone(),
        };
    }
    if ours == theirs {
        return ours.cloned();
    }
    if ours == base {
        return theirs.cloned();
    }
    if theirs == base {
        return ours.cloned();
    }
    match (ours, theirs) {
        (Some(Value::Object(ours_map)), Some(Value::Object(theirs_map))) => {
            let base_map = base.and_then(|v| v.as_object());
            let mut keys: BTreeSet<&String> = BTreeSet::new();
            keys.extend(ours_map.keys());
            keys.extend(theirs_map.keys());
            if let Some(b) = base_map {
                keys.extend(b.keys());
            }
            let mut merged = Map::new();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if let Some(value) = merge_value(
                    ctx,
                    &child,
                    base_map.and_then(|m| m.get(key)),
                    ours_map.get(key),
                    theirs_map.get(key),
                    out,
                ) {
                    merged.insert(key.clone(), value);
                }
            }
            Some(Value::Object(merged))
        }
        (Some(Value::Array(ours_items)), Some(Value::Array(theirs_items))) => {
            match array_rule(path) {
                ArrayRule::SetByCid => {
                    merge_keyed(ctx, path, "cid", base, ours_items, theirs_items, out)
                }
                ArrayRule::KeyedBy(key) => {
                    merge_keyed(ctx, path, key, base, ours_items, theirs_items, out)
                }
                ArrayRule::Opaque => {
                    out.push(conflict(ctx.object, path, base, ours, theirs));
                    None
                }
            }
        }
        // Everything else — text, numbers, booleans, a type change, a key
        // removed on one side and edited on the other — is a conflict. This
        // is where the refusal to merge prose lives.
        _ => {
            out.push(conflict(ctx.object, path, base, ours, theirs));
            None
        }
    }
}

/// Element-wise merge of an array whose elements have a natural key. Order is
/// canonical (sorted by that key) so ordering is never itself a conflict.
fn merge_keyed(
    ctx: &Ctx<'_>,
    path: &str,
    key: &str,
    base: Option<&Value>,
    ours_items: &[Value],
    theirs_items: &[Value],
    out: &mut Vec<Conflict>,
) -> Option<Value> {
    let index = |items: &[Value]| -> Option<BTreeMap<String, Value>> {
        let mut map = BTreeMap::new();
        for item in items {
            let k = item.get(key)?.as_str()?.to_string();
            if map.insert(k, item.clone()).is_some() {
                // Two elements with one key: the array is not a set after
                // all, so there is no safe element-wise merge.
                return None;
            }
        }
        Some(map)
    };
    let (Some(ours_map), Some(theirs_map)) = (index(ours_items), index(theirs_items)) else {
        out.push(conflict(
            ctx.object,
            path,
            base,
            Some(&Value::Array(ours_items.to_vec())),
            Some(&Value::Array(theirs_items.to_vec())),
        ));
        return None;
    };
    let base_map = match base {
        None => BTreeMap::new(),
        Some(Value::Array(items)) => match index(items) {
            Some(m) => m,
            None => {
                out.push(conflict(
                    ctx.object,
                    path,
                    base,
                    Some(&Value::Array(ours_items.to_vec())),
                    Some(&Value::Array(theirs_items.to_vec())),
                ));
                return None;
            }
        },
        Some(other) => {
            out.push(conflict(
                ctx.object,
                path,
                Some(other),
                Some(&Value::Array(ours_items.to_vec())),
                Some(&Value::Array(theirs_items.to_vec())),
            ));
            return None;
        }
    };

    let mut keys: BTreeSet<&String> = BTreeSet::new();
    keys.extend(ours_map.keys());
    keys.extend(theirs_map.keys());
    keys.extend(base_map.keys());

    let mut merged: Vec<Value> = Vec::new();
    for k in keys {
        let element_path = format!("{path}[{k}]");
        let b = base_map.get(k);
        let o = ours_map.get(k);
        let t = theirs_map.get(k);
        if o == t {
            if let Some(v) = o {
                merged.push(v.clone());
            }
            continue;
        }
        if o == b {
            if let Some(v) = t {
                merged.push(v.clone());
            }
            continue;
        }
        if t == b {
            if let Some(v) = o {
                merged.push(v.clone());
            }
            continue;
        }
        match (o, t) {
            (Some(_), Some(_)) => {
                if let Some(v) = merge_value(ctx, &element_path, b, o, t, out) {
                    merged.push(v);
                }
            }
            // Present on one side, gone on the other, and changed where it is
            // present: one person removed it while the other was editing it.
            _ => out.push(conflict(ctx.object, &element_path, b, o, t)),
        }
    }
    Some(Value::Array(merged))
}

/// Text fields are named here only to explain the refusal in words a
/// researcher can act on. The refusal itself applies to every scalar.
fn is_prose(path: &str) -> bool {
    matches!(
        path.rsplit('.').next().unwrap_or(path),
        "observations" | "summary" | "notes" | "statement" | "description" | "conclusion"
    )
}

fn conflict(
    object: &ObjectId,
    path: &str,
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
) -> Conflict {
    let reason = match (ours, theirs) {
        (Some(_), None) => "You edited this and the other side removed it. Removing and editing \
                            cannot both be true, so choose which one stands."
            .to_string(),
        (None, Some(_)) => "The other side edited this and you removed it. Removing and editing \
                            cannot both be true, so choose which one stands."
            .to_string(),
        _ if is_prose(path) => "Both sides rewrote this text. OLB never stitches two accounts \
                                together, because the result could describe an observation \
                                neither of you made. Keep one version or write a new one."
            .to_string(),
        _ if matches!(ours, Some(Value::Array(_))) => "Both sides changed this list in different \
                                                       ways, and its entries have no identity to \
                                                       match up. Choose one list or write a new \
                                                       one."
            .to_string(),
        _ => "Both sides changed this to a different value. Choose one or enter a new value."
            .to_string(),
    };
    Conflict {
        object: object.clone(),
        path: path.to_string(),
        base: base.cloned(),
        ours: ours.cloned(),
        theirs: theirs.cloned(),
        reason,
    }
}

/// Apply a complete set of resolutions and produce the merged record
/// versions.
///
/// Rejects a resolution set that does not cover every reported conflict — a
/// partial resolution silently dropping a conflict is how a scientist loses
/// an observation without being told.
///
/// This is the base-free form: with no base, the only differences it can
/// settle are the ones a human settled, so it merges exactly the objects named
/// in `conflicts` and refuses if anything else in them still differs. The real
/// merge flow has a base and should call `apply_resolution_with_base`, which
/// reproduces the auto-merge as well. Keeping both means the fixed signature
/// stays honest instead of guessing which side an unreported difference came
/// from — and guessing is how an edit disappears silently.
pub fn apply_resolution(
    conflicts: &[Conflict],
    resolutions: &[Resolution],
    ours: &[Record],
    theirs: &[Record],
) -> Result<Vec<Record>> {
    apply_resolution_with_base(&[], conflicts, resolutions, ours, theirs)
}

/// `apply_resolution` with the merge base, which is what the real flow has.
///
/// ADDED, not a change to the contract: `apply_resolution`'s signature carries
/// no base, and without one a difference between `ours` and `theirs` that was
/// not reported as a conflict is unattributable — it could be "ours never
/// moved, take theirs" or "theirs never moved, take ours", and the two give
/// different records. Every object in the union is merged here, so the result
/// is the complete set of record versions the merge commit must write.
pub fn apply_resolution_with_base(
    base: &[Record],
    conflicts: &[Conflict],
    resolutions: &[Resolution],
    ours: &[Record],
    theirs: &[Record],
) -> Result<Vec<Record>> {
    // Every conflict resolved exactly once, and nothing resolved that was
    // never reported.
    for resolution in resolutions {
        match resolution.choose.as_str() {
            "ours" | "theirs" => {}
            "custom" => {
                if resolution.value.is_none() {
                    return Err(VcsError::Malformed(format!(
                        "a custom resolution for {} needs a value",
                        resolution.path
                    )));
                }
            }
            other => {
                return Err(VcsError::Malformed(format!(
                    "unknown resolution {other} for {}",
                    resolution.path
                )))
            }
        }
        if !conflicts
            .iter()
            .any(|c| c.object == resolution.object && c.path == resolution.path)
        {
            return Err(VcsError::Refused(format!(
                "{} at {} was not reported as a conflict",
                resolution.object, resolution.path
            )));
        }
    }
    for conflict in conflicts {
        let matches = resolutions
            .iter()
            .filter(|r| r.object == conflict.object && r.path == conflict.path)
            .count();
        if matches == 0 {
            return Err(VcsError::Refused(format!(
                "{} at {} is unresolved; a partial resolution is never applied",
                conflict.object, conflict.path
            )));
        }
        if matches > 1 {
            return Err(VcsError::Refused(format!(
                "{} at {} was resolved {matches} times",
                conflict.object, conflict.path
            )));
        }
    }

    let by_object = |records: &[Record], object: &ObjectId| -> Option<Record> {
        records.iter().find(|r| &r.id == object).cloned()
    };

    let mut objects: BTreeSet<ObjectId> = BTreeSet::new();
    if base.is_empty() {
        objects.extend(conflicts.iter().map(|c| c.object.clone()));
    } else {
        objects.extend(ours.iter().map(|r| r.id.clone()));
        objects.extend(theirs.iter().map(|r| r.id.clone()));
    }

    let mut out = Vec::new();
    for object in objects {
        let per_object: BTreeMap<String, Resolution> = resolutions
            .iter()
            .filter(|r| r.object == object)
            .map(|r| (r.path.clone(), r.clone()))
            .collect();
        let b = by_object(base, &object);
        let o = by_object(ours, &object);
        let t = by_object(theirs, &object);
        if o.is_none() && t.is_none() {
            return Err(VcsError::MissingRecord(format!(
                "{object} has no version on either side"
            )));
        }
        // Nothing to write when both sides already agree.
        if let (Some(o), Some(t)) = (&o, &t) {
            if record::content_hash(o)? == record::content_hash(t)? {
                continue;
            }
        }
        match merge_record_inner(b.as_ref(), o.as_ref(), t.as_ref(), &per_object)? {
            Ok(merged) => out.push(merged),
            Err(remaining) => return Err(VcsError::Conflicts(remaining)),
        }
    }
    Ok(out)
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

/// `b"olb.mr.approve.v1\0" || mr_id || source_head_bytes || target_head_bytes`.
///
/// The heads are the raw 32 bytes, at the end and fixed width, so a variable
/// length `mr_id` in front can never shift one field into another.
fn approval_preimage(mr_id: &str, source_head: &str, target_head: &str) -> Result<Vec<u8>> {
    let mut buf = MR_APPROVE_DOMAIN.to_vec();
    buf.extend_from_slice(mr_id.as_bytes());
    buf.extend_from_slice(&crate::hashing::unhex32(source_head)?);
    buf.extend_from_slice(&crate::hashing::unhex32(target_head)?);
    Ok(buf)
}

/// Sign an approval over `mr_id || source_head || target_head`.
pub fn sign_approval(
    signing: &ed25519_dalek::SigningKey,
    mr_id: &str,
    source_head: &str,
    target_head: &str,
) -> Result<Approval> {
    use ed25519_dalek::Signer;
    let buf = approval_preimage(mr_id, source_head, target_head)?;
    Ok(Approval {
        key: crate::hashing::hex32(&signing.verifying_key().to_bytes()),
        source_head: source_head.to_string(),
        target_head: target_head.to_string(),
        sig: hex::encode(signing.sign(&buf).to_bytes()),
    })
}

/// Verify one approval against the MR it claims to be for.
pub fn verify_approval(mr: &MergeRequest, approval: &Approval) -> Result<()> {
    use ed25519_dalek::Verifier;
    let buf = approval_preimage(mr.id.as_str(), &approval.source_head, &approval.target_head)?;
    let key_bytes = crate::hashing::unhex32(&approval.key)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| VcsError::BadSignature(format!("bad approver key: {e}")))?;
    let raw = hex::decode(&approval.sig)
        .map_err(|e| VcsError::Malformed(format!("bad approval signature hex: {e}")))?;
    let bytes: [u8; 64] = raw
        .try_into()
        .map_err(|_| VcsError::Malformed("a signature is 64 bytes".to_string()))?;
    key.verify(&buf, &ed25519_dalek::Signature::from_bytes(&bytes))
        .map_err(|e| VcsError::BadSignature(format!("approval by {}: {e}", approval.key)))
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
    let mut counted: BTreeSet<&str> = BTreeSet::new();
    for approval in &mr.approvals {
        // Stale the moment either head moves. Without this, "get approved,
        // then push" defeats review entirely.
        if approval.source_head != mr.source_head || approval.target_head != mr.target_head {
            continue;
        }
        if approval.key == mr.author {
            continue;
        }
        if verify_approval(mr, approval).is_err() {
            continue;
        }
        if !policy.may_approve(&approval.key, Some(mr.base.as_str())) {
            continue;
        }
        counted.insert(approval.key.as_str());
    }
    Ok(counted.len())
}

#[cfg(test)]
mod tests {
    //! docs/10 §6.4. Items 41 and 43 (the peer-side drop and the protected
    //! fast-forward) live in `dag::tests`, where `receive` is.

    use super::*;
    use crate::dag::tests::{build_on, commit_on, create, edit, key, pk, scene, set_field, forked, ProtectedMain};
    use crate::dag::{self, MemberPolicy, OpenPolicy, Received};
    use crate::store::{CommitStore, MemStore};
    use crate::types::ChangeKind;
    use serde_json::json;

    /// A store where `ours` and `theirs` have each changed the same object in
    /// their own way, starting from one base version.
    fn diverged(
        ours_change: impl FnOnce(&mut Record),
        theirs_change: impl FnOnce(&mut Record),
    ) -> (MemStore, String, String, Record, Record, Record) {
        let (mut store, alice, root, h) = scene();
        let base = edit(&h, |r| {
            set_field(r, "statement", json!("Kinase X gates entry"));
            set_field(r, "notes", json!("baseline"));
        });
        let base_commit = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Baseline",
            &[(base.clone(), ChangeKind::Edit)],
        );
        let ours = edit(&base, ours_change);
        let theirs = edit(&base, theirs_change);
        let ours_head = commit_on(
            &mut store,
            &alice,
            &[&base_commit],
            "Ours",
            &[(ours.clone(), ChangeKind::Edit)],
        );
        let theirs_head = commit_on(
            &mut store,
            &alice,
            &[&base_commit],
            "Theirs",
            &[(theirs.clone(), ChangeKind::Edit)],
        );
        (store, ours_head, theirs_head, base, ours, theirs)
    }

    #[test]
    fn test28_disjoint_objects_merge_to_the_union() {
        let (mut store, alice, root, _h) = scene();
        let run = create("experiment", json!({"title": "Run 14"}));
        let plate = create("dataset", json!({"title": "Plate 3"}));
        let ours = commit_on(&mut store, &alice, &[&root], "ours", &[(run.clone(), ChangeKind::Create)]);
        let theirs = commit_on(
            &mut store,
            &alice,
            &[&root],
            "theirs",
            &[(plate.clone(), ChangeKind::Create)],
        );

        match merge(&store, &ours, &theirs, false).unwrap() {
            MergeOutcome::Merged {
                records,
                taken_ours,
                taken_theirs,
            } => {
                assert!(records.is_empty(), "nothing had to be rewritten");
                assert_eq!(taken_ours, 1);
                assert_eq!(taken_theirs, 1);
            }
            other => panic!("disjoint objects must merge cleanly: {other:?}"),
        }

        let merge_commit = commit_on(&mut store, &alice, &[&ours, &theirs], "Merge", &[]);
        let live = record::live_objects(&store, &merge_commit).unwrap();
        let ids: Vec<&str> = live.iter().map(|(o, _, _)| o.0.as_str()).collect();
        assert_eq!(live.len(), 3, "the hypothesis plus both new objects: {ids:?}");
        assert!(ids.contains(&run.id.0.as_str()));
        assert!(ids.contains(&plate.id.0.as_str()));
    }

    #[test]
    fn test29_disjoint_fields_of_one_object_merge() {
        let (store, ours_head, theirs_head, base, ours, theirs) = diverged(
            |r| set_field(r, "statement", json!("Kinase X is required for entry")),
            |r| set_field(r, "notes", json!("Plate 3 ran at 4 degrees")),
        );
        let merged = match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Merged { records, .. } => records,
            other => panic!("{other:?}"),
        };
        assert_eq!(merged.len(), 1);
        let m = &merged[0];
        assert_eq!(
            m.fields.get("statement"),
            Some(&json!("Kinase X is required for entry")),
            "ours is kept"
        );
        assert_eq!(
            m.fields.get("notes"),
            Some(&json!("Plate 3 ran at 4 degrees")),
            "and so is theirs"
        );
        assert_eq!(m.version, 4, "max(ours, theirs) + 1, and both sides were at 3");
        let mut expected = vec![
            record::content_hash(&ours).unwrap(),
            record::content_hash(&theirs).unwrap(),
        ];
        expected.sort();
        assert_eq!(m.prev, expected, "both predecessors are in prev");
        assert_eq!(m.id, base.id);
    }

    #[test]
    fn test30_the_identical_edit_on_both_sides_is_not_a_conflict() {
        // Two separate scenes: one where the sides touch different fields,
        // one where they make the identical edit to the same field.
        // Both sides make the identical edit to the same field, through two
        // different commits.
        let (store, ours_head, theirs_head, _b, _o, _t) = diverged(
            |r| set_field(r, "statement", json!("agreed wording")),
            |r| set_field(r, "statement", json!("agreed wording")),
        );
        match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Merged { records, .. } => assert!(
                records.is_empty(),
                "the two sides already hold the identical record"
            ),
            other => panic!("an identical edit is never a conflict: {other:?}"),
        }
    }

    #[test]
    fn test31_the_same_field_changed_differently_is_exactly_one_conflict() {
        let (store, ours_head, theirs_head, base, ours, theirs) = diverged(
            |r| set_field(r, "statement", json!("Kinase X is required")),
            |r| set_field(r, "statement", json!("Kinase X is sufficient")),
        );
        let conflicts = match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("a contested field must never auto-resolve: {other:?}"),
        };
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        let c = &conflicts[0];
        assert_eq!(c.path, "fields.statement");
        assert_eq!(c.object, base.id);
        assert_eq!(c.base, Some(json!("Kinase X gates entry")));
        assert_eq!(c.ours, Some(json!("Kinase X is required")));
        assert_eq!(c.theirs, Some(json!("Kinase X is sufficient")));
        assert!(!c.reason.is_empty());

        // Nothing is applied until it is resolved, and the two sides still
        // hold their own versions untouched.
        assert_eq!(ours.version, 3);
        assert_eq!(theirs.version, 3);
        assert_ne!(
            record::content_hash(&ours).unwrap(),
            record::content_hash(&theirs).unwrap()
        );
        assert!(store.record(&record::content_hash(&ours).unwrap()).is_some());
        assert!(store.record(&record::content_hash(&theirs).unwrap()).is_some());
    }

    #[test]
    fn test32_a_side_untouched_since_the_base_takes_the_other() {
        let (mut store, alice, root, h) = scene();
        let base = edit(&h, |r| set_field(r, "statement", json!("baseline")));
        let base_commit = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Baseline",
            &[(base.clone(), ChangeKind::Edit)],
        );
        let theirs = edit(&base, |r| set_field(r, "statement", json!("theirs moved on")));
        let theirs_head = commit_on(
            &mut store,
            &alice,
            &[&base_commit],
            "Theirs",
            &[(theirs.clone(), ChangeKind::Edit)],
        );
        // Ours adds an unrelated object and never touches the hypothesis.
        let run = create("experiment", json!({"title": "Run 14"}));
        let ours_head = commit_on(
            &mut store,
            &alice,
            &[&base_commit],
            "Ours",
            &[(run, ChangeKind::Create)],
        );

        match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Merged {
                records,
                taken_ours,
                taken_theirs,
            } => {
                assert!(records.is_empty());
                assert_eq!(taken_theirs, 1, "the hypothesis comes from theirs");
                assert_eq!(taken_ours, 1, "the experiment comes from ours");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test33_identical_content_through_different_commits_has_no_conflicts() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("agreed")));
        // The same version 2 record, committed independently on two branches.
        let ours = commit_on(&mut store, &alice, &[&root], "ours", &[(v2.clone(), ChangeKind::Edit)]);
        let theirs = commit_on(
            &mut store,
            &alice,
            &[&root],
            "theirs",
            &[(v2.clone(), ChangeKind::Edit)],
        );
        assert_ne!(ours, theirs, "two different commits");
        match merge(&store, &ours, &theirs, false).unwrap() {
            MergeOutcome::Merged { records, .. } => assert!(
                records.is_empty(),
                "content-addressed records make this free"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn test34_attachments_merge_as_a_set_keyed_by_cid() {
        let att = |cid: &str, caption: &str| json!({"cid": cid, "caption": caption});
        let (store, ours_head, theirs_head, _base, _o, _t) = {
            let (mut store, alice, root, h) = scene();
            let mut base = edit(&h, |_| {});
            base.attachments = vec![att("logos:a", "plate 1")];
            let base_commit = commit_on(
                &mut store,
                &alice,
                &[&root],
                "Baseline",
                &[(base.clone(), ChangeKind::Edit)],
            );
            let mut ours = edit(&base, |_| {});
            ours.attachments = vec![att("logos:a", "plate 1"), att("logos:b", "plate 2")];
            let mut theirs = edit(&base, |_| {});
            theirs.attachments = vec![att("logos:a", "plate 1"), att("logos:c", "plate 3")];
            let o = commit_on(&mut store, &alice, &[&base_commit], "ours", &[(ours.clone(), ChangeKind::Edit)]);
            let t = commit_on(&mut store, &alice, &[&base_commit], "theirs", &[(theirs.clone(), ChangeKind::Edit)]);
            (store, o, t, base, ours, theirs)
        };
        let merged = match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Merged { records, .. } => records,
            other => panic!("adds on both sides must union: {other:?}"),
        };
        assert_eq!(merged.len(), 1);
        let cids: Vec<&str> = merged[0]
            .attachments
            .iter()
            .map(|a| a["cid"].as_str().unwrap())
            .collect();
        assert_eq!(
            cids,
            vec!["logos:a", "logos:b", "logos:c"],
            "canonical order, so order is never itself a conflict"
        );

        // Remove on ours, edit the metadata on theirs: a conflict.
        let (mut store, alice, root, h) = scene();
        let mut base = edit(&h, |_| {});
        base.attachments = vec![att("logos:a", "plate 1")];
        let base_commit = commit_on(&mut store, &alice, &[&root], "Baseline", &[(base.clone(), ChangeKind::Edit)]);
        let mut ours = edit(&base, |_| {});
        ours.attachments = vec![];
        let mut theirs = edit(&base, |_| {});
        theirs.attachments = vec![att("logos:a", "plate 1, rerun")];
        let o = commit_on(&mut store, &alice, &[&base_commit], "ours", &[(ours, ChangeKind::Edit)]);
        let t = commit_on(&mut store, &alice, &[&base_commit], "theirs", &[(theirs, ChangeKind::Edit)]);
        let conflicts = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("remove plus edit is a conflict: {other:?}"),
        };
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "attachments[logos:a]");
        assert_eq!(conflicts[0].ours, None, "gone on our side");
        assert!(conflicts[0].theirs.is_some());
    }

    fn pred(label: &str, text: &str) -> serde_json::Value {
        json!({"label": label, "text": text})
    }

    /// A three-way scene where the BASE already carries `predictions`, so the
    /// keyed-array rule is exercised against a real base rather than against
    /// an absent field.
    fn with_predictions(
        ours_preds: serde_json::Value,
        theirs_preds: serde_json::Value,
    ) -> (MemStore, String, String) {
        let (mut store, alice, root, h) = scene();
        let base = edit(&h, |r| {
            set_field(r, "predictions", json!([pred("P1", "one"), pred("P2", "two")]));
        });
        let base_commit = commit_on(
            &mut store,
            &alice,
            &[&root],
            "Baseline predictions",
            &[(base.clone(), ChangeKind::Edit)],
        );
        let ours = edit(&base, |r| set_field(r, "predictions", ours_preds));
        let theirs = edit(&base, |r| set_field(r, "predictions", theirs_preds));
        let o = commit_on(&mut store, &alice, &[&base_commit], "ours", &[(ours, ChangeKind::Edit)]);
        let t = commit_on(&mut store, &alice, &[&base_commit], "theirs", &[(theirs, ChangeKind::Edit)]);
        (store, o, t)
    }

    #[test]
    fn test35_keyed_arrays_merge_element_wise() {
        // Ours adds P3, theirs adds P4, both keeping P1 and P2. Note the
        // input order: theirs lists P4 first, and the merged order is still
        // canonical, so ordering is never itself a conflict.
        let (store, o, t) = with_predictions(
            json!([pred("P1", "one"), pred("P2", "two"), pred("P3", "three")]),
            json!([pred("P4", "four"), pred("P1", "one"), pred("P2", "two")]),
        );
        let merged = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Merged { records, .. } => records,
            other => panic!("adding different elements must union: {other:?}"),
        };
        let labels: Vec<&str> = merged[0].fields["predictions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["label"].as_str().unwrap())
            .collect();
        assert_eq!(labels, vec!["P1", "P2", "P3", "P4"], "sorted by label");

        // Both sides add P3, with different text: one conflict, scoped to P3.
        let (store, o, t) = with_predictions(
            json!([pred("P1", "one"), pred("P2", "two"), pred("P3", "melting at 42")]),
            json!([pred("P1", "one"), pred("P2", "two"), pred("P3", "melting at 51")]),
        );
        let conflicts = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("{other:?}"),
        };
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        assert_eq!(
            conflicts[0].path, "fields.predictions[P3].text",
            "the conflict names the element, not the whole list"
        );
        assert_eq!(conflicts[0].base, None, "P3 did not exist at the base");

        // One side edits P1 while the other edits P2: both edits survive.
        let (store, o, t) = with_predictions(
            json!([pred("P1", "one, rerun"), pred("P2", "two")]),
            json!([pred("P1", "one"), pred("P2", "two, rerun")]),
        );
        let merged = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Merged { records, .. } => records,
            other => panic!("element-wise edits must merge: {other:?}"),
        };
        assert_eq!(
            merged[0].fields["predictions"],
            json!([pred("P1", "one, rerun"), pred("P2", "two, rerun")])
        );
    }

    #[test]
    fn test36_text_is_never_merged_even_when_the_edits_do_not_overlap() {
        // Deliberately non-overlapping: ours appends a line at the end,
        // theirs prepends one at the start. A line-based merge would stitch
        // them. OLB refuses, because the stitched result could describe an
        // observation neither scientist made.
        let (store, o, t, _b, _ours, _theirs) = diverged(
            |r| set_field(r, "observations", json!("baseline\nours: plate 3 melted at 42")),
            |r| set_field(r, "observations", json!("theirs: buffer was stale\nbaseline")),
        );
        let conflicts = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("prose must never auto-merge: {other:?}"),
        };
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "fields.observations");
        assert!(
            conflicts[0].reason.contains("neither of you"),
            "the refusal is explained in words a researcher can act on: {}",
            conflicts[0].reason
        );
    }

    #[test]
    fn test37_a_partial_resolution_is_rejected_and_a_full_one_merges() {
        let (mut store, ours_head, theirs_head, base, ours, theirs) = diverged(
            |r| set_field(r, "statement", json!("Kinase X is required")),
            |r| set_field(r, "statement", json!("Kinase X is sufficient")),
        );
        let conflicts = match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("{other:?}"),
        };

        // Nothing resolved at all.
        assert!(matches!(
            apply_resolution(&conflicts, &[], std::slice::from_ref(&ours), std::slice::from_ref(&theirs)),
            Err(VcsError::Refused(_))
        ));
        // Resolved at the wrong path.
        let wrong = Resolution {
            object: base.id.clone(),
            path: "fields.notes".into(),
            choose: "ours".into(),
            value: None,
        };
        assert!(matches!(
            apply_resolution(&conflicts, &[wrong], std::slice::from_ref(&ours), std::slice::from_ref(&theirs)),
            Err(VcsError::Refused(_))
        ));
        // A custom resolution with no value.
        let empty = Resolution {
            object: base.id.clone(),
            path: "fields.statement".into(),
            choose: "custom".into(),
            value: None,
        };
        assert!(matches!(
            apply_resolution(&conflicts, &[empty], std::slice::from_ref(&ours), std::slice::from_ref(&theirs)),
            Err(VcsError::Malformed(_))
        ));

        // The full resolution.
        let resolution = Resolution {
            object: base.id.clone(),
            path: "fields.statement".into(),
            choose: "custom".into(),
            value: Some(json!("Kinase X is required and sufficient")),
        };
        let records = apply_resolution(
            &conflicts,
            std::slice::from_ref(&resolution),
            std::slice::from_ref(&ours),
            std::slice::from_ref(&theirs),
        )
        .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].fields["statement"],
            json!("Kinase X is required and sufficient")
        );
        assert_eq!(records[0].version, 4, "both sides were at version 3");

        // Exactly one merge commit, with the target first.
        let alice = key(1);
        let writes: Vec<(Record, ChangeKind)> = records
            .iter()
            .map(|r| (r.clone(), ChangeKind::Merge))
            .collect();
        let env = build_on(
            &mut store,
            &alice,
            &[&ours_head, &theirs_head],
            "Merge the hypothesis",
            &writes,
            &[],
        );
        assert!(matches!(
            dag::receive(&mut store, &OpenPolicy, &env).unwrap(),
            Received::Applied { .. }
        ));
        assert_eq!(env.header.parents, vec![ours_head, theirs_head.clone()]);
        assert_eq!(env.header.changes.len(), 1);

        // And the merge is now clean.
        assert_eq!(
            merge(&store, &env.id, &theirs_head, false).unwrap(),
            MergeOutcome::UpToDate
        );
    }

    #[test]
    fn test37_the_base_aware_form_keeps_the_auto_merged_fields() {
        // What `apply_resolution` cannot do without a base, and why the
        // base-aware form exists: theirs also changed an uncontested field,
        // and that change must survive the resolution.
        let (store, ours_head, theirs_head, base, ours, theirs) = diverged(
            |r| set_field(r, "statement", json!("ours")),
            |r| {
                set_field(r, "statement", json!("theirs"));
                set_field(r, "notes", json!("theirs also fixed the notes"));
            },
        );
        let conflicts = match merge(&store, &ours_head, &theirs_head, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("{other:?}"),
        };
        assert_eq!(conflicts.len(), 1, "only the statement is contested");
        let resolution = Resolution {
            object: base.id.clone(),
            path: "fields.statement".into(),
            choose: "ours".into(),
            value: None,
        };
        let records = apply_resolution_with_base(
            std::slice::from_ref(&base),
            &conflicts,
            std::slice::from_ref(&resolution),
            std::slice::from_ref(&ours),
            std::slice::from_ref(&theirs),
        )
        .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].fields["statement"], json!("ours"));
        assert_eq!(
            records[0].fields["notes"],
            json!("theirs also fixed the notes"),
            "the uncontested change from theirs is not lost"
        );
    }

    #[test]
    fn test44_a_retraction_against_an_edit_is_a_conflict_on_the_flag() {
        let (mut store, alice, root, h) = scene();
        let base = edit(&h, |r| set_field(r, "statement", json!("baseline")));
        let base_commit = commit_on(&mut store, &alice, &[&root], "Baseline", &[(base.clone(), ChangeKind::Edit)]);
        let ours = record::retract(&base).unwrap();
        let theirs = edit(&base, |r| set_field(r, "statement", json!("sharpened")));
        let o = commit_on(&mut store, &alice, &[&base_commit], "retract", &[(ours, ChangeKind::Flag)]);
        let t = commit_on(&mut store, &alice, &[&base_commit], "edit", &[(theirs, ChangeKind::Edit)]);

        let conflicts = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("a retraction against an edit needs a human: {other:?}"),
        };
        // There is no delete, so this reduces to an ordinary field conflict.
        assert!(
            conflicts.iter().any(|c| c.path == "flags.retracted"),
            "{conflicts:?}"
        );
        let flag = conflicts.iter().find(|c| c.path == "flags.retracted").unwrap();
        assert_eq!(flag.base, None, "the flag was absent at the base");
        assert_eq!(flag.ours, Some(json!(true)));
        assert_eq!(flag.theirs, None);
    }

    // ---- approvals (docs/10 §4.4, §4.5) ----

    fn mr(
        author: &str,
        base: &str,
        source_head: &str,
        target_head: &str,
        needed: usize,
        approvals: Vec<Approval>,
    ) -> MergeRequest {
        MergeRequest {
            id: "mr-h2".into(),
            project: crate::dag::tests::PROJECT.into(),
            title: "Add H2 draft and docking protocol".into(),
            source: "h2-computational".into(),
            target: "main".into(),
            author: author.into(),
            base: base.into(),
            source_head: source_head.into(),
            target_head: target_head.into(),
            reviewers: vec![],
            approvals,
            approvals_needed: needed,
            summary: json!({"added": 1, "edited": 0, "removed": 0, "conflicts": 0}),
            state: "open".into(),
        }
    }

    #[test]
    fn test38_the_approval_threshold_counts_only_eligible_distinct_keys() {
        let (_store, alice, root, ours, theirs, _h) = forked();
        let priya = key(3);
        let sam = key(4);
        let reviewer = key(5);
        let author = pk(&alice);
        let sign = |k: &ed25519_dalek::SigningKey| {
            sign_approval(k, "mr-h2", &theirs, &ours).unwrap()
        };
        let open = ProtectedMain::new(2);

        // One valid approval is not two.
        let one = mr(&author, &root, &theirs, &ours, 2, vec![sign(&priya)]);
        assert_eq!(count_fresh_approvals(&one, &open).unwrap(), 1);
        assert!(count_fresh_approvals(&one, &open).unwrap() < one.approvals_needed);

        // Two valid approvals from distinct eligible keys.
        let two = mr(&author, &root, &theirs, &ours, 2, vec![sign(&priya), sign(&sam)]);
        assert_eq!(count_fresh_approvals(&two, &open).unwrap(), 2);

        // The merge author's own approval does not count.
        let selfie = mr(&author, &root, &theirs, &ours, 2, vec![sign(&priya), sign(&alice)]);
        assert_eq!(count_fresh_approvals(&selfie, &open).unwrap(), 1);

        // A reviewer may not approve a merge (docs/05 §3 Protection).
        let with_reviewer = mr(
            &author,
            &root,
            &theirs,
            &ours,
            2,
            vec![sign(&priya), sign(&reviewer)],
        );
        let policy = ProtectedMain {
            needed: 2,
            reviewers: vec![pk(&reviewer)],
            joined_late: vec![],
        };
        assert_eq!(count_fresh_approvals(&with_reviewer, &policy).unwrap(), 1);

        // The same key twice counts once.
        let doubled = mr(&author, &root, &theirs, &ours, 2, vec![sign(&priya), sign(&priya)]);
        assert_eq!(count_fresh_approvals(&doubled, &open).unwrap(), 1);

        // A forged signature counts for nothing.
        let mut forged = sign(&sam);
        forged.sig = sign(&priya).sig;
        let tampered = mr(&author, &root, &theirs, &ours, 2, vec![sign(&priya), forged]);
        assert_eq!(count_fresh_approvals(&tampered, &open).unwrap(), 1);
    }

    #[test]
    fn test39_an_approval_goes_stale_the_moment_a_head_moves() {
        let (mut store, alice, root, ours, theirs, h) = forked();
        let priya = key(3);
        let approval = sign_approval(&priya, "mr-h2", &theirs, &ours).unwrap();
        let open = ProtectedMain::new(1);
        let before = mr(&pk(&alice), &root, &theirs, &ours, 1, vec![approval.clone()]);
        assert_eq!(count_fresh_approvals(&before, &open).unwrap(), 1);

        // Priya approves, then someone pushes to the source branch.
        let v2 = edit(&h, |r| set_field(r, "statement", json!("something else entirely")));
        let moved = commit_on(&mut store, &alice, &[&theirs], "push after approval", &[(v2, ChangeKind::Edit)]);
        let after = mr(&pk(&alice), &root, &moved, &ours, 1, vec![approval.clone()]);
        assert_eq!(
            count_fresh_approvals(&after, &open).unwrap(),
            0,
            "approve, then push must not defeat review"
        );

        // The same holds when the target moves.
        let target_moved = mr(&pk(&alice), &root, &theirs, &moved, 1, vec![approval]);
        assert_eq!(count_fresh_approvals(&target_moved, &open).unwrap(), 0);
    }

    #[test]
    fn test40_an_approval_is_bound_to_one_mr_and_one_pair_of_heads() {
        let (_store, alice, root, ours, theirs, _h) = forked();
        let priya = key(3);
        let approval = sign_approval(&priya, "mr-h2", &theirs, &ours).unwrap();

        let a = mr(&pk(&alice), &root, &theirs, &ours, 1, vec![approval.clone()]);
        verify_approval(&a, &approval).expect("verifies for the MR it was made for");

        let mut b = a.clone();
        b.id = "mr-b".into();
        assert!(
            verify_approval(&b, &approval).is_err(),
            "an approval for mr-h2 must not verify for mr-b"
        );

        // Replaying it onto a different pair of heads fails too: the heads
        // are inside the signed preimage, not just compared against the MR.
        let mut replayed = approval.clone();
        replayed.source_head = ours.clone();
        replayed.target_head = theirs.clone();
        assert!(verify_approval(&a, &replayed).is_err());
    }

    #[test]
    fn test42_approver_eligibility_is_judged_at_the_merge_base() {
        let (_store, alice, root, ours, theirs, _h) = forked();
        let sam = key(4);
        // Sam was a member who could approve at the merge base, and a
        // membership change afterwards took that away.
        let policy = ProtectedMain {
            needed: 1,
            reviewers: vec![],
            joined_late: vec![(pk(&sam), root.clone())],
        };
        assert!(policy.may_approve(&pk(&sam), Some(&root)));
        assert!(
            !policy.may_approve(&pk(&sam), Some(&ours)),
            "the fixture's membership really does differ after the base"
        );

        let request = mr(
            &pk(&alice),
            &root,
            &theirs,
            &ours,
            1,
            vec![sign_approval(&sam, "mr-h2", &theirs, &ours).unwrap()],
        );
        assert_eq!(
            count_fresh_approvals(&request, &policy).unwrap(),
            1,
            "a membership change after the base cannot retroactively invalidate an approval"
        );
    }

    #[test]
    fn merge_record_refuses_to_mix_two_objects() {
        let a = create("hypothesis", json!({"statement": "a"}));
        let b = create("experiment", json!({"title": "b"}));
        assert!(merge_record(None, Some(&a), Some(&b)).is_err());
        assert!(merge_record(None, None, None).is_err());
        // One side only: a creation, nothing to merge.
        assert_eq!(merge_record(None, Some(&a), None).unwrap().unwrap(), a);
    }

    #[test]
    fn an_unkeyed_array_is_one_opaque_value() {
        assert_eq!(array_rule("fields.stages_enabled"), ArrayRule::Opaque);
        assert_eq!(array_rule("attachments"), ArrayRule::SetByCid);
        assert_eq!(array_rule("fields.predictions"), ArrayRule::KeyedBy("label"));

        let (store, o, t, _b, _ours, _theirs) = diverged(
            |r| set_field(r, "stages_enabled", json!(["idea", "hypothesis"])),
            |r| set_field(r, "stages_enabled", json!(["idea", "funding"])),
        );
        let conflicts = match merge(&store, &o, &t, false).unwrap() {
            MergeOutcome::Conflicted(c) => c,
            other => panic!("an opaque list cannot be merged element-wise: {other:?}"),
        };
        assert_eq!(conflicts[0].path, "fields.stages_enabled");
    }

    #[test]
    fn up_to_date_and_fast_forward_are_distinguished() {
        let (mut store, alice, root, h) = scene();
        let v2 = edit(&h, |r| set_field(r, "statement", json!("two")));
        let ahead = commit_on(&mut store, &alice, &[&root], "two", &[(v2, ChangeKind::Edit)]);
        assert_eq!(merge(&store, &ahead, &ahead, false).unwrap(), MergeOutcome::UpToDate);
        assert_eq!(merge(&store, &ahead, &root, false).unwrap(), MergeOutcome::UpToDate);
        assert_eq!(
            merge(&store, &root, &ahead, false).unwrap(),
            MergeOutcome::FastForward { to: ahead }
        );
    }
}
