// Print raw decrypted records/commits as JSON lines, for schema comparison.
// Read-only; key comes in as hex, so it cannot touch the keychain.
use olb_keys::vault;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 4 {
        eprintln!("usage: dump_raw <dir> <64-hex-key> <records|commits>");
        std::process::exit(2);
    }
    let dir = std::path::PathBuf::from(&a[1]);
    let raw = hex::decode(a[2].trim()).expect("key is not hex");
    let key: [u8; 32] = raw.try_into().expect("key must be 32 bytes");
    let (file, aad) = match a[3].as_str() {
        "records" => ("records.jsonl", "olb.store.records.v1"),
        "commits" => ("commits.jsonl", "olb.store.commits.v1"),
        other => { eprintln!("unknown: {other}"); std::process::exit(2); }
    };
    for line in vault::read_lines(&dir.join(file), &key, aad).expect("read failed") {
        println!("{}", String::from_utf8_lossy(&line));
    }
}
