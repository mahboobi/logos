//! What the DAG needs from storage, and an in-memory implementation.
//!
//! The DAG, merge and record code take `&dyn CommitStore` rather than a
//! concrete type so none of them depend on where bytes actually live. That
//! keeps every algorithm testable against `MemStore` with no filesystem, no
//! encryption and no Keychain — and it means the durable implementation
//! (`olb-keys`, encrypted append-only log) can be replaced without touching
//! a line of version-control logic.

use crate::types::{CommitEnvelope, ObjectId, Record, RefTarget, VcsError};
use crate::Result;
use std::collections::BTreeMap;

/// Read and write access to commits, records and refs.
///
/// Every method is fallible only where failure is meaningful: a lookup that
/// finds nothing returns `None`, because "absent" is an ordinary state the
/// caller must handle (a missing parent is retried, not an error to bubble).
pub trait CommitStore {
    fn commit(&self, id: &str) -> Option<CommitEnvelope>;
    fn put_commit(&mut self, env: &CommitEnvelope) -> Result<()>;
    fn has_commit(&self, id: &str) -> bool {
        self.commit(id).is_some()
    }

    /// Records are keyed by content hash, never by `(object, version)` —
    /// two branches can each legitimately produce a version 3, and neither
    /// may overwrite the other (docs/10 §3.3).
    fn record(&self, hash: &str) -> Option<Record>;
    fn put_record(&mut self, rec: &Record) -> Result<()>;
    /// Every stored version of an object, in no guaranteed order.
    fn record_versions(&self, object: &ObjectId) -> Vec<Record>;

    fn ref_get(&self, name: &str) -> Option<RefTarget>;
    fn ref_set(&mut self, name: &str, target: RefTarget) -> Result<()>;
    fn ref_remove(&mut self, name: &str) -> Result<()>;
    fn refs(&self) -> Vec<(String, RefTarget)>;

    /// Commits held back because a parent has not arrived, keyed by the
    /// missing parent so landing that parent can wake exactly the right ones.
    fn pending_put(&mut self, missing_parent: &str, env: &CommitEnvelope) -> Result<()>;
    fn pending_take(&mut self, parent_now_present: &str) -> Vec<CommitEnvelope>;
    fn pending_all(&self) -> Vec<(String, CommitEnvelope)>;
}

/// The canonical name of the default branch. `main` always exists and is
/// protected (docs/10 §2.1).
pub const MAIN: &str = "refs/heads/main";
pub const HEAD: &str = "HEAD";

/// Branch ref name from a bare branch name.
pub fn branch_ref(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// Tag ref name from a bare tag name.
pub fn tag_ref(name: &str) -> String {
    format!("refs/tags/{name}")
}

/// In-memory store for tests and for anything that wants a scratch DAG.
#[derive(Debug, Default, Clone)]
pub struct MemStore {
    commits: BTreeMap<String, CommitEnvelope>,
    records: BTreeMap<String, Record>,
    refs: BTreeMap<String, RefTarget>,
    pending: BTreeMap<String, Vec<CommitEnvelope>>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// A store with `main` and `HEAD` already set up, as a real project has
    /// from its first commit.
    pub fn with_main(root_commit: &str) -> Self {
        let mut s = Self::new();
        s.refs.insert(
            MAIN.to_string(),
            RefTarget::Branch {
                commit: root_commit.to_string(),
            },
        );
        s.refs.insert(
            HEAD.to_string(),
            RefTarget::Symbolic {
                to: MAIN.to_string(),
            },
        );
        s
    }

    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }
}

impl CommitStore for MemStore {
    fn commit(&self, id: &str) -> Option<CommitEnvelope> {
        self.commits.get(id).cloned()
    }

    fn put_commit(&mut self, env: &CommitEnvelope) -> Result<()> {
        // Append-only: a second write of the same id is accepted only when
        // the content matches. Silently replacing a commit would rewrite
        // history, which CLAUDE.md forbids outright.
        if let Some(existing) = self.commits.get(&env.id) {
            if existing.header != env.header {
                return Err(VcsError::Refused(format!(
                    "commit {} already stored with different content",
                    env.id
                )));
            }
        }
        self.commits.insert(env.id.clone(), env.clone());
        Ok(())
    }

    fn record(&self, hash: &str) -> Option<Record> {
        self.records.get(hash).cloned()
    }

    fn put_record(&mut self, rec: &Record) -> Result<()> {
        let hash = crate::record::content_hash(rec)?;
        self.records.insert(hash, rec.clone());
        Ok(())
    }

    fn record_versions(&self, object: &ObjectId) -> Vec<Record> {
        self.records
            .values()
            .filter(|r| &r.id == object)
            .cloned()
            .collect()
    }

    fn ref_get(&self, name: &str) -> Option<RefTarget> {
        self.refs.get(name).cloned()
    }

    fn ref_set(&mut self, name: &str, target: RefTarget) -> Result<()> {
        // A tag is immutable: re-tagging an existing name is an error, not
        // an overwrite (docs/10 §2.1).
        if name.starts_with("refs/tags/") {
            if let Some(RefTarget::Tag { .. }) = self.refs.get(name) {
                return Err(VcsError::Refused(format!("tag {name} already exists")));
            }
        }
        self.refs.insert(name.to_string(), target);
        Ok(())
    }

    fn ref_remove(&mut self, name: &str) -> Result<()> {
        if name == MAIN {
            return Err(VcsError::Refused("main cannot be deleted".into()));
        }
        self.refs.remove(name);
        Ok(())
    }

    fn refs(&self) -> Vec<(String, RefTarget)> {
        self.refs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn pending_put(&mut self, missing_parent: &str, env: &CommitEnvelope) -> Result<()> {
        self.pending
            .entry(missing_parent.to_string())
            .or_default()
            .push(env.clone());
        Ok(())
    }

    fn pending_take(&mut self, parent_now_present: &str) -> Vec<CommitEnvelope> {
        self.pending.remove(parent_now_present).unwrap_or_default()
    }

    fn pending_all(&self) -> Vec<(String, CommitEnvelope)> {
        self.pending
            .iter()
            .flat_map(|(k, v)| v.iter().map(move |e| (k.clone(), e.clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_cannot_be_deleted() {
        let mut s = MemStore::with_main("abc");
        assert!(s.ref_remove(MAIN).is_err());
        assert!(s.ref_get(MAIN).is_some());
    }

    #[test]
    fn a_tag_cannot_be_moved() {
        let mut s = MemStore::new();
        let t = RefTarget::Tag {
            commit: "a".into(),
            tagger: "k".into(),
            sig: "s".into(),
        };
        s.ref_set(&tag_ref("v1"), t.clone()).unwrap();
        assert!(s.ref_set(&tag_ref("v1"), t).is_err());
    }

    #[test]
    fn a_branch_can_be_moved() {
        let mut s = MemStore::with_main("a");
        s.ref_set(MAIN, RefTarget::Branch { commit: "b".into() }).unwrap();
        assert!(matches!(
            s.ref_get(MAIN),
            Some(RefTarget::Branch { commit }) if commit == "b"
        ));
    }

    #[test]
    fn pending_wakes_only_the_right_commits() {
        let mut s = MemStore::new();
        let mk = |id: &str| CommitEnvelope {
            id: id.into(),
            header: crate::types::CommitHeader {
                v: 1,
                project: "p".into(),
                parents: vec![],
                tree: String::new(),
                author: String::new(),
                co_signers: vec![],
                message: String::new(),
                changes: vec![],
                files_added: vec![],
                files_removed: vec![],
                timestamp: 0,
            },
            sig: String::new(),
            co_sigs: vec![],
            meta: Default::default(),
        };
        s.pending_put("p1", &mk("c1")).unwrap();
        s.pending_put("p2", &mk("c2")).unwrap();

        let woken = s.pending_take("p1");
        assert_eq!(woken.len(), 1);
        assert_eq!(woken[0].id, "c1");
        // c2 is untouched: a pending commit must never block an unrelated one.
        assert_eq!(s.pending_all().len(), 1);
    }
}
