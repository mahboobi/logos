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

/// Domain tag for the Phase 0 test record. Separate from the commit and
/// record tags so a test-record signature can never be replayed as either.
const RECORD_DOMAIN: &[u8] = b"olb.testrecord.v1\0";

/// Delivery node config. `mode: Core` is a relay node, `preset: logos.test` is
/// the testnet; `entryLayer` defaults to `channels` (kernel + messaging +
/// reliable channels). Verbatim from the delivery_module.lidl createNode docs.
const DELIVERY_CFG: &str = r#"{"mode":"Core","preset":"logos.test"}"#;

/// Storage upload/download chunk size in bytes (the contract's default).
const STORAGE_CHUNK: i64 = 65536;

/// Usable payload budget for one Delivery `send`.
///
/// logos.test sets maxMessageSize to 150 KiB and the limit is applied to the
/// ENCODED WakuMessage, not the payload, so the usable budget is lower. An
/// oversize publish is dropped inside GossipSub and surfaces as
/// NoPeersToPublish — identical to having no peers — so we must refuse it
/// here rather than discover it as a phantom connectivity problem
/// (docs/09-sync-design.md).
const SEND_BUDGET: usize = 144 * 1024;

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

    /// S6 History. Fixture-backed until the commit DAG lands (docs/10).
    fn history(&mut self) -> String;

    /// S7 Members. Fixture-backed until membership records land (docs/11).
    fn members(&mut self) -> String;

    /// One record by id, for S2. Fixture-backed until the record store lands.
    fn record(&mut self, record_id: String) -> String;

    /// The `olb1:` string a user copies and hands to someone else
    /// (docs/04 §2.1). Public half only — the private keys never leave
    /// this module.
    fn my_olb_id(&mut self) -> String;

    /// Invite someone to a project by their Open Labbook ID. Seals this
    /// project's metadata key to them and publishes it to their inbox.
    fn invite_member(&mut self, project_id: String, olb_id: String, role: String) -> String;

    /// Invites that have arrived and are waiting on a decision.
    fn invites(&mut self) -> String;

    /// Accept one. Installs the key and starts syncing the project.
    fn accept_invite(&mut self, invite_id: String) -> String;

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

mod app;
use app::App;
// `has_commit` and friends live on the trait, not the concrete store.
use olb_vcs::store::CommitStore as _;

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
/// Dedup window. Was an unbounded Vec scanned with contains(): O(n) on every
/// inbound message and it never shrank. A set plus a bounded FIFO keeps the
/// lookup O(1) and the memory flat. The DAG is the real dedup authority
/// (docs/09) — this only suppresses the immediate replay storm, so a window
/// is sufficient.
// BTreeSet, not HashSet: HashSet::new() is not const (RandomState), so it
// cannot initialise a static. Ordered lookup is O(log n), still far better
// than the linear Vec scan it replaces.
static SEEN_HASHES: Mutex<std::collections::BTreeSet<String>> =
    Mutex::new(std::collections::BTreeSet::new());
static SEEN_ORDER: Mutex<std::collections::VecDeque<String>> =
    Mutex::new(std::collections::VecDeque::new());
const SEEN_WINDOW: usize = 4096;
static NODE_READY: Mutex<bool> = Mutex::new(false);
static STORAGE_READY: Mutex<bool> = Mutex::new(false);
static CIDS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
/// Published so the bring-up thread can build a status snapshot without the impl.
static IDENTITY_HEX: Mutex<String> = Mutex::new(String::new());

/// The open store, shared.
///
/// Behind a lock rather than owned by `OlbCore` because the Delivery
/// listener runs on its own thread and has to apply incoming commits to the
/// same store the UI methods write to. Two independently opened handles to
/// the same files would append safely but their in-memory indexes and their
/// `refs.json` writes would race.
static APP: Mutex<Option<App>> = Mutex::new(None);
/// Why the store is not open, when it is not.
static LOCKED: Mutex<Option<String>> = Mutex::new(None);

/// Borrowed access to the open store. Derefs to `App`, so call sites read
/// the same as they did when the struct owned it.
struct AppGuard(std::sync::MutexGuard<'static, Option<App>>);

impl std::ops::Deref for AppGuard {
    type Target = App;
    fn deref(&self) -> &App {
        self.0.as_ref().expect("an AppGuard is only built when Some")
    }
}
impl std::ops::DerefMut for AppGuard {
    fn deref_mut(&mut self) -> &mut App {
        self.0.as_mut().expect("an AppGuard is only built when Some")
    }
}

/// Lock the store, from any thread.
fn app_guard() -> Result<AppGuard, String> {
    // A poisoned lock means some earlier holder panicked. What it guards is
    // still structurally sound — every mutation goes through a method that
    // either completes or returns an error — so recovering beats bricking
    // the module for the rest of the session.
    let g = APP.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        Ok(AppGuard(g))
    } else {
        Err(LOCKED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| "the store is not open".into()))
    }
}

struct OlbCore;

impl Default for OlbCore {
    fn default() -> Self {
        let profile = std::env::var("LOGOS_PROFILE").unwrap_or_else(|_| "default".into());
        match App::open(&profile) {
            Ok(app) => *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app),
            Err(l) => {
                eprintln!("[olb_core] store locked: {}", l.reason);
                *LOCKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(l.reason);
            }
        }
        OlbCore
    }
}

impl OlbCore {
    fn app(&mut self) -> Result<AppGuard, String> {
        app_guard()
    }

    fn author_hex(&self) -> String {
        app_guard().map(|a| a.author_hex()).unwrap_or_default()
    }

    /// The derived view the screens read. Empty rather than an error when
    /// the store is locked, so a screen renders its empty state instead of
    /// a stack trace.
    fn state_doc(&self) -> serde_json::Value {
        app_guard()
            .ok()
            .map(|a| a.state.clone())
            .unwrap_or_else(|| serde_json::json!({
                "v": 2,
                "profile": {"name": "", "orcid": ""},
                "projects": [],
                "notes": [],
                "current": serde_json::Value::Null,
            }))
    }

    fn save_doc(&mut self, st: &serde_json::Value) -> Result<(), String> {
        let mut app = self.app()?;
        app.state = st.clone();
        app.save_state()
    }

    /// Write a record of `kind` on `project` and the signed commit that
    /// introduces it. Replaces Phase 0's `{action, payload}` entries, which
    /// were a log rather than a history: nothing could be verified, merged
    /// or anchored from them.
    fn commit_action(
        &mut self,
        kind: &str,
        project: &str,
        fields: serde_json::Value,
        message: &str,
    ) -> Result<String, String> {
        let mut app = self.app()?;
        let rec = app.new_record(kind, project, fields)?;
        app.commit_record(&rec, olb_vcs::types::ChangeKind::Create, message)
    }

    /// The project a new record belongs to: the selected one, else the most
    /// recent. Personal notes use the reserved id below.
    fn current_project(&self) -> String {
        app_guard()
            .ok()
            .and_then(|a| a.state.get("current").and_then(|v| v.as_str()).map(String::from))
            .unwrap_or_default()
    }
}

/// Personal notes are not part of any project, but a commit header requires
/// one. They get a reserved project id so their DAG stays separate from
/// every real project's.
const PERSONAL: &str = "olb:proj:personal";

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


/// Pull one top-level section out of the compiled-in fixture document.
fn fixture_part(key: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(FIXTURES)
        .ok()
        .and_then(|v| v.get(key).cloned())
        .unwrap_or_else(|| serde_json::json!({}))
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

    // Must match publish_test_record exactly: canonical bytes under the
    // same domain tag. Verifying with a different serializer than the one
    // that signed is how two honest nodes reject each other's records.
    let signed_bytes = olb_vcs::hashing::preimage(RECORD_DOMAIN, payload).ok()?;
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
fn announce_store(st: &serde_json::Value) {
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

        // Canonical, domain-separated bytes — NOT serde_json::to_vec.
        // Phase 0 signed and verified with the plain serializer, which only
        // agreed between two nodes because both happened to emit the same
        // key order. Any crate enabling serde_json's preserve_order would
        // have flipped that, and every peer's signature check would start
        // failing for reasons no error message would explain.
        let signed_bytes = match olb_vcs::hashing::preimage(RECORD_DOMAIN, &payload) {
            Ok(b) => b,
            Err(e) => return record::err("internal", &e.to_string()),
        };
        let sig = match self.app() {
            Ok(app) => app.identity.signing.sign(&signed_bytes),
            Err(e) => return record::err("locked", &e),
        };
        let envelope = record::signed(&author, payload, &hex::encode(sig.to_bytes()));

        let wire = envelope.to_string();
        if wire.len() > SEND_BUDGET {
            return record::err(
                "invalid",
                &format!(
                    "record is {} bytes, over the {SEND_BUDGET}-byte Delivery budget; \
                     large content belongs in Storage with only its CID in the record",
                    wire.len()
                ),
            );
        }
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
        self.state_doc()
            .get("projects")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]))
            .to_string()
    }

    fn notes(&mut self) -> String {
        let st = self.state_doc();
        let mut notes = st
            .get("notes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        notes.reverse(); // newest first
        serde_json::Value::Array(notes).to_string()
    }

    fn profile(&mut self) -> String {
        let st = self.state_doc();
        let p = st.get("profile").cloned().unwrap_or_else(|| serde_json::json!({}));
        serde_json::json!({
            "name":        p.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            "orcid":       p.get("orcid").and_then(|v| v.as_str()).unwrap_or(""),
            "fingerprint": self.author_hex(),
            "profileDir":  app::profile_dir().to_string_lossy(),
        })
        .to_string()
    }

    fn set_profile(&mut self, name: String, orcid: String) -> String {
        let mut st = self.state_doc();
        st["profile"] = serde_json::json!({"name": name, "orcid": orcid});
        if let Err(e) = self.commit_action(
            "member",
            PERSONAL,
            serde_json::json!({"name": name, "orcid": orcid}),
            "Update profile",
        ) {
            return record::err("internal", &e);
        }
        match self.save_doc(&st) {
            Ok(()) => {
                announce_store(&st);
                self.profile()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn set_current_project(&mut self, project_id: String) -> String {
        let mut st = self.state_doc();
        let exists = st
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().any(|p| p.get("id").and_then(|i| i.as_str()) == Some(&project_id)))
            .unwrap_or(false);
        if !exists {
            return record::err("not_found", &format!("no project {project_id}"));
        }
        st["current"] = serde_json::Value::String(project_id);
        match self.save_doc(&st) {
            Ok(()) => {
                announce_store(&st);
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

        let mut st = self.state_doc();
        let project = serde_json::json!({
            "id": app::new_project_id(),
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
        let pid = project["id"].as_str().unwrap_or_default().to_string();
        // The project's metadata key and salt, minted once. Without them
        // there is no conversation to join and no frame to seal, so this
        // has to happen before the first commit is advertised.
        if let Ok(app) = self.app() {
            if let Err(e) = app.create_project_secret(&pid) {
                return record::err("internal", &e);
            }
        }
        if let Err(e) = self.commit_action(
            "project",
            &pid,
            project.clone(),
            &format!("Create project: {title}"),
        ) {
            return record::err("internal", &e);
        }
        // Subscribe to the bucket this project landed in, then advertise.
        let topic = olb_vcs::sync::project_topic(&pid);
        std::thread::spawn(move || {
            retry_call("delivery subscribe new project", || {
                modules().delivery_module.subscribe(&topic)
            });
            publish_heads(&pid);
        });
        match self.save_doc(&st) {
            Ok(()) => {
                announce_store(&st);
                project.to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn create_note(&mut self, title: String, body: String) -> String {
        if title.trim().is_empty() {
            return record::err("invalid", "a note needs a title");
        }
        let mut st = self.state_doc();
        let note = serde_json::json!({
            "id": new_id("note"),
            "title": title.trim(),
            "body": body,
            "createdAt": now_ms(),
            "author": self.author_hex(),
            "promoted": serde_json::Value::Null,
        });
        st["notes"].as_array_mut().map(|a| a.push(note.clone()));
        let note_title = note["title"].as_str().unwrap_or_default().to_string();
        if let Err(e) = self.commit_action(
            "entry",
            PERSONAL,
            note.clone(),
            &format!("Note: {note_title}"),
        ) {
            return record::err("internal", &e);
        }
        match self.save_doc(&st) {
            Ok(()) => {
                announce_store(&st);
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
        let mut st = self.state_doc();

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

        if let Err(e) = self.commit_action(
            "idea",
            &project_id,
            idea.clone(),
            &format!("Promote note to {}", STAGES[stage_no - 1]),
        ) {
            return record::err("internal", &e);
        }
        let pid = project_id.clone();
        std::thread::spawn(move || publish_heads(&pid));
        match self.save_doc(&st) {
            Ok(()) => {
                announce_store(&st);
                idea.to_string()
            }
            Err(e) => record::err("internal", &e),
        }
    }

    fn history(&mut self) -> String {
        // Per project, not global. A commit header carries its project and
        // the refs are namespaced per project, so this is a branch walk
        // rather than a filter over everything the node has ever done.
        let project = self.current_project();
        if project.is_empty() {
            return serde_json::json!({
                "commits": [], "branches": [], "tags": [],
                "empty": "No project selected",
            })
            .to_string();
        }
        let Ok(app) = self.app() else {
            return record::err("locked", "the store is not open");
        };
        let mut h = app.history(&project, 200);

        // The diff and merge-request shapes S6 draws are not wired yet, so
        // those sections still come from the fixture. The commit graph,
        // lanes and anchor marks above are real.
        if let Some(obj) = h.as_object_mut() {
            let fx = fixture_part("history");
            for k in ["mergeRequest", "diff"] {
                if let Some(v) = fx.get(k) {
                    obj.insert(k.into(), v.clone());
                }
            }
        }
        h.to_string()
    }

    fn members(&mut self) -> String {
        fixture_part("members").to_string()
    }

    fn record(&mut self, record_id: String) -> String {
        let all = fixture_part("records");
        all.get(&record_id)
            .cloned()
            .map(|mut v| {
                // The fixture keys records by id but does not carry the id
                // inside the record; S2 needs it to emit openHistory/openFile.
                if let Some(o) = v.as_object_mut() {
                    o.insert("id".into(), serde_json::Value::String(record_id.clone()));
                }
                v.to_string()
            })
            .unwrap_or_else(|| record::err("not_found", &format!("no record {record_id}")))
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
        let st = self.state_doc();
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

    fn my_olb_id(&mut self) -> String {
        match self.app() {
            Ok(app) => serde_json::json!({
                "ok": true,
                "id": app.my_olb_id(),
                "fingerprint": app.my_fingerprint(),
            })
            .to_string(),
            Err(e) => record::err("locked", &e),
        }
    }

    fn invite_member(&mut self, project_id: String, olb_id: String, role: String) -> String {
        let role = match role.to_ascii_lowercase().as_str() {
            "lead" => olb_keys::invite::Role::Lead,
            "reviewer" => olb_keys::invite::Role::Reviewer,
            "replica" => olb_keys::invite::Role::Replica,
            _ => olb_keys::invite::Role::Contributor,
        };
        let built = match self.app() {
            Ok(app) => app.build_invite_for(&project_id, olb_id.trim(), role),
            Err(e) => return record::err("locked", &e),
        };
        let (env, topic) = match built {
            Ok(v) => v,
            Err(e) => return record::err("invalid", &e),
        };
        let bytes = match serde_json::to_vec(&env) {
            Ok(b) => b,
            Err(e) => return record::err("internal", &e.to_string()),
        };
        if bytes.len() > SEND_BUDGET {
            return record::err("invalid", "the invite is too large for one Delivery message");
        }
        match modules().delivery_module.send(&topic, &bytes) {
            Ok(_) => serde_json::json!({
                "ok": true,
                "inviteId": env.invite_id,
                "expiry": env.expiry,
            })
            .to_string(),
            Err(e) => record::err("offline", &format!("could not send the invite: {e:?}")),
        }
    }

    fn invites(&mut self) -> String {
        let st = self.state_doc();
        // The sealed envelope stays here. It carries a project key sealed to
        // this node, and nothing sealed belongs on the other side of the IPC
        // seam even when only this node can open it.
        let rows: Vec<serde_json::Value> = st
            .get("invites")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut i| {
                if let Some(o) = i.as_object_mut() {
                    o.remove("envelope");
                }
                i
            })
            .collect();
        serde_json::Value::Array(rows).to_string()
    }

    fn accept_invite(&mut self, invite_id: String) -> String {
        let joined = match self.app() {
            Ok(mut app) => app.accept_invite_by_id(&invite_id),
            Err(e) => return record::err("locked", &e),
        };
        match joined {
            Ok(project_id) => {
                let topic = olb_vcs::sync::project_topic(&project_id);
                let p = project_id.clone();
                std::thread::spawn(move || {
                    retry_call("delivery subscribe joined project", || {
                        modules().delivery_module.subscribe(&topic)
                    });
                    publish_heads(&p);
                });
                let st = self.state_doc();
                announce_store(&st);
                serde_json::json!({"ok": true, "projectId": project_id}).to_string()
            }
            Err(e) => record::err("invalid", &e),
        }
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

/// Send a sealed frame on a project's bucket topic.
fn send_frame(project_id: &str, msg: &olb_vcs::sync::SyncMessage) {
    let Ok(app) = app_guard() else { return };
    let topic = olb_vcs::sync::project_topic(project_id);
    match app.seal_frame(project_id, msg) {
        Ok(bytes) => {
            if bytes.len() > SEND_BUDGET {
                eprintln!(
                    "[olb_core] refusing a {}-byte sync frame, over the {SEND_BUDGET} budget",
                    bytes.len()
                );
                return;
            }
            drop(app);
            if let Err(e) = modules().delivery_module.send(&topic, &bytes) {
                eprintln!("[olb_core] sync send failed: {e:?}");
            }
        }
        Err(e) => eprintln!("[olb_core] could not seal a sync frame: {e}"),
    }
}

/// Tell peers where this node's branches are. Called after every local
/// commit and once on connect.
fn publish_heads(project_id: &str) {
    let msg = match app_guard() {
        Ok(app) => app.heads_message(project_id),
        Err(_) => return,
    };
    send_frame(project_id, &msg);
}

/// Handle one decrypted frame.
fn handle_sync(project_id: &str, msg: olb_vcs::sync::SyncMessage) {
    use olb_vcs::sync::{react_to_heads, Reaction, SyncMessage};

    match msg {
        SyncMessage::Heads { .. } => {
            let (ours, reaction) = {
                let Ok(app) = app_guard() else { return };
                let ours = app.heads(project_id);
                let known = |id: &str| app.store.has_commit(id);
                let r = react_to_heads(&ours, None, &msg, &known, olb_vcs::sync::MAX_WALK);
                (ours, r)
            };
            match reaction {
                Ok(Reaction::Want(ids)) => send_frame(
                    project_id,
                    &SyncMessage::Want {
                        project: project_id.to_string(),
                        ids,
                    },
                ),
                Ok(Reaction::Offer(_)) => {
                    // The peer is behind. Re-advertise rather than pushing:
                    // they decide what they still need, and a push would
                    // duplicate whatever another member already sent.
                    let _ = ours;
                    publish_heads(project_id);
                }
                Ok(Reaction::AnchorDivergence { ours, theirs }) => {
                    // Not a transient error to retry past. A peer whose
                    // anchor replaces rather than extends our history is
                    // either broken or hostile, and syncing from it would
                    // launder a rewritten history into this store.
                    eprintln!(
                        "[olb_core] REFUSING to sync {project_id}: the peer's anchor does not \
                         extend ours (ours {ours}, theirs {theirs})"
                    );
                }
                Ok(Reaction::NeedSnapshot) => {
                    eprintln!("[olb_core] {project_id} is too far behind to catch up by walking; \
                               a snapshot is needed (not implemented yet)");
                }
                Ok(Reaction::InSync) => {}
                Err(e) => eprintln!("[olb_core] sync reaction failed: {e}"),
            }
        }

        SyncMessage::Want { ids, .. } => {
            let packed = match app_guard() {
                Ok(app) => app.commits_for(project_id, &ids),
                Err(_) => return,
            };
            match packed {
                Ok(msgs) => {
                    for m in &msgs {
                        send_frame(project_id, m);
                    }
                }
                Err(e) => eprintln!("[olb_core] could not pack commits: {e}"),
            }
        }

        SyncMessage::Have { commits, .. } => {
            let applied = match app_guard() {
                Ok(mut app) => app.apply_incoming(&commits),
                Err(_) => return,
            };
            if applied > 0 {
                eprintln!("[olb_core] applied {applied} commit(s) from a peer");
                let st = app_guard().map(|a| a.state.clone()).unwrap_or_default();
                announce_store(&st);
                announce_session();
            }
        }

        SyncMessage::Snapshot { cid, .. } => {
            eprintln!("[olb_core] a peer offered a snapshot at {cid}; not implemented yet");
        }
    }
}

/// Retry a dependency call until it succeeds or the deadline passes.
///
/// Two different waits are folded into one budget here. The short one is the
/// per-caller token handshake, which settles in well under a second. The
/// long one is `delivery.start`, which on a cold start has been measured at
/// **90 seconds** on this machine — the module logs
/// `RET_STALE_WARN elapsedMs=90000` while libp2p discovers peers and RLN
/// answers its membership check.
///
/// The previous version allowed 8 attempts with the backoff capped at 4 s,
/// about 20 seconds in total. It only appeared to work because subscription
/// happens from the `on_node_started` event rather than from this call's
/// return — so a timeout here left the node coming up anyway, unobserved.
/// A deadline states the real budget instead of encoding it as an attempt
/// count that happens to be survivable.
fn retry_until(
    label: &str,
    budget: std::time::Duration,
    mut f: impl FnMut() -> Result<(), logos_rust_sdk::LogosError>,
) -> bool {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = std::time::Duration::from_millis(250);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f() {
            Ok(()) => {
                if attempt > 1 {
                    eprintln!("[olb_core] {label} succeeded on attempt {attempt}");
                }
                return true;
            }
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    eprintln!(
                        "[olb_core] {label} gave up after {attempt} attempts \
                         ({:?} budget): {e:?}",
                        budget
                    );
                    return false;
                }
                eprintln!("[olb_core] {label} attempt {attempt} failed: {e:?}");
                std::thread::sleep(delay);
                delay = (delay * 2).min(std::time::Duration::from_secs(8));
            }
        }
    }
}

/// Short budget: a call that only has to outlast the token handshake.
const HANDSHAKE_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
/// Long budget: a call that has to outlast a cold network start.
const STARTUP_BUDGET: std::time::Duration = std::time::Duration::from_secs(180);

/// Retry a dependency call that returns a value.
fn retry_call<T>(
    label: &str,
    mut f: impl FnMut() -> Result<T, logos_rust_sdk::LogosError>,
) -> Option<T> {
    let mut out = None;
    let ok = retry_until(label, HANDSHAKE_BUDGET, || match f() {
        Ok(v) => {
            out = Some(v);
            Ok(())
        }
        Err(e) => Err(e),
    });
    if ok {
        out
    } else {
        None
    }
}

/// Bring up delivery and storage. Independent: a failure in one must not take
/// the other down with it.
fn bring_up(persistence_path: &str) {
    // ---- Delivery (task 4) ----
    let delivery_up = retry_call("delivery create_node", || {
        modules().delivery_module.create_node(DELIVERY_CFG)
    })
    .is_some()
        && retry_until("delivery start", STARTUP_BUDGET, || {
            modules().delivery_module.start().map(|_| ())
        });

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
                            // One topic per bucket, shared with whatever
                            // other projects hash into it; frames are
                            // sealed, so the others are noise we skip.
                            let topics: Vec<String> = app_guard()
                                .map(|a| {
                                    a.synced_projects()
                                        .into_iter()
                                        .map(|(_, _, t)| t)
                                        .collect()
                                })
                                .unwrap_or_default();
                            for t in topics.iter().collect::<std::collections::BTreeSet<_>>() {
                                retry_call("delivery subscribe project", || {
                                    modules().delivery_module.subscribe(t)
                                });
                            }
                            if let Ok(a) = app_guard() {
                                let inbox = a.my_inbox_topic();
                                drop(a);
                                retry_call("delivery subscribe inbox", || {
                                    modules().delivery_module.subscribe(&inbox)
                                });
                            }
                            let projects: Vec<String> = app_guard()
                                .map(|a| {
                                    a.synced_projects().into_iter().map(|(id, _, _)| id).collect()
                                })
                                .unwrap_or_default();
                            for p in projects {
                                publish_heads(&p);
                            }
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
                        // A project sync frame, or another project's traffic
                        // on the same bucket. open_frame returns None for
                        // anything we hold no key for, which is most of it.
                        let opened = app_guard().ok().and_then(|a| a.open_frame(&msg.payload));
                        if let Some((project_id, sync_msg)) = opened {
                            handle_sync(&project_id, sync_msg);
                            continue;
                        }
                        // Not ours as a sync frame. It may be an invite on
                        // our inbox bucket; most traffic there belongs to
                        // other people, so a failure to parse is ordinary.
                        if let Ok(env) = serde_json::from_slice::<olb_keys::invite::InviteEnvelope>(
                            &msg.payload,
                        ) {
                            let recorded = app_guard()
                                .ok()
                                .map(|mut a| a.record_incoming_invite(&env));
                            if let Some(Ok(())) = recorded {
                                eprintln!("[olb_core] an invite arrived: {}", env.invite_id);
                                let st = app_guard().map(|a| a.state.clone()).unwrap_or_default();
                                announce_store(&st);
                            }
                        }
                        continue;
                    }
                    // Store catch-up replays history and upstream suppresses
                    // duplicates only briefly and in memory, so the contract
                    // requires dedup by messageHash.
                    {
                        let mut seen = SEEN_HASHES.lock().unwrap();
                        if !seen.insert(msg.message_hash.clone()) {
                            continue;
                        }
                        let mut order = SEEN_ORDER.lock().unwrap();
                        order.push_back(msg.message_hash.clone());
                        while order.len() > SEEN_WINDOW {
                            if let Some(old) = order.pop_front() {
                                seen.remove(&old);
                            }
                        }
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
