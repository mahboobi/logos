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
    fn author_hex(&self) -> String {
        hex::encode(self.signing.verifying_key().to_bytes())
    }
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

impl OlbCoreModule for OlbCore {
    fn session_status(&mut self) -> String {
        let node_ready = *NODE_READY.lock().unwrap();
        let received = RECEIVED.lock().unwrap().len();
        serde_json::json!({
            "module":    "olb_core",
            "version":   env!("CARGO_PKG_VERSION"),
            "state":     if node_ready { "online" } else { "offline" },
            "identity":  self.author_hex(),
            "topic":     RECORD_TOPIC,
            "projects":  0,
            "records":   received,
            "storage":   if *STORAGE_READY.lock().unwrap() { "online" } else { "offline" },
            "cids":      CIDS.lock().unwrap().len(),
        })
        .to_string()
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
                }
            });
        }
    } else {
        eprintln!("[olb_core] continuing without delivery; storage still attempted");
    }

    // ---- Storage (task 5) ----
    // data-dir must be absolute and per-instance: the contract's default is a
    // RELATIVE ".cache/storage", which would collide between alice and bob.
    let storage_cfg = serde_json::json!({
        "data-dir": format!("{persistence_path}/storage"),
        "network": "logos.test",
        "listen-port": 0,
    })
    .to_string();

    let storage_up =
        retry_call("storage init", || modules().storage_module.init(&storage_cfg)).is_some()
            && retry_call("storage start", || modules().storage_module.start()).is_some();
    *STORAGE_READY.lock().unwrap() = storage_up;

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
