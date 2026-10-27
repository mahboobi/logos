//! The seam between the module surface and the Phase 1b/1c crates.
//!
//! Everything the trait in `lib.rs` exposes is implemented here, on top of
//! `olb_vcs` (records, the commit DAG) and `olb_keys` (identity, the
//! encrypted store). `lib.rs` keeps only the trait, the IPC glue and the
//! Delivery/Storage bring-up.
//!
//! Three changes this makes to how Phase 0 behaved:
//!
//! 1. **The store is encrypted** with a key from the login Keychain, and it
//!    lives outside the tree `lgs basecamp launch` scrubs.
//! 2. **Commits are real.** Phase 0 appended `{action, payload}` entries to a
//!    JSON array. Now every user action writes a versioned `Record` and a
//!    signed commit with proper `changes[]` and a recomputed state root, so
//!    history, sync and anchoring all have something true to operate on.
//! 3. **History is per project.** It used to be one global log across every
//!    project on the node.

use olb_keys::identity::SecretIdentity;
use olb_keys::store::DurableStore;
use olb_vcs::store::CommitStore;
use olb_vcs::types::{Change, ChangeKind, ObjectId, Record, RefTarget};
use serde_json::{json, Value};

/// Refs are namespaced per project. One node holds many projects in one
/// store, so a bare `refs/heads/main` would make every project share a
/// branch — the first thing that would break is history, which is now
/// required to be per project.
fn main_ref(project_id: &str) -> String {
    format!("refs/projects/{project_id}/heads/main")
}

pub struct App {
    pub store: DurableStore,
    pub identity: SecretIdentity,
    /// Derived view the screens read: projects, notes, profile, current.
    /// Rebuildable from the commit log in principle; cached because every
    /// screen reads it and walking the DAG per read would be wasteful.
    pub state: Value,
}

/// Why the store could not be opened. Surfaced to the UI rather than
/// panicking: a Keychain prompt the user has not answered yet is an ordinary
/// state, not a crash.
pub struct Locked {
    pub reason: String,
}

fn empty_state() -> Value {
    json!({
        "v": 2,
        "profile": {"name": "", "orcid": ""},
        "projects": [],
        "notes": [],
        "current": Value::Null,
    })
}

impl App {
    /// Open this profile's store and identity.
    ///
    /// The first call on a machine makes macOS prompt for Keychain access.
    /// That prompt is the cost of the key not living beside the data; until
    /// it is answered this returns `Locked` and the UI says so.
    pub fn open(profile: &str) -> Result<App, Locked> {
        let key = olb_keys::keyring::store_key(profile).map_err(|e| Locked {
            reason: format!("the store key is not available: {e}"),
        })?;

        let dir = olb_keys::store::default_dir(profile);
        let store = DurableStore::open(&dir, *key).map_err(|e| Locked {
            reason: format!("the store could not be opened: {e}"),
        })?;

        let state = store
            .read_state()
            .ok()
            .flatten()
            .unwrap_or_else(empty_state);

        let name = state
            .pointer("/profile/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let orcid = state
            .pointer("/profile/orcid")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        // Migrates a Phase 0 plaintext identity.key by ADOPTING it, never by
        // generating a replacement: a node that changes its author key
        // orphans every commit it has ever signed.
        let identity = olb_keys::identity::load_or_create(&dir, &key, name, orcid).map_err(|e| {
            Locked {
                reason: format!("the identity could not be opened: {e}"),
            }
        })?;

        Ok(App {
            store,
            identity,
            state,
        })
    }

    pub fn author_hex(&self) -> String {
        hex::encode(self.identity.signing.verifying_key().to_bytes())
    }

    pub fn save_state(&self) -> Result<(), String> {
        self.store.write_state(&self.state).map_err(|e| e.to_string())
    }

    // ---- records and commits ------------------------------------------

    /// Build a version-1 record of `kind`.
    ///
    /// The id is stamped after hashing, because a record carries its own id
    /// inside the hashed body and docs/10 §3.1 is otherwise circular: hash
    /// the draft with an empty id, then write the result in.
    pub fn new_record(&self, kind: &str, project: &str, fields: Value) -> Result<Record, String> {
        let mut rec = Record {
            id: ObjectId(String::new()),
            kind: kind.to_string(),
            version: 1,
            project: project.to_string(),
            idea: None,
            authors: vec![json!({"key": self.author_hex(), "role": "author", "orcid": null})],
            agents: vec![],
            parents: vec![],
            prev: vec![],
            fields: fields.as_object().cloned().unwrap_or_default(),
            attachments: vec![],
            flags: serde_json::Map::new(),
            visibility: "group".into(),
            created_at: rfc3339_now(),
        };
        let id = olb_vcs::record::object_id_of_first_version(&rec).map_err(|e| e.to_string())?;
        rec.id = id;
        Ok(rec)
    }

    /// The state root after applying `change` on top of `parent`.
    fn tree_after(
        &self,
        parent: Option<&str>,
        object: &ObjectId,
        version: u64,
        record_hash: &str,
    ) -> Result<String, String> {
        let mut live = match parent {
            Some(p) => olb_vcs::record::live_objects(&self.store, p).map_err(|e| e.to_string())?,
            None => Vec::new(),
        };
        match live.iter_mut().find(|(id, _, _)| id == object) {
            Some(slot) => {
                slot.1 = version;
                slot.2 = record_hash.to_string();
            }
            None => live.push((object.clone(), version, record_hash.to_string())),
        }
        olb_vcs::record::state_root(&live).map_err(|e| e.to_string())
    }

    /// Write a record and the signed commit that introduces or updates it.
    ///
    /// The commit is pushed through `dag::receive` rather than straight into
    /// the store, so a locally authored commit passes exactly the same eight
    /// checks an incoming one does. If our own tree or version arithmetic is
    /// wrong, it fails here rather than on a peer's node.
    pub fn commit_record(
        &mut self,
        rec: &Record,
        kind: ChangeKind,
        message: &str,
    ) -> Result<String, String> {
        let hash = olb_vcs::record::content_hash(rec).map_err(|e| e.to_string())?;
        self.store.put_record(rec).map_err(|e| e.to_string())?;

        let branch = main_ref(&rec.project);
        let parent = match self.store.ref_get(&branch) {
            Some(RefTarget::Branch { commit }) => Some(commit),
            _ => None,
        };
        let parents: Vec<String> = parent.iter().cloned().collect();
        let tree = self.tree_after(parent.as_deref(), &rec.id, rec.version, &hash)?;

        let change = Change {
            object: rec.id.clone(),
            from_version: rec.version.saturating_sub(1),
            to_version: rec.version,
            kind,
            record: hash,
            restored_from: None,
        };

        let env = olb_vcs::dag::build_commit(
            &self.identity.signing,
            &rec.project,
            parents,
            tree,
            message.to_string(),
            vec![change],
            vec![],
            vec![],
            now_ms(),
        )
        .map_err(|e| e.to_string())?;

        let id = env.id.clone();
        olb_vcs::dag::receive(&mut self.store, &olb_vcs::dag::OpenPolicy, &env)
            .map_err(|e| format!("our own commit failed validation: {e}"))?;
        olb_vcs::dag::set_branch(&mut self.store, &branch, &id, false)
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// Commits on one project, newest first, with the lane data S6 draws.
    ///
    /// Per project, not global: a commit header carries its project and the
    /// refs are namespaced, so this is the branch walk rather than a filter
    /// over everything the node has ever done.
    pub fn history(&self, project_id: &str, limit: usize) -> Value {
        let branch = main_ref(project_id);
        let head = match self.store.ref_get(&branch) {
            Some(RefTarget::Branch { commit }) => commit,
            _ => return json!({"commits": [], "branches": [], "tags": []}),
        };

        let rows = olb_vcs::dag::log(&self.store, &head, limit).unwrap_or_default();
        let commits: Vec<Value> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let h = &r.commit.header;
                let mine = h.author == self.author_hex();
                let who = if mine {
                    self.state
                        .pointer("/profile/name")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or("You")
                        .to_string()
                } else {
                    // No member directory yet, so a peer is identified by the
                    // only thing that is actually verified: their key.
                    format!("{}…", h.author.chars().take(8).collect::<String>())
                };
                let n = h.changes.len();
                // Shapes S6 already renders. Keeping `badge` and `meta` means
                // the screen shows real commits today rather than blanks
                // waiting on a redesign.
                let badge = match r.commit.meta.anchor {
                    Some(a) => json!({"kind": "done", "text": format!("Batch {}", a.batch)}),
                    None => json!({"kind": "attention", "text": "Waiting for next anchor"}),
                };
                json!({
                    "id": h_short(&r.commit.id),
                    "fullId": r.commit.id,
                    "message": h.message,
                    "lane": r.lane,
                    "badge": badge,
                    "meta": format!(
                        "{who} · signed · {}",
                        if n == 1 { "1 record".to_string() } else { format!("{n} records") }
                    ),
                    "selected": i == 0,
                    "merge": if r.merge { json!({"fromLane": 1}) } else { Value::Null },
                    "mine": mine,
                    "at": h.timestamp,
                    "through": r.through,
                    "fork": r.fork,
                    "parents": h.parents,
                })
            })
            .collect();

        let branches: Vec<Value> = self
            .store
            .refs()
            .into_iter()
            .filter_map(|(name, t)| {
                let prefix = format!("refs/projects/{project_id}/heads/");
                let short = name.strip_prefix(&prefix)?.to_string();
                match t {
                    RefTarget::Branch { commit } => Some(json!({
                        "name": short,
                        "commit": commit,
                        "status": if short == "main" { "Protected" } else { "" },
                        "meta": if short == "main" {
                            "The shared record"
                        } else {
                            ""
                        },
                    })),
                    _ => None,
                }
            })
            .collect();

        json!({"commits": commits, "branches": branches, "tags": []})
    }
}

/// S6 shows short hashes; the full id stays alongside for anything that
/// needs to address the commit.
fn h_short(id: &str) -> String {
    id.chars().take(7).collect()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// RFC 3339 in UTC, without pulling in a date library for one format string.
pub fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Howard Hinnant's civil_from_days. Small, exact, and avoids a dependency
/// whose only job would be printing one timestamp format.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// This profile's store directory, for the Settings screen.
pub fn profile_dir() -> std::path::PathBuf {
    let profile = std::env::var("LOGOS_PROFILE").unwrap_or_else(|_| "default".into());
    olb_keys::store::default_dir(&profile)
}

/// A fresh project id.
///
/// `olb:proj:` and `olb:obj:` are different namespaces (docs/05 §1): an
/// object id is the hash of a record's first version, but a project id is
/// not derived from any record — the project record carries BOTH, its own
/// object id and the project it belongs to, which is itself.
pub fn new_project_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 10];
    rand::rngs::OsRng.fill_bytes(&mut b);
    format!(
        "olb:proj:{}",
        data_encoding::BASE32_NOPAD.encode(&b).to_lowercase()
    )
}
