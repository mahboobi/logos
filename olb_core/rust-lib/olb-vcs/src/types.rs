//! The shapes everything else agrees on. Specified by `docs/10-vcs-design.md`
//! §1.3 (header), §1.5 (envelope), §3 (records) and §4.3 (conflicts).
//!
//! Field names and their JSON spelling are load-bearing: they go into
//! `canon()` and therefore into every commit id and signature. Renaming one
//! silently changes every id in a store. Add fields at the end of a struct and
//! bump `v`; never rename or reorder for cosmetic reasons.

use serde::{Deserialize, Serialize};

/// An object id: `olb:obj:<base32-nopad-lowercase of the v1 record hash>`.
///
/// A newtype rather than a `String` so a record hash and an object id cannot
/// be swapped at a call site — they are both 64-ish character strings and the
/// compiler is the only thing that will ever notice the difference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub String);

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a change did to an object. `Flag` covers retraction — there is no
/// delete (docs/10 §3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Create,
    Edit,
    Flag,
    Restore,
    Merge,
}

/// One object's transition inside a commit. `record` is the content hash of
/// the resulting version, which is what lets a peer verify a commit without
/// trusting whoever served the record store (docs/10 §1.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub object: ObjectId,
    pub from_version: u64,
    pub to_version: u64,
    pub kind: ChangeKind,
    /// 64 hex characters.
    pub record: String,
    /// Only set when `kind == Restore`: the version that was restored from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<u64>,
}

/// The hashed and signed part of a commit (docs/10 §1.3).
///
/// Note what is NOT here: `branch`. A commit's branch changes when the branch
/// is renamed, merged or deleted, so hashing it would give identical content
/// different ids on different branches and a merge could never recognise the
/// same commit. It lives in `CommitMeta` as a display hint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitHeader {
    pub v: u32,
    pub project: String,
    /// 0 entries = root, 1 = normal, 2+ = merge. `parents[0]` is the TARGET
    /// side of a merge, so the first-parent chain is the mainline and S6 can
    /// draw it as lane 0. `parents[1..]` are sorted by id for determinism
    /// (docs/10 §2.3).
    pub parents: Vec<String>,
    /// State root after this commit, 64 hex (docs/10 §3.4).
    pub tree: String,
    /// 64 hex ed25519 public key.
    pub author: String,
    /// Approvers and agent operators. Sorted and deduplicated.
    #[serde(default)]
    pub co_signers: Vec<String>,
    pub message: String,
    /// Sorted by `object`.
    pub changes: Vec<Change>,
    /// Sorted, unique, `logos:<cid>` form.
    #[serde(default)]
    pub files_added: Vec<String>,
    #[serde(default)]
    pub files_removed: Vec<String>,
    /// Milliseconds since the Unix epoch, UTC.
    pub timestamp: i64,
}

/// Where an anchor placed this commit. Local only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorRef {
    /// 0-based index of this commit's leaf in the cumulative tree.
    pub leaf: u64,
    /// Which flush carried it.
    pub batch: u64,
}

/// Local, mutable, never hashed, and never trusted when it arrives from a
/// peer — a peer's `meta` is discarded and rebuilt on receipt (docs/10 §1.5).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMeta {
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub received_at: i64,
    /// `1 + max(gen of parents)`, `gen(root) = 0`. Drives merge-base search
    /// (docs/10 §2.4).
    #[serde(default)]
    pub gen: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<AnchorRef>,
}

/// A commit as stored and as sent on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitEnvelope {
    /// 64 hex: `SHA256(b"olb.commit.v1\0" || canon(header))`.
    pub id: String,
    pub header: CommitHeader,
    /// 128 hex ed25519 signature over the same preimage the id hashes.
    pub sig: String,
    /// Parallel to `header.co_signers`; each signs the 32-byte id under a
    /// different domain tag (docs/10 §1.4).
    #[serde(default)]
    pub co_sigs: Vec<String>,
    #[serde(default)]
    pub meta: CommitMeta,
}

/// A research object version (docs/05 §1, extended by docs/10 §3.2).
///
/// `fields` is type-specific and stays a free-form object: the type table in
/// docs/05 §1.1 has seven shapes and they are still moving. `prev` is a `Vec`
/// from day one because a merged version genuinely has two predecessors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: ObjectId,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u64,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idea: Option<ObjectId>,
    #[serde(default)]
    pub authors: Vec<serde_json::Value>,
    #[serde(default)]
    pub agents: Vec<serde_json::Value>,
    /// Derivation, per docs/05 §1 — NOT version lineage. That is `prev`.
    #[serde(default)]
    pub parents: Vec<ObjectId>,
    /// Version lineage: record hashes of the immediately preceding versions.
    /// Empty for version 1; two or more for a merged version (docs/10 §3.2).
    #[serde(default)]
    pub prev: Vec<String>,
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    #[serde(default)]
    pub flags: serde_json::Map<String, serde_json::Value>,
    pub visibility: String,
    pub created_at: String,
}

/// What a ref points at (docs/10 §2.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RefTarget {
    /// `refs/heads/<name>` — a mutable branch pointer.
    Branch { commit: String },
    /// `refs/tags/<name>` — immutable. Re-tagging an existing name is an
    /// error, not an overwrite.
    Tag {
        commit: String,
        tagger: String,
        sig: String,
    },
    /// `HEAD` pointing at a branch.
    Symbolic { to: String },
    /// `HEAD` pointing straight at a commit — detached, read-only in the UI.
    Detached { commit: String },
}

/// One unmergeable field, as shown to the user and as serialised for the UI
/// (docs/10 §4.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub object: ObjectId,
    /// Dotted path inside the record, e.g. `fields.observations`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ours: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theirs: Option<serde_json::Value>,
    /// Why this could not be resolved automatically, in words a researcher
    /// can act on. Shown verbatim in the UI, so no jargon and no codes.
    pub reason: String,
}

/// Closed on purpose. Every variant is something a caller branches on:
/// `MissingParent` is retried when the parent lands, `BadSignature` is dropped
/// and logged, `Conflicts` is rendered. A stringly-typed error would collapse
/// all three into "something went wrong".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VcsError {
    /// A value could not be put in canonical form — in practice always a
    /// float, which is refused rather than encoded lossily (docs/10 §1.1).
    NotCanonical(String),
    /// The recomputed id does not match the claimed one.
    IdMismatch { claimed: String, computed: String },
    BadSignature(String),
    /// The commit cannot be applied yet. Hold it and retry when this parent
    /// arrives; it must never block unrelated commits (docs/10 §2.5 step 4).
    MissingParent(String),
    /// A referenced record blob is absent from the store.
    MissingRecord(String),
    /// `from_version` did not match the object's head on the parent state, or
    /// `to_version != from_version + 1`.
    VersionGap {
        object: ObjectId,
        expected: u64,
        found: u64,
    },
    /// The recomputed state root disagrees with the header.
    TreeMismatch { claimed: String, computed: String },
    /// Merge produced conflicts that a human must resolve.
    Conflicts(Vec<Conflict>),
    /// The commit DAG and the record `prev` chain disagree — what a tampered
    /// store or a buggy apply looks like (docs/10 §3.3).
    ChainDisagreement(String),
    NotFound(String),
    /// A rule was broken: re-tagging, writing to a protected branch without
    /// approvals, an author who is not a member.
    Refused(String),
    Malformed(String),
}

impl std::fmt::Display for VcsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsError::NotCanonical(m) => write!(f, "not canonical: {m}"),
            VcsError::IdMismatch { claimed, computed } => {
                write!(f, "commit id mismatch: claimed {claimed}, computed {computed}")
            }
            VcsError::BadSignature(m) => write!(f, "bad signature: {m}"),
            VcsError::MissingParent(p) => write!(f, "missing parent {p}"),
            VcsError::MissingRecord(h) => write!(f, "missing record {h}"),
            VcsError::VersionGap { object, expected, found } => write!(
                f,
                "version gap on {object}: expected {expected}, found {found}"
            ),
            VcsError::TreeMismatch { claimed, computed } => {
                write!(f, "tree mismatch: claimed {claimed}, computed {computed}")
            }
            VcsError::Conflicts(c) => write!(f, "{} conflict(s)", c.len()),
            VcsError::ChainDisagreement(m) => write!(f, "version chain disagreement: {m}"),
            VcsError::NotFound(m) => write!(f, "not found: {m}"),
            VcsError::Refused(m) => write!(f, "refused: {m}"),
            VcsError::Malformed(m) => write!(f, "malformed: {m}"),
        }
    }
}

impl std::error::Error for VcsError {}
