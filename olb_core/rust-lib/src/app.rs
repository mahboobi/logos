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

/// Read a Phase 0 `store.json` and carry its contents into the new shape.
///
/// Only the parts that are still meaningful: projects, notes, profile and
/// the current selection. Its `commits` array is deliberately dropped — those
/// entries were `{action, payload}` records with no parents, no tree and no
/// verifiable signature, so importing them as commits would manufacture a
/// history that never existed. The real commit log starts from the next
/// action, and the original file is kept for anyone who wants to look.
fn adopt_legacy_store(dir: &std::path::Path) -> Option<Value> {
    let text = std::fs::read_to_string(dir.join("store.json")).ok()?;
    let old: Value = serde_json::from_str(&text).ok()?;
    let mut v = empty_state();
    for key in ["projects", "notes", "profile", "current"] {
        if let Some(found) = old.get(key) {
            v[key] = found.clone();
        }
    }
    Some(v)
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
    /// Open with a store key the caller has already unwrapped.
    ///
    /// The key arrives from `passphrase::unlock`, so this is only reachable
    /// once the user has actually typed something that opens the store —
    /// there is no path here that invents a key.
    pub fn open_with_key(profile: &str, key: &zeroize::Zeroizing<[u8; 32]>) -> Result<App, Locked> {
        let dir = olb_keys::store::default_dir(profile);
        let store = DurableStore::open(&dir, **key).map_err(|e| Locked {
            reason: format!("the store could not be opened: {e}"),
        })?;

        // Sealed state if we have it; otherwise adopt the Phase 0 plaintext
        // store.json sitting in the same directory. Migrating the identity
        // without the document it describes left a node with the right
        // author key and none of its own projects — which looks exactly
        // like data loss to the person holding it.
        let state = match store.read_state().ok().flatten() {
            Some(v) => v,
            None => match adopt_legacy_store(&dir) {
                Some(v) => {
                    if let Err(e) = store.write_state(&v) {
                        eprintln!("[olb_core] could not seal the migrated store: {e}");
                    } else {
                        // Keep the original. A migration that deletes its
                        // own source leaves nothing to re-run or inspect if
                        // the import turns out to be wrong.
                        let from = dir.join("store.json");
                        let to = dir.join("store.json.migrated");
                        if let Err(e) = std::fs::rename(&from, &to) {
                            eprintln!("[olb_core] could not set the legacy store aside: {e}");
                        }
                        eprintln!(
                            "[olb_core] migrated {} project(s) and {} note(s) from store.json",
                            v.get("projects").and_then(|x| x.as_array()).map_or(0, |a| a.len()),
                            v.get("notes").and_then(|x| x.as_array()).map_or(0, |a| a.len()),
                        );
                    }
                    v
                }
                None => empty_state(),
            },
        };

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
        let identity = olb_keys::identity::load_or_create(&dir, key, name, orcid).map_err(|e| {
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

// ---------------------------------------------------------------------------
// Project secrets, sync frames and invites.
//
// A project's metadata key and salt never cross the IPC seam (CLAUDE.md:
// the UI never sees a project key or a salt), so they live in their own
// sealed file and are reachable only from here.
// ---------------------------------------------------------------------------

use olb_keys::invite::{ContentKey, KeyScope};
use zeroize::Zeroizing;

/// A frame as it travels on a shared bucket topic.
///
/// `tag` is what lets a receiver skip another project's traffic without
/// attempting to decrypt it. It is already a hash of the project id under a
/// secret salt, so it identifies the conversation without naming the
/// project to anyone who is not in it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Frame {
    tag: String,
    body: String,
}

impl App {
    fn secrets(&self) -> Value {
        self.store
            .read_secrets()
            .ok()
            .flatten()
            .unwrap_or_else(|| json!({}))
    }

    fn put_secret(&self, project_id: &str, key_hex: &str) -> Result<(), String> {
        let mut sec = self.secrets();
        sec[project_id] = json!({"key": key_hex});
        self.store.write_secrets(&sec).map_err(|e| e.to_string())
    }

    /// Mint a project's metadata key. Called once, at creation.
    pub fn create_project_secret(&self, project_id: &str) -> Result<(), String> {
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        self.put_secret(project_id, &hex::encode(key))
    }

    /// Record a key that arrived in an invite.
    pub fn adopt_project_key(&self, project_id: &str, key: &ContentKey) -> Result<(), String> {
        // Zeroizing derefs to the array; AsRef would be ambiguous here.
        self.put_secret(project_id, &hex::encode(&**key))
    }

    fn project_key(&self, project_id: &str) -> Option<ContentKey> {
        let hexs = self.secrets().get(project_id)?.get("key")?.as_str()?.to_string();
        let b = hex::decode(hexs).ok()?;
        Some(Zeroizing::new(<[u8; 32]>::try_from(b.as_slice()).ok()?))
    }

    /// The project's tag salt, DERIVED from its key rather than stored
    /// beside it. One secret then travels in an invite instead of two, and
    /// a member who holds the key can never be missing the salt.
    fn project_salt(&self, project_id: &str) -> Option<Vec<u8>> {
        let key = self.project_key(project_id)?;
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, key.as_ref());
        let mut out = [0u8; 32];
        hk.expand(b"olb.project.salt.v1", &mut out).ok()?;
        Some(out.to_vec())
    }

    /// The opaque conversation id for a project, as it appears on the wire.
    pub fn project_tag(&self, project_id: &str) -> Option<String> {
        let salt = self.project_salt(project_id)?;
        Some(olb_vcs::anchor::project_tag(project_id, &salt))
    }

    /// Every project this node holds keys for, with its topic and tag.
    pub fn synced_projects(&self) -> Vec<(String, String, String)> {
        self.secrets()
            .as_object()
            .map(|m| {
                m.keys()
                    .filter_map(|id| {
                        let tag = self.project_tag(id)?;
                        Some((id.clone(), tag, olb_vcs::sync::project_topic(id)))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Seal a sync message for a project's conversation.
    ///
    /// The bucket is shared with every other OLB project that hashes into
    /// it, so a plaintext frame would hand them this project's commits.
    pub fn seal_frame(
        &self,
        project_id: &str,
        msg: &olb_vcs::sync::SyncMessage,
    ) -> Result<Vec<u8>, String> {
        let key = self
            .project_key(project_id)
            .ok_or_else(|| format!("no key for {project_id}"))?;
        let tag = self
            .project_tag(project_id)
            .ok_or_else(|| format!("no salt for {project_id}"))?;
        let plain = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
        let sealed = olb_keys::vault::seal(&key, SYNC_AAD, &plain).map_err(|e| e.to_string())?;
        let frame = Frame {
            tag,
            body: data_encoding::BASE64.encode(&sealed),
        };
        serde_json::to_vec(&frame).map_err(|e| e.to_string())
    }

    /// Open a frame if it belongs to a project we hold a key for.
    ///
    /// Returns `None` for somebody else's traffic, which on a shared bucket
    /// is most of it — that is an ordinary outcome, not an error.
    pub fn open_frame(&self, bytes: &[u8]) -> Option<(String, olb_vcs::sync::SyncMessage)> {
        let frame: Frame = serde_json::from_slice(bytes).ok()?;
        let project_id = self
            .synced_projects()
            .into_iter()
            .find(|(_, tag, _)| *tag == frame.tag)
            .map(|(id, _, _)| id)?;
        let key = self.project_key(&project_id)?;
        let sealed = data_encoding::BASE64.decode(frame.body.as_bytes()).ok()?;
        let plain = olb_keys::vault::open(&key, SYNC_AAD, &sealed).ok()?;
        Some((project_id, serde_json::from_slice(&plain).ok()?))
    }

    /// This project's branch heads, as advertised to peers.
    pub fn heads(&self, project_id: &str) -> Vec<(String, String)> {
        let prefix = format!("refs/projects/{project_id}/heads/");
        self.store
            .refs()
            .into_iter()
            .filter_map(|(name, t)| match t {
                RefTarget::Branch { commit } => {
                    Some((name.strip_prefix(&prefix)?.to_string(), commit))
                }
                _ => None,
            })
            .collect()
    }

    pub fn heads_message(&self, project_id: &str) -> olb_vcs::sync::SyncMessage {
        olb_vcs::sync::SyncMessage::Heads {
            project: project_id.to_string(),
            heads: self.heads(project_id),
            anchor: None,
        }
    }

    /// Commits a peer asked for, packed to stay under the send budget.
    pub fn commits_for(
        &self,
        project_id: &str,
        ids: &[String],
    ) -> Result<Vec<olb_vcs::sync::SyncMessage>, String> {
        let envs: Vec<_> = ids.iter().filter_map(|id| self.store.commit(id)).collect();
        olb_vcs::sync::pack(project_id, &envs).map_err(|e| e.to_string())
    }

    /// Apply commits a peer sent. Returns how many were newly applied.
    ///
    /// Every one goes through `dag::receive`, so a peer cannot push a commit
    /// past the checks our own commits face.
    pub fn apply_incoming(&mut self, commits: &[olb_vcs::types::CommitEnvelope]) -> usize {
        let mut applied = 0;
        for env in commits {
            match olb_vcs::dag::receive(&mut self.store, &olb_vcs::dag::OpenPolicy, env) {
                Ok(olb_vcs::dag::Received::Applied { also_applied }) => {
                    applied += 1 + also_applied.len();
                    let branch = main_ref(&env.header.project);
                    let _ = olb_vcs::dag::set_branch(&mut self.store, &branch, &env.id, false);
                }
                Ok(_) => {}
                Err(e) => eprintln!("[olb_core] dropped a peer commit: {e}"),
            }
        }
        applied
    }
}

/// AAD for a sealed sync frame.
const SYNC_AAD: &str = "olb.sync.frame.v1";

// ---------------------------------------------------------------------------
// Invites. What actually lets two nodes share a project.
// ---------------------------------------------------------------------------

impl App {
    /// The `olb1:` string a user copies and hands to someone else.
    pub fn my_olb_id(&self) -> String {
        olb_keys::identity::encode_id(&self.identity.public).unwrap_or_default()
    }

    pub fn my_fingerprint(&self) -> String {
        olb_keys::identity::fingerprint(&self.identity.public)
    }

    /// Where invites for this node arrive.
    pub fn my_inbox_topic(&self) -> String {
        olb_vcs::sync::inbox_topic(&self.identity.public.seal_pk)
    }

    /// Build an invite carrying this project's metadata key.
    ///
    /// The wire envelope deliberately carries an EMPTY `project_name`. The
    /// invite bucket is shared with fifteen-sixteenths of everyone else, and
    /// a readable project name on it tells strangers who is starting what.
    /// The real name travels in the note, which is sealed to the invitee.
    pub fn build_invite_for(
        &self,
        project_id: &str,
        invitee_olb_id: &str,
        role: olb_keys::invite::Role,
    ) -> Result<(olb_keys::invite::InviteEnvelope, String), String> {
        let invitee = olb_keys::identity::decode_id(invitee_olb_id)
            .map_err(|e| format!("that is not a usable Open Labbook ID: {e}"))?;

        let key = self
            .project_key(project_id)
            .ok_or_else(|| format!("no key for {project_id}"))?;

        let name = self
            .state
            .get("projects")
            .and_then(|v| v.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(project_id))
            })
            .and_then(|p| p.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let note = json!({"projectName": name, "from": self.my_fingerprint()}).to_string();
        let expiry = olb_keys::identity::now_unix() + 14 * 24 * 60 * 60;

        let env = olb_keys::invite::build_invite(
            &self.identity,
            &invitee,
            project_id,
            "",
            role,
            &[],
            &[(KeyScope::ProjectMeta, 1, key)],
            &note,
            expiry,
        )
        .map_err(|e| e.to_string())?;

        let topic = olb_vcs::sync::inbox_topic(&invitee.seal_pk);
        Ok((env, topic))
    }

    /// Hold an invite that arrived, for the user to look at.
    ///
    /// Verification is not acceptance (docs/04 §9). A signature proves who
    /// sent it, never that the recipient wants it, so nothing is installed
    /// here — the UI shows the inviter's fingerprint and waits.
    pub fn record_incoming_invite(
        &mut self,
        env: &olb_keys::invite::InviteEnvelope,
    ) -> Result<(), String> {
        olb_keys::invite::verify_invite(env, olb_keys::identity::now_unix())
            .map_err(|e| e.to_string())?;

        let note = olb_keys::invite::open_note(&self.identity, env).unwrap_or_default();
        let parsed: Value = serde_json::from_str(&note).unwrap_or_else(|_| json!({}));

        let row = json!({
            "inviteId": env.invite_id,
            "projectId": env.project_id,
            "projectName": parsed.get("projectName").and_then(|v| v.as_str()).unwrap_or(""),
            "inviterFingerprint": parsed.get("from").and_then(|v| v.as_str()).unwrap_or(""),
            "inviter": env.inviter,
            "role": format!("{:?}", env.role),
            "expiry": env.expiry,
            "envelope": serde_json::to_string(env).unwrap_or_default(),
        });

        let list = self.state["invites"].as_array().cloned().unwrap_or_default();
        if list
            .iter()
            .any(|i| i.get("inviteId").and_then(|v| v.as_str()) == Some(env.invite_id.as_str()))
        {
            return Ok(());
        }
        let mut list = list;
        list.push(row);
        self.state["invites"] = Value::Array(list);
        self.save_state()
    }

    /// Accept an invite: unseal the key, keep it, and start syncing.
    pub fn accept_invite_by_id(&mut self, invite_id: &str) -> Result<String, String> {
        let raw = self
            .state
            .get("invites")
            .and_then(|v| v.as_array())
            .and_then(|a| {
                a.iter()
                    .find(|i| i.get("inviteId").and_then(|v| v.as_str()) == Some(invite_id))
            })
            .and_then(|i| i.get("envelope"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("no invite {invite_id}"))?
            .to_string();

        let env: olb_keys::invite::InviteEnvelope =
            serde_json::from_str(&raw).map_err(|e| e.to_string())?;

        let keys = olb_keys::invite::accept_invite(&self.identity, &env)
            .map_err(|e| e.to_string())?;

        for (scope, _version, key) in &keys {
            if matches!(scope, KeyScope::ProjectMeta) {
                self.adopt_project_key(&env.project_id, key)?;
            }
        }

        // The project exists for us now, even though we hold no records for
        // it yet — those arrive over sync.
        let mut projects = self.state["projects"].as_array().cloned().unwrap_or_default();
        if !projects
            .iter()
            .any(|p| p.get("id").and_then(|v| v.as_str()) == Some(env.project_id.as_str()))
        {
            let name = self
                .state
                .get("invites")
                .and_then(|v| v.as_array())
                .and_then(|a| {
                    a.iter()
                        .find(|i| i.get("inviteId").and_then(|v| v.as_str()) == Some(invite_id))
                })
                .and_then(|i| i.get("projectName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Shared project")
                .to_string();
            projects.push(json!({
                "id": env.project_id,
                "title": name,
                "privacy": "Invite-only",
                "stages": ["Idea", "Hypothesis", "Funding", "Experiments", "Data", "Publish"],
                "ideas": [],
                "joined": true,
            }));
            self.state["projects"] = Value::Array(projects);
            self.state["current"] = Value::String(env.project_id.clone());
        }

        // Drop it from the pending list; the key is what matters now.
        if let Some(a) = self.state["invites"].as_array() {
            let kept: Vec<Value> = a
                .iter()
                .filter(|i| i.get("inviteId").and_then(|v| v.as_str()) != Some(invite_id))
                .cloned()
                .collect();
            self.state["invites"] = Value::Array(kept);
        }
        self.save_state()?;
        Ok(env.project_id)
    }
}
