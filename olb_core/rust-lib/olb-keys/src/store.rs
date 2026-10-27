//! The durable, encrypted store (docs/08 R6, and the Phase 1b store
//! decision).
//!
//! Shape, and why:
//!
//! | File | Form | Why |
//! | --- | --- | --- |
//! | `commits.jsonl` | sealed append-only log | A commit append is O(1). The previous store rewrote one JSON document per write, which is O(total history) per commit and gets slower forever. |
//! | `records.jsonl` | sealed append-only log | Same, and records are immutable once written. |
//! | `refs.json` | sealed document | Small, mutable, rewritten whole. |
//! | `pending.json` | sealed document | Small, churny, and losing it costs only a re-request. |
//! | `state.json` | sealed document | Derived cache of the app-level view. Deletable: it is rebuilt from the logs. |
//!
//! This deviates from docs/07's "SQLite". The logs match the append-only
//! semantics CLAUDE.md already requires, stay auditable, and add no C
//! dependency to a nix build that everything else is queued behind. If query
//! power is ever needed, the logs are the source of truth and an index can be
//! derived from them without a format change.
//!
//! **Everything is sealed with the Keychain-held store key.** The directory
//! sits outside the tree `lgs basecamp launch` scrubs, so it survives a
//! relaunch — which also means it survives a laptop being stolen, hence the
//! encryption.

use crate::{vault, KeyError, Result};
use olb_vcs::store::CommitStore;
use olb_vcs::types::{CommitEnvelope, ObjectId, Record, RefTarget};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const AAD_COMMITS: &str = "olb.store.commits.v1";
const AAD_RECORDS: &str = "olb.store.records.v1";
const AAD_REFS: &str = "olb.store.refs.v1";
const AAD_PENDING: &str = "olb.store.pending.v1";
const AAD_STATE: &str = "olb.store.state.v1";

/// Where a profile's data lives.
///
/// Outside `.scaffold/basecamp/profiles/<p>/` deliberately: that tree is what
/// `lgs basecamp launch` deletes on every launch, so anything that must
/// outlive a relaunch cannot live there (docs/08 R6). Keyed by profile so
/// alice and bob never share state.
pub fn default_dir(profile: &str) -> PathBuf {
    let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(base)
        .join("Library/Application Support/OpenLabbook")
        .join(profile)
}

/// Commits, records and refs, encrypted at rest, indexed in memory.
///
/// The whole log is read into memory at open. At OLB's scale — thousands of
/// commits per project — that is a few megabytes and makes every read a map
/// lookup. If a project ever outgrows it, the logs stay the source of truth
/// and an on-disk index becomes an optimisation rather than a migration.
pub struct DurableStore {
    dir: PathBuf,
    key: [u8; 32],
    commits: BTreeMap<String, CommitEnvelope>,
    records: BTreeMap<String, Record>,
    refs: BTreeMap<String, RefTarget>,
    pending: BTreeMap<String, Vec<CommitEnvelope>>,
}

impl DurableStore {
    pub fn open(dir: impl AsRef<Path>, key: [u8; 32]) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| KeyError::Io(e.to_string()))?;

        let mut commits = BTreeMap::new();
        for line in vault::read_lines(&dir.join("commits.jsonl"), &key, AAD_COMMITS)? {
            let env: CommitEnvelope = serde_json::from_slice(&line)
                .map_err(|e| KeyError::Malformed(format!("commit log: {e}")))?;
            commits.insert(env.id.clone(), env);
        }

        let mut records = BTreeMap::new();
        for line in vault::read_lines(&dir.join("records.jsonl"), &key, AAD_RECORDS)? {
            let rec: Record = serde_json::from_slice(&line)
                .map_err(|e| KeyError::Malformed(format!("record log: {e}")))?;
            let hash = olb_vcs::record::content_hash(&rec)
                .map_err(|e| KeyError::Malformed(e.to_string()))?;
            records.insert(hash, rec);
        }

        let refs = match vault::read_sealed(&dir.join("refs.json"), &key, AAD_REFS)? {
            Some(b) => serde_json::from_slice(&b)
                .map_err(|e| KeyError::Malformed(format!("refs: {e}")))?,
            None => BTreeMap::new(),
        };

        let pending = match vault::read_sealed(&dir.join("pending.json"), &key, AAD_PENDING)? {
            Some(b) => serde_json::from_slice(&b)
                .map_err(|e| KeyError::Malformed(format!("pending: {e}")))?,
            None => BTreeMap::new(),
        };

        Ok(Self {
            dir,
            key,
            commits,
            records,
            refs,
            pending,
        })
    }

    /// Open the store for a profile, taking the key from the Keychain.
    pub fn open_profile(profile: &str) -> Result<Self> {
        let key = crate::keyring::store_key(profile)?;
        Self::open(default_dir(profile), *key)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn commit_count(&self) -> usize {
        self.commits.len()
    }

    /// The derived app-level view (projects, notes, profile). Not a source of
    /// truth — deleting it must only cost a rebuild.
    pub fn read_state(&self) -> Result<Option<serde_json::Value>> {
        match vault::read_sealed(&self.dir.join("state.json"), &self.key, AAD_STATE)? {
            Some(b) => Ok(Some(
                serde_json::from_slice(&b).map_err(|e| KeyError::Malformed(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    pub fn write_state(&self, v: &serde_json::Value) -> Result<()> {
        let bytes = serde_json::to_vec(v).map_err(|e| KeyError::Malformed(e.to_string()))?;
        vault::write_sealed(&self.dir.join("state.json"), &self.key, AAD_STATE, &bytes)
    }

    fn flush_refs(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.refs).map_err(|e| KeyError::Malformed(e.to_string()))?;
        vault::write_sealed(&self.dir.join("refs.json"), &self.key, AAD_REFS, &bytes)
    }

    fn flush_pending(&self) -> Result<()> {
        let bytes =
            serde_json::to_vec(&self.pending).map_err(|e| KeyError::Malformed(e.to_string()))?;
        vault::write_sealed(&self.dir.join("pending.json"), &self.key, AAD_PENDING, &bytes)
    }
}

/// Map a storage failure into the error type the VCS layer speaks.
fn as_vcs(e: KeyError) -> olb_vcs::VcsError {
    olb_vcs::VcsError::Malformed(e.to_string())
}

impl CommitStore for DurableStore {
    fn commit(&self, id: &str) -> Option<CommitEnvelope> {
        self.commits.get(id).cloned()
    }

    fn put_commit(&mut self, env: &CommitEnvelope) -> olb_vcs::Result<()> {
        if let Some(existing) = self.commits.get(&env.id) {
            // Append-only means append-only: the same id with different
            // content is a rewrite attempt, not an update.
            if existing.header != env.header {
                return Err(olb_vcs::VcsError::Refused(format!(
                    "commit {} already stored with different content",
                    env.id
                )));
            }
            return Ok(());
        }
        let bytes = serde_json::to_vec(env)
            .map_err(|e| olb_vcs::VcsError::Malformed(e.to_string()))?;
        vault::append_line(
            &self.dir.join("commits.jsonl"),
            &self.key,
            AAD_COMMITS,
            &bytes,
        )
        .map_err(as_vcs)?;
        self.commits.insert(env.id.clone(), env.clone());
        Ok(())
    }

    fn record(&self, hash: &str) -> Option<Record> {
        self.records.get(hash).cloned()
    }

    fn put_record(&mut self, rec: &Record) -> olb_vcs::Result<()> {
        let hash = olb_vcs::record::content_hash(rec)?;
        if self.records.contains_key(&hash) {
            // Content-addressed, so an identical hash is identical content.
            return Ok(());
        }
        let bytes =
            serde_json::to_vec(rec).map_err(|e| olb_vcs::VcsError::Malformed(e.to_string()))?;
        vault::append_line(
            &self.dir.join("records.jsonl"),
            &self.key,
            AAD_RECORDS,
            &bytes,
        )
        .map_err(as_vcs)?;
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

    fn ref_set(&mut self, name: &str, target: RefTarget) -> olb_vcs::Result<()> {
        if name.starts_with("refs/tags/") {
            if let Some(RefTarget::Tag { .. }) = self.refs.get(name) {
                return Err(olb_vcs::VcsError::Refused(format!(
                    "tag {name} already exists"
                )));
            }
        }
        self.refs.insert(name.to_string(), target);
        self.flush_refs().map_err(as_vcs)
    }

    fn ref_remove(&mut self, name: &str) -> olb_vcs::Result<()> {
        if name == olb_vcs::store::MAIN {
            return Err(olb_vcs::VcsError::Refused("main cannot be deleted".into()));
        }
        self.refs.remove(name);
        self.flush_refs().map_err(as_vcs)
    }

    fn refs(&self) -> Vec<(String, RefTarget)> {
        self.refs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn pending_put(&mut self, missing_parent: &str, env: &CommitEnvelope) -> olb_vcs::Result<()> {
        let slot = self.pending.entry(missing_parent.to_string()).or_default();
        if slot.iter().any(|e| e.id == env.id) {
            return Ok(());
        }
        slot.push(env.clone());
        self.flush_pending().map_err(as_vcs)
    }

    fn pending_take(&mut self, parent_now_present: &str) -> Vec<CommitEnvelope> {
        let taken = self.pending.remove(parent_now_present).unwrap_or_default();
        if !taken.is_empty() {
            // A failure to persist here costs a replay, not data: the commits
            // are returned either way and will be re-requested if lost.
            let _ = self.flush_pending();
        }
        taken
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
    use olb_vcs::store::MAIN;

    const KEY: [u8; 32] = [3u8; 32];

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "olb-store-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn envelope(id: &str, parents: Vec<String>) -> CommitEnvelope {
        CommitEnvelope {
            id: id.into(),
            header: olb_vcs::types::CommitHeader {
                v: 1,
                project: "olb:proj:test".into(),
                parents,
                tree: String::new(),
                author: String::new(),
                co_signers: vec![],
                message: format!("commit {id}"),
                changes: vec![],
                files_added: vec![],
                files_removed: vec![],
                timestamp: 0,
            },
            sig: String::new(),
            co_sigs: vec![],
            meta: Default::default(),
        }
    }

    #[test]
    fn commits_survive_a_reopen() {
        let d = tmpdir("reopen");
        {
            let mut s = DurableStore::open(&d, KEY).unwrap();
            s.put_commit(&envelope("a", vec![])).unwrap();
            s.put_commit(&envelope("b", vec!["a".into()])).unwrap();
        }
        let s = DurableStore::open(&d, KEY).unwrap();
        assert_eq!(s.commit_count(), 2);
        assert_eq!(s.commit("b").unwrap().header.parents, vec!["a".to_string()]);
    }

    #[test]
    fn the_store_is_unreadable_with_the_wrong_key() {
        // The point of the whole exercise: a copied directory is useless
        // without the key from the Keychain.
        let d = tmpdir("wrongkey");
        {
            let mut s = DurableStore::open(&d, KEY).unwrap();
            s.put_commit(&envelope("a", vec![])).unwrap();
        }
        assert!(DurableStore::open(&d, [9u8; 32]).is_err());
    }

    #[test]
    fn nothing_readable_is_left_in_plaintext_on_disk() {
        let d = tmpdir("plaintext");
        let mut s = DurableStore::open(&d, KEY).unwrap();
        s.put_commit(&envelope("a", vec![])).unwrap();
        s.ref_set(MAIN, RefTarget::Branch { commit: "a".into() })
            .unwrap();

        for entry in std::fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            let bytes = std::fs::read(&p).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains("olb:proj:test") && !text.contains("refs/heads/main"),
                "{p:?} leaks plaintext"
            );
        }
    }

    #[test]
    fn rewriting_a_commit_is_refused() {
        let d = tmpdir("rewrite");
        let mut s = DurableStore::open(&d, KEY).unwrap();
        s.put_commit(&envelope("a", vec![])).unwrap();

        let mut tampered = envelope("a", vec![]);
        tampered.header.message = "something else".into();
        assert!(s.put_commit(&tampered).is_err());
    }

    #[test]
    fn storing_the_same_commit_twice_is_fine() {
        let d = tmpdir("idempotent");
        let mut s = DurableStore::open(&d, KEY).unwrap();
        s.put_commit(&envelope("a", vec![])).unwrap();
        s.put_commit(&envelope("a", vec![])).unwrap();
        assert_eq!(s.commit_count(), 1);
    }

    #[test]
    fn refs_and_pending_survive_a_reopen() {
        let d = tmpdir("refs");
        {
            let mut s = DurableStore::open(&d, KEY).unwrap();
            s.ref_set(MAIN, RefTarget::Branch { commit: "a".into() })
                .unwrap();
            s.pending_put("missing", &envelope("c", vec!["missing".into()]))
                .unwrap();
        }
        let mut s = DurableStore::open(&d, KEY).unwrap();
        assert!(s.ref_get(MAIN).is_some());
        assert_eq!(s.pending_take("missing").len(), 1);
    }

    #[test]
    fn state_is_derived_and_a_missing_one_is_not_an_error() {
        let d = tmpdir("state");
        let s = DurableStore::open(&d, KEY).unwrap();
        assert_eq!(s.read_state().unwrap(), None);
        s.write_state(&serde_json::json!({"projects": []})).unwrap();
        assert!(s.read_state().unwrap().is_some());
    }
}
