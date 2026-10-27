//! Open Labbook version control: canonical form, records, the commit DAG,
//! merge, and the Merkle tree anchors are built from.
//!
//! This crate is pure: no Logos SDK, no IPC, no I/O beyond what callers hand
//! it. That is the point — `olb_core/rust-lib` cannot be built outside the
//! nix module build (its `logos-rust-sdk` path dependency is staged by the
//! builder), so anything living there cannot have a `cargo test`. CLAUDE.md
//! requires tests for record versioning, the commit DAG and merge, and Merkle
//! proofs. They live here.
//!
//! Specification: `docs/10-vcs-design.md`. Where this crate and docs/05
//! disagree, docs/10 §7 records which was chosen and why.

pub mod canon;
pub mod dag;
pub mod hashing;
pub mod merge;
pub mod merkle;
pub mod record;
pub mod store;
pub mod types;

pub use types::{
    Change, ChangeKind, CommitEnvelope, CommitHeader, CommitMeta, Conflict, ObjectId, Record,
    RefTarget, VcsError,
};

/// Result alias used throughout. `VcsError` is deliberately a closed enum
/// rather than a string: every variant here is something a caller may need to
/// branch on (a missing parent is retried, a bad signature is dropped and
/// logged, a conflict is shown to the user).
pub type Result<T> = std::result::Result<T, VcsError>;
