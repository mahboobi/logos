//! Read a profile's encrypted store and print what is actually in it.
//!
//! The store is sealed, so "is my project saved?" cannot be answered by
//! looking at the files. This answers it without going through the UI.
//!
//!   cargo run --example dump_store -- <profile>

use olb_vcs::store::CommitStore;

fn main() {
    let profile = std::env::args().nth(1).unwrap_or_else(|| "alice".into());
    // Read-only on purpose. This tool must NEVER create a key: a probe that
    // mints one overwrites the key an existing store is encrypted with, and
    // that is exactly how this example destroyed a store once.
    let key = match olb_keys::keyring::peek_key(&profile) {
        Ok(Some(k)) => k,
        Ok(None) => { println!("no store key for '{profile}' — nothing to read"); return; }
        Err(e) => { println!("keychain read FAILED (not creating one): {e}"); return; }
    };
    let dir = olb_keys::store::default_dir(&profile);
    println!("dir: {}", dir.display());

    let store = match olb_keys::store::DurableStore::open(&dir, *key) {
        Ok(s) => s,
        Err(e) => { println!("store FAILED to open: {e}"); return; }
    };

    println!("commits: {}", store.commit_count());
    for (name, t) in store.refs() {
        println!("  ref {name} -> {t:?}");
    }
    match store.read_state() {
        Ok(Some(v)) => {
            let n = |k: &str| v.get(k).and_then(|x| x.as_array()).map_or(0, |a| a.len());
            println!("state: projects={} notes={} invites={}", n("projects"), n("notes"), n("invites"));
            println!("current: {}", v.get("current").cloned().unwrap_or_default());
            if let Some(ps) = v.get("projects").and_then(|x| x.as_array()) {
                for p in ps {
                    println!("  project {} :: {}",
                        p.get("id").and_then(|x| x.as_str()).unwrap_or("?"),
                        p.get("title").and_then(|x| x.as_str()).unwrap_or("?"));
                }
            }
        }
        Ok(None) => println!("state: NONE (no state.json readable)"),
        Err(e) => println!("state FAILED: {e}"),
    }
    match store.read_secrets() {
        Ok(Some(v)) => println!("secrets: {} project key(s)", v.as_object().map_or(0, |m| m.len())),
        Ok(None) => println!("secrets: none"),
        Err(e) => println!("secrets FAILED: {e}"),
    }
}
