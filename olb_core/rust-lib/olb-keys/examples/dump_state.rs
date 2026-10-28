// Read-only inspector. Decrypts a profile's state, commits and records and
// prints a summary. It NEVER writes, and it never touches the keychain — the
// key comes in as hex on the command line, so there is no path by which this
// can mint a replacement store key. (That is how a store key was destroyed on
// 27 Oct: a probe took the mint-on-any-read-error branch in keyring.rs.)
//
//   cargo run --example dump_state -- <dir> <64-hex-key>
use olb_keys::vault;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: dump_state <store-dir> <64-hex-key>");
        std::process::exit(2);
    }
    let dir = std::path::PathBuf::from(&args[1]);
    let raw = hex::decode(args[2].trim()).expect("key is not hex");
    let key: [u8; 32] = raw.try_into().expect("key must be 32 bytes");

    match vault::read_sealed(&dir.join("state.json"), &key, "olb.store.state.v1") {
        Ok(Some(b)) => {
            let v: serde_json::Value = serde_json::from_slice(&b).expect("state is not json");
            let projects = v.get("projects").and_then(|p| p.as_array());
            println!("state.json: v={} current={}",
                v.get("v").map(|x| x.to_string()).unwrap_or_default(),
                v.get("current").map(|x| x.to_string()).unwrap_or_default());
            println!("projects: {}", projects.map_or(0, |a| a.len()));
            if let Some(a) = projects {
                for p in a {
                    println!("  - {} | {} | createdAt={} | privacy={}",
                        p.get("id").and_then(|x| x.as_str()).unwrap_or("?"),
                        p.get("title").and_then(|x| x.as_str()).unwrap_or("?"),
                        p.get("createdAt").map(|x| x.to_string()).unwrap_or_default(),
                        p.get("privacy").and_then(|x| x.as_str()).unwrap_or("?"));
                }
            }
            println!("notes: {}", v.get("notes").and_then(|n| n.as_array()).map_or(0, |a| a.len()));
        }
        Ok(None) => println!("state.json: absent"),
        Err(e) => println!("state.json: UNREADABLE with this key: {e}"),
    }

    // Commits. `message` and `author` live on the header; the signature is
    // over the same preimage the id hashes.
    match vault::read_lines(&dir.join("commits.jsonl"), &key, "olb.store.commits.v1") {
        Ok(lines) => {
            println!("commits: {}", lines.len());
            for l in &lines {
                let v: serde_json::Value = serde_json::from_slice(l).unwrap_or_default();
                let s = |ptr: &str| {
                    v.pointer(ptr).and_then(|x| x.as_str()).unwrap_or("?").to_string()
                };
                let id = s("/id");
                println!(
                    "  - {} | {} | project={} author={}..",
                    &id[..id.len().min(12)],
                    s("/header/message"),
                    s("/header/project"),
                    &s("/header/author")[..8.min(s("/header/author").len())],
                );
            }
        }
        Err(e) => println!("commits: UNREADABLE: {e}"),
    }

    // Records. `kind` serialises as "type".
    match vault::read_lines(&dir.join("records.jsonl"), &key, "olb.store.records.v1") {
        Ok(lines) => {
            println!("records: {}", lines.len());
            for l in &lines {
                let v: serde_json::Value = serde_json::from_slice(l).unwrap_or_default();
                let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("?").to_string();
                println!(
                    "  - type={} v={} project={} title={}",
                    s("type"),
                    v.get("version").map(|x| x.to_string()).unwrap_or_default(),
                    s("project"),
                    v.pointer("/fields/title")
                        .and_then(|x| x.as_str())
                        .unwrap_or("-"),
                );
            }
        }
        Err(e) => println!("records: UNREADABLE: {e}"),
    }

    // Identity last: the store can be emptied deliberately, but an identity
    // that will not open blocks the next launch entirely, and it is the one
    // file here that cannot be regenerated — a node that changes its author
    // key orphans every commit it ever signed.
    match vault::read_sealed(&dir.join("identity.olbk"), &key, "olb.identity.vault.v1") {
        Ok(Some(b)) => {
            let v: serde_json::Value = serde_json::from_slice(&b).unwrap_or_default();
            println!(
                "identity: OPENS  name={:?} orcid={:?}",
                v.get("name").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("orcid").and_then(|x| x.as_str()).unwrap_or("-"),
            );
        }
        Ok(None) => println!("identity: absent (a new one will be minted on next open)"),
        Err(e) => println!("identity: UNREADABLE with this key: {e}"),
    }
}
