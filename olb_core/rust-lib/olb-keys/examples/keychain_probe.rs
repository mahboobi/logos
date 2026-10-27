//! Probe the real Keychain path end to end, outside Basecamp.
//!
//! `olb_core` opens the store during module install, which is synchronous —
//! if the Keychain blocks there, the module never becomes ready and the app
//! shows nothing with no error. Worth being able to check that in isolation
//! rather than discovering it through a blank window.
//!
//!   cargo run --example keychain_probe -- <profile>

fn main() {
    let profile = std::env::args().nth(1).unwrap_or_else(|| "probe".into());
    let t0 = std::time::Instant::now();

    match olb_keys::keyring::store_key(&profile) {
        Ok(k) => {
            println!("keychain ok in {:?} (key {} bytes)", t0.elapsed(), k.len());
            let dir = std::env::temp_dir().join(format!("olb-probe-{profile}"));
            match olb_keys::store::DurableStore::open(&dir, *k) {
                Ok(s) => println!("store ok at {:?}, {} commits", s.dir(), s.commit_count()),
                Err(e) => println!("store FAILED: {e}"),
            }
        }
        Err(e) => println!("keychain FAILED in {:?}: {e}", t0.elapsed()),
    }
}
