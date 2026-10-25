//! Open Labbook core module.
//!
//! Rust-FIRST authoring: the trait below IS the contract. The builder derives
//! the `.lidl` from it (`codegen.rust.trait` in metadata.json), generates the
//! C-ABI scaffold around it, and compiles this crate to a staticlib — there is
//! no committed `.lidl` and no `build.rs`.
//!
//! LIDL maps these types one-to-one: `String`, `i64`, `u64`, `f64`, `bool`,
//! `Vec<u8>`, `Option<T>`. There is no struct/record type, so OLB records
//! cross the IPC seam as JSON strings (see docs/08 R10).

/// Trait name is not free: it is the metadata.json module name in PascalCase
/// with `Module` appended — `olb_core` -> `OlbCoreModule`. `Send + 'static` is
/// required because the generated dispatch stores the impl as
/// `Box<dyn Any + Send>`.
pub trait OlbCoreModule: Send + 'static {
    /// Phase 0 task 3: the session snapshot `olb_ui` renders, as a JSON string.
    fn session_status(&mut self) -> String;

    /// Called once the module is wired to the host. `ctx` carries the
    /// host-stamped instance id and persistence path.
    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

/// Typed events — the Rust analog of the C++ `logos_events:` section. Each
/// method becomes an `emit_<name>` free function; subscribers use
/// `modules().olb_core.on_session_changed(...)`.
pub trait OlbCoreModuleEvents {
    fn session_changed(&self, status: String);
}

// The builder injects the generated scaffold here — the `install` entry point,
// `RustModuleContext`, `context()`, `modules()` and the `emit_*` emitters.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/provider_gen.rs"));

#[derive(Default)]
struct OlbCore;

impl OlbCoreModule for OlbCore {
    fn session_status(&mut self) -> String {
        // Phase 0 skeleton: a fixed shape so the UI seam can be proven before
        // identity and the record store exist. Fields become live in Phase 1b.
        serde_json::json!({
            "module":      "olb_core",
            "version":     env!("CARGO_PKG_VERSION"),
            "state":       "ready",
            "identity":    serde_json::Value::Null,
            "projects":    0,
            "peers":       0,
        })
        .to_string()
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<OlbCore>();
}
