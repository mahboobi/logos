//! Open Labbook core module.
//!
//! Rust-FIRST authoring: the trait below IS the contract. The builder derives
//! the `.lidl` from it (`codegen.rust.trait` in metadata.json), generates the
//! C-ABI scaffold around it, and compiles this crate to a staticlib — there is
//! no committed `.lidl` and no `build.rs`.
//!
//! LIDL maps these types one-to-one: `String`, `i64`, `u64`, `f64`, `bool`,
//! `Vec<u8>`, `Option<T>`. There is no struct/record type, so OLB records
//! cross the IPC seam as JSON strings (docs/08 R10 — open decision). Every
//! record shape is built in `record::` so switching to typed records later is
//! one module to change, not a scatter of inline json! calls.

// NOTE: no `use std::sync::Mutex` here — provider_gen.rs is include!()d into
// this module and already imports it; a second import is E0252.

/// Content topic Phase 0 records travel on. Delivery's convention is
/// `/<app>/<version>/<name>/<encoding>`.
const RECORD_TOPIC: &str = "/open-labbook/1/records/json";

/// Delivery node config. `mode: Core` is a relay node, `preset: logos.test` is
/// the testnet; `entryLayer` defaults to `channels` (kernel + messaging +
/// reliable channels). Verbatim from the delivery_module.lidl createNode docs.
const DELIVERY_CFG: &str = r#"{"mode":"Core","preset":"logos.test"}"#;

/// Storage upload/download chunk size in bytes (the contract's default).
const STORAGE_CHUNK: i64 = 65536;

/// Trait name is not free: it is the metadata.json module name in PascalCase
/// with `Module` appended — `olb_core` -> `OlbCoreModule`. `Send + 'static` is
/// required because the generated dispatch stores the impl as
/// `Box<dyn Any + Send>`.
pub trait OlbCoreModule: Send + 'static {
    /// Phase 0 task 3: the session snapshot `olb_ui` renders, as a JSON string.
    fn session_status(&mut self) -> String;

    /// Phase 0 task 4: sign a test record and publish it on the record topic.
    /// Returns the signed record as JSON, or a `{"ok":false,...}` envelope.
    fn publish_test_record(&mut self, title: String) -> String;

    /// Every record this node has received, newest first, as a JSON array.
    fn received_records(&mut self) -> String;

    /// This node's signing-key fingerprint — the Phase 0 stand-in for identity.
    fn identity_fingerprint(&mut self) -> String;

    /// Phase 0 task 5: upload a local file to Logos Storage by absolute path.
    /// Asynchronous — returns the session id; the CID arrives on `file_stored`.
    fn upload_file(&mut self, abs_path: String) -> String;

    /// Fetch a stored file by CID and write it to `abs_dest`. Asynchronous —
    /// completion arrives on `file_fetched`.
    fn fetch_file(&mut self, cid: String, abs_dest: String) -> String;

    /// CIDs this node has uploaded or fetched, as a JSON array.
    fn stored_cids(&mut self) -> String;

    /// Phase 1a: the project board (docs/03 S1), as a JSON object.
    /// Serves fixture data while the record store does not exist yet; the
    /// shape is the contract either way.
    fn board(&mut self) -> String;

    /// Whether this module is serving fixtures rather than live data.
    fn fixtures_enabled(&mut self) -> bool;

    /// Create a project from a JSON spec (docs/03 S8: basics, field and team,
    /// stages, privacy, anchoring, references). Produces a signed commit.
    fn create_project(&mut self, spec_json: String) -> String;

    /// Choose which project the board shows. Returns the project JSON.
    fn set_current_project(&mut self, project_id: String) -> String;

    /// Personal notebook: a private note, not part of any project.
    fn create_note(&mut self, title: String, body: String) -> String;

    /// Every personal note, newest first, as a JSON array.
    fn notes(&mut self) -> String;

    /// Promote a personal note into an idea on a project, entering at the
    /// named stage ("Idea", "Hypothesis", "Experiments" or "Data").
    fn promote_note(&mut self, note_id: String, project_id: String, stage: String) -> String;

    /// The local profile (name, orcid, fingerprint) as JSON.
    fn profile(&mut self) -> String;

    /// Update the local profile. Produces a signed commit.
    fn set_profile(&mut self, name: String, orcid: String) -> String;

    /// Projects this node knows about, as a JSON array.
    fn projects(&mut self) -> String;

    /// Called once the module is wired to the host. `ctx` carries the
    /// host-stamped instance id and persistence path.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

/// Typed events — the Rust analog of the C++ `logos_events:` section. Each
/// method becomes an `emit_<name>` free function.
pub trait OlbCoreModuleEvents {
    fn session_changed(&self, status: String);
    /// Fires on every verified record that arrives from a peer.
    fn record_received(&self, record_json: String);
    /// Fires when an upload completes; payload carries `cid` and `sessionId`.
    fn file_stored(&self, result_json: String);
    /// Fires when a download completes; payload carries `sessionId`.
    fn file_fetched(&self, result_json: String);
    /// Fires whenever the local store changes (project, note, profile).
    fn store_changed(&self, summary_json: String);
}

// The builder injects the generated scaffold here — the `install` entry point,
// `RustModuleContext`, `context()`, `modules()` and the `emit_*` emitters.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

// ---------------------------------------------------------------------------
// Record shapes. One place, so docs/08 R10 is a localised change.
// ---------------------------------------------------------------------------
mod record {
    use serde_json::{json, Value};

    /// The signed envelope. `payload` is signed; `sig` and `author` are not.
    pub fn signed(author_hex: &str, payload: Value, sig_hex: &str) -> Value {
        json!({
            "v": 1,
            "author": author_hex,
            "payload": payload,
            "sig": sig_hex,
        })
    }

    pub fn payload(title: &str, author_hex: &str, created_at: i64) -> Value {
        json!({
            "kind": "test_record",
            "title": title,
            "author": author_hex,
            "createdAt": created_at,
        })
    }

    pub fn err(code: &str, message: &str) -> String {
        json!({"ok": false, "error": {"code": code, "message": message}}).to_string()
    }
}

// ---------------------------------------------------------------------------
// Shared state. The Delivery listener runs on its own thread and cannot touch
// the module impl (the host keeps it single-threaded), so received records and
// the dedup set live in statics.
// ---------------------------------------------------------------------------
static RECEIVED: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
static SEEN_HASHES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NODE_READY: Mutex<bool> = Mutex::new(false);
static STORAGE_READY: Mutex<bool> = Mutex::new(false);
static CIDS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
/// Published so the bring-up thread can build a status snapshot without the impl.
static IDENTITY_HEX: Mutex<String> = Mutex::new(String::new());

struct OlbCore {
    signing: ed25519_dalek::SigningKey,
}

impl Default for OlbCore {
    fn default() -> Self {
        // Phase 0: an ephemeral per-launch key. Durable identity needs a store
        // that survives `lgs basecamp launch`, which scrubs the profile tree —
        // that is task 7 / docs/08 R6, deliberately not solved here.
        use rand::RngCore;
        let mut seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        OlbCore { signing: ed25519_dalek::SigningKey::from_bytes(&seed) }
    }
}

impl OlbCore {
    /// Append a signed commit. CLAUDE.md: every user-visible action that
    /// changes data produces one, and history is never rewritten.
    fn commit(&mut self, st: &mut serde_json::Value, action: &str, payload: serde_json::Value) {
        use ed25519_dalek::Signer;
        let parent = st
            .get("commits")
            .and_then(|c| c.as_array())
            .and_then(|a| a.last())
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let body = serde_json::json!({
            "action": action,
            "payload": payload,
            "parent": parent,
            "at": now_ms(),
            "author": self.author_hex(),
        });
        let bytes = serde_json::to_vec(&body).unwrap_or_default();
        let sig = self.signing.sign(&bytes);
        // The commit id is the hash of the signed body, so the DAG is
        // content-addressed the way docs/05 expects.
        let id = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes));

        let entry = serde_json::json!({
            "id": id,
            "body": body,
            "sig": hex::encode(sig.to_bytes()),
        });
        if let Some(a) = st["commits"].as_array_mut() {
            a.push(entry);
        }
    }

    fn author_hex(&self) -> String {
        hex::encode(self.signing.verifying_key().to_bytes())
    }
}

/// The fixture document, compiled in. Phase 1a builds every screen against
/// fixtures/sample-project.json before wiring live data (CLAUDE.md UI rules).
/// Live data replaces the body of each accessor, not its shape.
const FIXTURES: &str = include_str!("../fixtures/sample-project.json");

/// Fixture mode is OFF by default now that the store is real; set
/// OLB_FIXTURES=1 to render the sample project instead (useful for comparing
/// a screen against design/canvas without creating data).
fn fixtures_on() -> bool {
    matches!(std::env::var("OLB_FIXTURES").as_deref(), Ok("1"))
}


// ---------------------------------------------------------------------------
// Durable store (docs/08 R6).
//
// `lgs basecamp launch` scrubs the whole profile tree on EVERY launch,
// including module_data/, so instance_persistence_path() cannot hold anything
// that must outlive a relaunch. The store therefore lives outside that tree,
// keyed by LOGOS_PROFILE so alice and bob never share state.
//
// This is deliberately a plain JSON file, not SQLite: Phase 0 needs
// durability and an auditable commit log, not query power. Phase 1b can
// migrate the same shapes into SQLite without changing the module API.
// ---------------------------------------------------------------------------
mod store {
    use serde_json::{json, Value};
    use std::path::PathBuf;

    pub fn dir() -> PathBuf {
        let profile = std::env::var("LOGOS_PROFILE").unwrap_or_else(|_| "default".into());
        let base = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        // Outside .scaffold/basecamp/profiles/<p>/ on purpose — that tree is
        // what launch deletes.
        PathBuf::from(base)
            .join("Library/Application Support/OpenLabbook")
            .join(profile)
    }

    pub fn path() -> PathBuf {
        dir().join("store.json")
    }

    pub fn load() -> Value {
        match std::fs::read_to_string(path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| empty()),
            Err(_) => empty(),
        }
    }

    pub fn save(v: &Value) -> Result<(), String> {
        std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
        // Write-then-rename so a crash mid-write cannot truncate the store.
        let tmp = path().with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path()).map_err(|e| e.to_string())
    }

    pub fn empty() -> Value {
        json!({
            "v": 1,
            "profile": {"name": "", "orcid": ""},
            "projects": [],
            "notes": [],
            "commits": [],
        })
    }
}

/// Stage names in the order the board uses them.
const STAGES: [&str; 6] = ["Idea", "Hypothesis", "Funding", "Experiments", "Data", "Publish"];

/// 1-based stage index for a stage name, accepting the aliases the UI offers
/// ("Experiment", "Data/Results").
fn stage_index(name: &str) -> Option<usize> {
    let n = name.trim().to_ascii_lowercase();
    let n = match n.as_str() {
        "experiment" => "experiments",
        "data/results" | "results" => "data",
        other => other,
    };
    STAGES
        .iter()
        .position(|s| s.to_ascii_lowercase() == n)
        .map(|i| i + 1)
}

fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{n:x}")
}

/// Storage events carry a single `payload: tstr` holding JSON — unwrap the
/// event array's first element, then parse it.
fn storage_payload(ev: &logos_rust_sdk::EventData) -> Option<serde_json::Value> {
    let arr = ev.data.as_array()?;
    let raw = arr.first()?.as_str()?;
    serde_json::from_str(raw).ok()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Verify a received envelope against its embedded author key. Returns the
/// parsed record only when the signature checks out.
fn verify(bytes: &[u8]) -> Option<serde_json::Value> {
    use ed25519_dalek::Verifier;

    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let author = v.get("author")?.as_str()?;
    let sig_hex = v.get("sig")?.as_str()?;
    let payload = v.get("payload")?;

    let key_bytes: [u8; 32] = hex::decode(author).ok()?.try_into().ok()?;
    let sig_bytes: [u8; 64] = hex::decode(sig_hex).ok()?.try_into().ok()?;

    let key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes).ok()?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    // Sign over the canonical serialization of the payload only.
    let signed_bytes = serde_json::to_vec(payload).ok()?;
    key.verify(&signed_bytes, &sig).ok()?;
    Some(v)
}

/// The session snapshot, shared by `session_status()` and the
/// `session_changed` event so the two can never disagree.
fn session_json(identity: &str) -> String {
    serde_json::json!({
        "module":   "olb_core",
        "version":  env!("CARGO_PKG_VERSION"),
        "state":    if *NODE_READY.lock().unwrap() { "online" } else { "offline" },
        "identity": identity,
        "topic":    RECORD_TOPIC,
        "projects": 0,
        "records":  RECEIVED.lock().unwrap().len(),
        "storage":  if *STORAGE_READY.lock().unwrap() { "online" } else { "offline" },
        "cids":     CIDS.lock().unwrap().len(),
    })
    .to_string()
}

/// Announce that the durable store changed, so every view refreshes.
fn announce_store() {
    let st = store::load();
    let summary = serde_json::json!({
        "projects": st.get("projects").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        "notes":    st.get("notes").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
        "commits":  st.get("commits").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
    });
    emit_store_changed(&summary.to_string());
}

/// Announce the current session state. Without this the view keeps whatever it
/// read during onContextReady — which is BEFORE the deferred bring-up finishes,
/// so delivery and storage show "offline" forever even after they come up.
fn announce_session() {
    let identity = IDENTITY_HEX.lock().unwrap().clone();
    emit_session_changed(&session_json(&identity));
}

impl OlbCoreModule for OlbCore {
    fn session_status(&mut self) -> String {
        session_json(&self.author_hex())
    }

    fn identity_fingerprint(&mut self) -> String {
        self.author_hex()
    }

    fn publish_test_record(&mut self, title: String) -> String {
        use ed25519_dalek::Signer;

        let author = self.author_hex();
        let payload = record::payload(&title, &author, now_ms());

        let signed_bytes = match serde_json::to_vec(&payload) {
            Ok(b) => b,
            Err(e) => return record::err("internal", &e.to_string()),
        };
        let sig = self.signing.sign(&signed_bytes);
        let envelope = record::signed(&author, payload, &hex::encode(sig.to_bytes()));

        let wire = envelope.to_string();
        match modules().delivery_module.send(RECORD_TOPIC, wire.as_bytes()) {
            Ok(_) => wire,
            Err(e) => record::err("offline", &format!("delivery send failed: {e:?}")),
        }
    }

    fn received_records(&mut self) -> String {
        let recs = RECEIVED.lock().unwrap();
        serde_json::Value::Array(recs.iter().rev().cloned().collect()).to_string()
    }

    fn upload_file(&mut self, abs_path: String) -> String {
        if !std::path::Path::new(&abs_path).is_absolute() {
            return record::err("invalid", "upload_file needs an absolute path");
        }
        if !std::path::Path::new(&abs_path).exists() {
            return record::err("not_found", &format!("no such file: {abs_path}"));
        }
        // uploadUrl is async: this returns a session id, and the CID arrives
        // on storageUploadDone.
        match modules().storage_module.upload_url(&abs_path, STORAGE_CHUNK) {
            Ok(v) => serde_json::json!({"ok": true, "data": {"session": v}}).to_string(),
            Err(e) => record::err("storage_error", &format!("{e:?}")),
        }
    }

    fn fetch_file(&mut self, cid: String, abs_dest: String) -> String {
        if !std::path::Path::new(&abs_dest).is_absolute() {
            return record::err("invalid", "fetch_file needs an absolute destination");
        }
        // local=false so it will go to the network when the block is not cached.
        match modules()
            .storage_module
            .download_to_url(&cid, &abs_dest, false, STORAGE_CHUNK)
        {
            Ok(v) => serde_json::json!({"ok": true, "data": {"session": v}}).to_string(),
            Err(e) => record::err("storage_error", &format!("{e:?}")),
        }
    }

    fn projects(&mut self) -> String {
        store::load()
            .get("projects")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]))
            .to_string()
    }

    fn notes(&mut self) -> String {
        let st = store::load();
        let mut notes = st
            .get("notes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        notes.reverse(); // newest first
        serde_json::Value::Array(notes).to_string()
    }

    fn profile(&mut self) -> String {
        let st = store::load();
        let p = st.get("profile").cloned().unwrap_or_else(|| serde_json::json!({}));
        serde_json::json!({
            "name":        p.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "orcid":       p.get("orcid").and_then(|v| v.as_str()).unwrap_or(""),
            "fingerprint": self.author_hex(),
            "profileDir":  store::dir().to_string_lossy(),
        })
        .to_string()
    }

    fn set_profile(&mut self, name: String, orcid: String) -> String {
        let mut st = store::load();
        st["profile"] = serde_json::json!({"name": name, "orcid": orcid});
        self.commit(&mut st, "profile.update", serde_json::json!({"name": name, "orcid": orcid}));
        match store::save(&st) {
            Ok(()) => {
                announce_store();
                self.profile()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn set_current_project(&mut self, project_id: String) -> String {
        let mut st = store::load();
        let exists = st
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().any(|p| p.get("id").and_then(|i| i.as_str()) == Some(&project_id)))
            .unwrap_or(false);
        if !exists {
            return record::err("not_found", &format!("no project {project_id}"));
        }
        st["current"] = serde_json::Value::String(project_id);
        match store::save(&st) {
            Ok(()) => {
                announce_store();
                st["current"].to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn create_project(&mut self, spec_json: String) -> String {
        let spec: serde_json::Value =
            serde_json::from_str(&spec_json).unwrap_or_else(|_| serde_json::json!({}));
        let title = spec.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if title.is_empty() {
            return record::err("invalid", "a project needs a title");
        }
        let str_of = |k: &str| spec.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Stages the project uses. Skipped stages are hidden from the board
        // and can be turned on later (docs/03 S8 §3), so the board must read
        // this rather than assume all six.
        let stages = spec
            .get("stages")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_else(|| STAGES.iter().map(|s| serde_json::json!(s)).collect());

        let mut st = store::load();
        let project = serde_json::json!({
            "id": new_id("proj"),
            "title": title,
            "summary": str_of("summary"),
            "description": str_of("description"),
            "startDate": str_of("startDate"),
            "duration": str_of("duration"),
            "licence": str_of("licence"),
            "field": str_of("field"),
            "keywords": spec.get("keywords").cloned().unwrap_or_else(|| serde_json::json!([])),
            "teamSize": str_of("teamSize"),
            "team": spec.get("team").cloned().unwrap_or_else(|| serde_json::json!([])),
            "stages": stages,
            // Privacy cannot change after creation (docs/03 S8 §4), so it is
            // written once here and never updated.
            "privacy": if str_of("privacy").is_empty() { "Private".to_string() } else { str_of("privacy") },
            "anchoring": str_of("anchoring"),
            "approvals": str_of("approvals"),
            "replica": str_of("replica"),
            "allowBranches": spec.get("allowBranches").and_then(|v| v.as_bool()).unwrap_or(true),
            "autoTag": spec.get("autoTag").and_then(|v| v.as_bool()).unwrap_or(true),
            "references": spec.get("references").cloned().unwrap_or_else(|| serde_json::json!([])),
            "grant": str_of("grant"),
            "createdAt": now_ms(),
            "author": self.author_hex(),
            "ideas": [],
        });
        st["projects"]
            .as_array_mut()
            .map(|a| a.push(project.clone()));
        st["current"] = project["id"].clone();
        self.commit(&mut st, "project.create", project.clone());
        match store::save(&st) {
            Ok(()) => {
                announce_store();
                project.to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn create_note(&mut self, title: String, body: String) -> String {
        if title.trim().is_empty() {
            return record::err("invalid", "a note needs a title");
        }
        let mut st = store::load();
        let note = serde_json::json!({
            "id": new_id("note"),
            "title": title.trim(),
            "body": body,
            "createdAt": now_ms(),
            "author": self.author_hex(),
            "promoted": serde_json::Value::Null,
        });
        st["notes"].as_array_mut().map(|a| a.push(note.clone()));
        self.commit(&mut st, "note.create", note.clone());
        match store::save(&st) {
            Ok(()) => {
                announce_store();
                note.to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn promote_note(&mut self, note_id: String, project_id: String, stage: String) -> String {
        let Some(stage_no) = stage_index(&stage) else {
            return record::err(
                "invalid",
                &format!("unknown stage '{stage}'; expected one of {STAGES:?}"),
            );
        };
        let mut st = store::load();

        // The note is kept, not consumed: records are append-only, so
        // promotion marks the note and adds an idea rather than moving it.
        let note = st
            .get("notes")
            .and_then(|v| v.as_array())
            .and_then(|a| a.iter().find(|n| n.get("id").and_then(|i| i.as_str()) == Some(&note_id)))
            .cloned();
        let Some(note) = note else {
            return record::err("not_found", &format!("no note {note_id}"));
        };

        let idea = serde_json::json!({
            "id": new_id("idea"),
            "stage": stage_no,
            "published": false,
            "title": note.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "summary": note.get("body").and_then(|v| v.as_str()).unwrap_or(""),
            "privacy": "Private · project group",
            "facts": [
                {"k": "Hypothesis", "v": "Not yet"},
                {"k": "Funding",    "v": "Not started"},
                {"k": "Notebook",   "v": "1 entry"},
                {"k": "Data",       "v": "None yet"},
            ],
            "branch": "main",
            "headCommit": "",
            "headMessage": format!("Promoted from personal note at stage {}", STAGES[stage_no - 1]),
            "commitSummary": "1 commit",
            "lead": st.pointer("/profile/name").and_then(|v| v.as_str()).unwrap_or("You"),
            "people": "1 person",
            "updated": "just now",
            "open": note.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "fromNote": note_id,
        });

        let mut attached = false;
        if let Some(projects) = st["projects"].as_array_mut() {
            for p in projects.iter_mut() {
                if p.get("id").and_then(|i| i.as_str()) == Some(&project_id) {
                    p["ideas"].as_array_mut().map(|a| a.push(idea.clone()));
                    attached = true;
                    break;
                }
            }
        }
        if !attached {
            return record::err("not_found", &format!("no project {project_id}"));
        }

        if let Some(notes) = st["notes"].as_array_mut() {
            for n in notes.iter_mut() {
                if n.get("id").and_then(|i| i.as_str()) == Some(&note_id) {
                    n["promoted"] = serde_json::json!({
                        "projectId": project_id, "stage": STAGES[stage_no - 1],
                    });
                }
            }
        }

        self.commit(&mut st, "note.promote", idea.clone());
        match store::save(&st) {
            Ok(()) => {
                announce_store();
                idea.to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn fixtures_enabled(&mut self) -> bool {
        fixtures_on()
    }

    fn board(&mut self) -> String {
        if fixtures_on() {
            return match serde_json::from_str::<serde_json::Value>(FIXTURES) {
                Ok(v) => v
                    .get("board")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}))
                    .to_string(),
                Err(e) => record::err("internal", &format!("fixture parse failed: {e}")),
            };
        }

        // Live board, built from the durable store. Same shape as the
        // fixture so the view does not change (docs/03 S1).
        let st = store::load();
        let projects = st.get("projects").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let commits = st.get("commits").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);

        // The selected project, falling back to the most recent.
        let current_id = st.get("current").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let current = projects
            .iter()
            .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(current_id.as_str()))
            .cloned()
            .or_else(|| projects.last().cloned());
        let ideas = current
            .as_ref()
            .and_then(|p| p.get("ideas"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let published = ideas
            .iter()
            .filter(|i| i.get("published").and_then(|v| v.as_bool()) == Some(true))
            .count();

        let title = current
            .as_ref()
            .and_then(|p| p.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("No project yet")
            .to_string();
        let privacy = current
            .as_ref()
            .and_then(|p| p.get("privacy"))
            .and_then(|v| v.as_str())
            .unwrap_or("Private · project group")
            .to_string();

        let node_ready = *NODE_READY.lock().unwrap();
        let storage_ready = *STORAGE_READY.lock().unwrap();
        let cids = CIDS.lock().unwrap().len();
        let received = RECEIVED.lock().unwrap().len();

        serde_json::json!({
            "projectId": current.as_ref().and_then(|p| p.get("id")).cloned(),
            "title": title,
            "badges": [
                {"kind": "neutral", "text": privacy},
                {"kind": "neutral", "text": format!("{} idea{}", ideas.len(), if ideas.len() == 1 { "" } else { "s" })},
                {"kind": "done",    "text": format!("{published} published")},
            ],
            "status": {
                "sync": {
                    "label": "Live sync",
                    "value": if node_ready { "Delivery online" } else { "Offline" },
                    "sub": if node_ready {
                        "Edits arrive over Logos Delivery, end-to-end encrypted"
                    } else {
                        "Changes are saved locally"
                    },
                },
                "copies": {
                    "label": "Copies of project files",
                    "value": format!("{cids} file{} in Logos Storage", if cids == 1 { "" } else { "s" }),
                    "warning": if storage_ready { "" } else { "Storage is offline" },
                },
                "history": {
                    "label": "Version history",
                    "value": format!("{commits} signed commit{}", if commits == 1 { "" } else { "s" }),
                    "sub": format!("{received} record{} received from peers", if received == 1 { "" } else { "s" }),
                },
            },
            // The project's own stage list — a project may skip stages.
            "stagesUsed": current
                .as_ref()
                .and_then(|p| p.get("stages"))
                .cloned()
                .unwrap_or_else(|| STAGES.iter().map(|s| serde_json::json!(s)).collect()),
            // Every project, for the sidebar list.
            "projects": projects.iter().map(|p| serde_json::json!({
                "id": p.get("id").cloned().unwrap_or(serde_json::Value::Null),
                "title": p.get("title").cloned().unwrap_or(serde_json::Value::Null),
                "current": p.get("id").and_then(|i| i.as_str()) == Some(current_id.as_str()),
            })).collect::<Vec<_>>(),
            "ideas": ideas,
            "activity": st.get("commits").and_then(|v| v.as_array()).map(|a| {
                a.iter().rev().take(8).map(|c| serde_json::json!({
                    "text": format!("{} · {}",
                        c.pointer("/body/action").and_then(|v| v.as_str()).unwrap_or(""),
                        c.get("id").and_then(|v| v.as_str()).unwrap_or("").chars().take(8).collect::<String>()),
                    "link": "",
                    "time": "",
                })).collect::<Vec<_>>()
            }).unwrap_or_default(),
        })
        .to_string()
    }

    fn stored_cids(&mut self) -> String {
        let cids = CIDS.lock().unwrap();
        serde_json::Value::Array(cids.iter().rev().cloned().collect()).to_string()
    }

    fn on_context_ready(&mut self, ctx: &RustModuleContext) {
        // Do NOT call dependencies synchronously here. onContextReady fires a
        // few hundred milliseconds BEFORE the per-caller token handshake
        // completes (logos-module-builder docs/configuration.md), so an
        // immediate call presents a token the provider has not registered yet
        // and comes back {"code":"unauthorized","message":"token not
        // recognized (re-exchange failed)"}. The SDK's own single re-exchange
        // retry also fires too early to help. Bring-up runs off-thread with
        // backoff instead.
        *IDENTITY_HEX.lock().unwrap() = self.author_hex();
        let persistence = ctx.instance_persistence_path.clone();
        std::thread::spawn(move || bring_up(&persistence));
    }
}

/// Retry a dependency call until the per-caller token handshake has settled.
fn retry_call<T>(
    label: &str,
    mut f: impl FnMut() -> Result<T, logos_rust_sdk::LogosError>,
) -> Option<T> {
    let mut delay = std::time::Duration::from_millis(250);
    for attempt in 1..=8 {
        match f() {
            Ok(v) => {
                if attempt > 1 {
                    eprintln!("[olb_core] {label} succeeded on attempt {attempt}");
                }
                return Some(v);
            }
            Err(e) => {
                eprintln!("[olb_core] {label} attempt {attempt} failed: {e:?}");
                std::thread::sleep(delay);
                delay = (delay * 2).min(std::time::Duration::from_secs(4));
            }
        }
    }
    eprintln!("[olb_core] {label} gave up after 8 attempts");
    None
}

/// Bring up delivery and storage. Independent: a failure in one must not take
/// the other down with it.
fn bring_up(persistence_path: &str) {
    // ---- Delivery (task 4) ----
    let delivery_up = retry_call("delivery create_node", || {
        modules().delivery_module.create_node(DELIVERY_CFG)
    })
    .is_some()
        && retry_call("delivery start", || modules().delivery_module.start()).is_some();

    if delivery_up {
        let mut m = modules();
        if let Ok(started) = m.delivery_module.on_node_started() {
            std::thread::spawn(move || {
                for ev in started {
                    if let Some(e) = delivery_module::DeliveryModuleClient::decode_node_started(&ev)
                    {
                        *NODE_READY.lock().unwrap() = e.success;
                        announce_session();
                        if e.success {
                            retry_call("delivery subscribe", || {
                                modules().delivery_module.subscribe(RECORD_TOPIC)
                            });
                        } else {
                            eprintln!("[olb_core] delivery node failed to start: {}", e.message);
                        }
                    }
                }
            });
        }

        let mut m2 = modules();
        if let Ok(messages) = m2.delivery_module.on_message_received() {
            std::thread::spawn(move || {
                for ev in messages {
                    let Some(msg) =
                        delivery_module::DeliveryModuleClient::decode_message_received(&ev)
                    else {
                        continue;
                    };
                    if msg.content_topic != RECORD_TOPIC {
                        continue;
                    }
                    // Store catch-up replays history and upstream suppresses
                    // duplicates only briefly and in memory, so the contract
                    // requires dedup by messageHash.
                    {
                        let mut seen = SEEN_HASHES.lock().unwrap();
                        if seen.contains(&msg.message_hash) {
                            continue;
                        }
                        seen.push(msg.message_hash.clone());
                    }
                    // Unsigned or tampered records are dropped, not shown.
                    let Some(rec) = verify(&msg.payload) else {
                        eprintln!("[olb_core] dropped record with bad signature");
                        continue;
                    };
                    RECEIVED.lock().unwrap().push(rec.clone());
                    emit_record_received(&rec.to_string());
                    announce_session();
                }
            });
        }
    } else {
        eprintln!("[olb_core] continuing without delivery; storage still attempted");
    }

    // ---- Storage (task 5) ----
    // data-dir must be absolute and per-instance: the contract's default is a
    // RELATIVE ".cache/storage", which would collide between alice and bob.
    // Every port must be ephemeral, not just listen-port. The contract's
    // defaults are fixed (disc-port 8090, metrics-port 8008), so a second
    // profile binds the same discovery port as the first and storage_module
    // dies with SIGSEGV — seen exactly once alice held 8090 and bob started.
    // Two profiles on one machine is the normal dogfooding setup, so all of
    // these have to be 0.
    let storage_cfg = serde_json::json!({
        "data-dir": format!("{persistence_path}/storage"),
        "network": "logos.test",
        "listen-port": 0,
        "disc-port": 0,
        "metrics": false,
        "metrics-port": 0,
    })
    .to_string();

    let storage_up =
        retry_call("storage init", || modules().storage_module.init(&storage_cfg)).is_some()
            && retry_call("storage start", || modules().storage_module.start()).is_some();
    *STORAGE_READY.lock().unwrap() = storage_up;
    announce_session();

    if storage_up {
        let mut ms = modules();
        if let Ok(uploads) = ms.storage_module.on_storage_upload_done() {
            std::thread::spawn(move || {
                for ev in uploads {
                    // One JSON string: {success, sessionId, cid?, error?}
                    let Some(p) = storage_payload(&ev) else { continue };
                    if p.get("success").and_then(|v| v.as_bool()) == Some(true) {
                        if let Some(cid) = p.get("cid").and_then(|v| v.as_str()) {
                            CIDS.lock()
                                .unwrap()
                                .push(serde_json::json!({"cid": cid, "direction": "upload"}));
                        }
                    }
                    emit_file_stored(&p.to_string());
                }
            });
        }

        let mut md = modules();
        if let Ok(downloads) = md.storage_module.on_storage_download_done() {
            std::thread::spawn(move || {
                for ev in downloads {
                    let Some(p) = storage_payload(&ev) else { continue };
                    if p.get("success").and_then(|v| v.as_bool()) == Some(true) {
                        if let Some(sid) = p.get("sessionId").and_then(|v| v.as_str()) {
                            // sessionId == CID for downloadToUrl.
                            CIDS.lock()
                                .unwrap()
                                .push(serde_json::json!({"cid": sid, "direction": "download"}));
                        }
                    }
                    emit_file_fetched(&p.to_string());
                }
            });
        }
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<OlbCore>();
}
