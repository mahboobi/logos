//! Encrypted file I/O: sealed blobs and a sealed append-only log.
//!
//! XChaCha20-Poly1305, chosen over the 12-byte-nonce variant for one reason:
//! the nonce is 24 bytes, so random nonces are safe. With a 96-bit nonce,
//! birthday collisions become a real concern after ~2^32 messages under one
//! key, and a commit log is exactly the kind of thing that appends
//! indefinitely. A counter would avoid that too, but a counter has to be
//! persisted and kept in step with the file — one crash at the wrong moment
//! and it repeats. Random 192-bit nonces have neither problem.
//!
//! Each record carries an AAD string naming what it is (`"commits"`,
//! `"state"`). That stops a sealed `state` blob from being moved into the
//! commit log and still decrypting: the AEAD tag covers the label, so a
//! record pasted into the wrong file fails authentication.

use crate::{KeyError, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use std::io::{BufRead, Write};
use std::path::Path;

const NONCE_LEN: usize = 24;

/// Seal `plaintext` as `nonce || ciphertext`.
pub fn seal(key: &[u8; 32], aad: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|e| KeyError::Decrypt(format!("seal: {e}")))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open a `nonce || ciphertext` blob.
///
/// A failure here cannot distinguish "wrong key" from "tampered with" — that
/// is what AEAD gives you and pretending otherwise would be a lie. Callers
/// should surface it as "this store cannot be opened", never as corruption.
pub fn open(key: &[u8; 32], aad: &str, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < NONCE_LEN + 16 {
        return Err(KeyError::Malformed(format!(
            "sealed blob is {} bytes, too short to hold a nonce and a tag",
            blob.len()
        )));
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(
            XNonce::from_slice(nonce_bytes),
            Payload {
                msg: ct,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| {
            KeyError::Decrypt(format!(
                "authentication failed for '{aad}' — wrong key, or the file was modified"
            ))
        })
}

/// Write a sealed blob to a file, atomically.
///
/// Temp-then-rename, so a crash mid-write leaves the previous version intact
/// rather than a truncated file that will never decrypt.
pub fn write_sealed(path: &Path, key: &[u8; 32], aad: &str, plaintext: &[u8]) -> Result<()> {
    let blob = seal(key, aad, plaintext)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| KeyError::Io(e.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| KeyError::Io(e.to_string()))?;
        f.write_all(&blob).map_err(|e| KeyError::Io(e.to_string()))?;
        f.sync_all().map_err(|e| KeyError::Io(e.to_string()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| KeyError::Io(e.to_string()))
}

/// Read a sealed file. A missing file is `Ok(None)`, not an error: an empty
/// store is an ordinary first-run state.
pub fn read_sealed(path: &Path, key: &[u8; 32], aad: &str) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(blob) => Ok(Some(open(key, aad, &blob)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(KeyError::Io(e.to_string())),
    }
}

/// Append one sealed record to a log file, as a single base64 line.
///
/// O(1) per append, which is the whole reason the commit log is a log and
/// not a document rewritten in full on every commit. `fsync` before
/// returning: a commit the user has been told was saved must survive a power
/// cut, and an unflushed append is the easiest way to lose one.
pub fn append_line(path: &Path, key: &[u8; 32], aad: &str, plaintext: &[u8]) -> Result<()> {
    let blob = seal(key, aad, plaintext)?;
    let line = data_encoding::BASE64.encode(&blob);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| KeyError::Io(e.to_string()))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| KeyError::Io(e.to_string()))?;
    f.write_all(line.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .map_err(|e| KeyError::Io(e.to_string()))?;
    f.sync_all().map_err(|e| KeyError::Io(e.to_string()))
}

/// Read every record from a sealed log.
///
/// A crash during `append_line` leaves a partial final line. That is the ONLY
/// damage tolerated here, and it is detected precisely: a complete log always
/// ends with a newline, so if the file does, every line in it is whole and any
/// failure to decode or authenticate one is real damage, not a torn write.
///
/// Getting this wrong is worse than it sounds. An earlier version treated any
/// failure on the last line as a torn write, which meant opening a one-record
/// log with the WRONG KEY silently produced an empty store — and the caller
/// would then start appending records under the new key on top of records it
/// could not read. A loud error is the only safe answer.
pub fn read_lines(path: &Path, key: &[u8; 32], aad: &str) -> Result<Vec<Vec<u8>>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(KeyError::Io(e.to_string())),
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let ends_cleanly = bytes.ends_with(b"\n");

    let reader = std::io::BufReader::new(&bytes[..]);
    let mut out = Vec::new();
    let mut lines = reader.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.map_err(|e| KeyError::Io(e.to_string()))?;
        if line.is_empty() {
            continue;
        }
        // Only an unterminated final line may be dropped.
        let torn = !ends_cleanly && lines.peek().is_none();

        let decoded = match data_encoding::BASE64.decode(line.as_bytes()) {
            Ok(d) => d,
            Err(e) if torn => {
                eprintln!("[olb] dropping torn final log record in {path:?}: {e}");
                break;
            }
            Err(e) => return Err(KeyError::Malformed(format!("log line is not base64: {e}"))),
        };
        match open(key, aad, &decoded) {
            Ok(p) => out.push(p),
            Err(e) if torn => {
                eprintln!("[olb] dropping torn final log record in {path:?}: {e}");
                break;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];

    fn tmpdir() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "olb-vault-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn seal_then_open_round_trips() {
        let b = seal(&KEY, "commits", b"hello").unwrap();
        assert_eq!(open(&KEY, "commits", &b).unwrap(), b"hello");
    }

    #[test]
    fn the_same_plaintext_seals_differently_every_time() {
        // Random nonces. Identical ciphertexts would leak that two commits
        // carry the same bytes.
        let a = seal(&KEY, "commits", b"same").unwrap();
        let b = seal(&KEY, "commits", b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_wrong_key_fails_authentication() {
        let b = seal(&KEY, "commits", b"hello").unwrap();
        assert!(open(&[8u8; 32], "commits", &b).is_err());
    }

    #[test]
    fn a_record_cannot_be_moved_between_logs() {
        // This is what the AAD buys: a sealed `state` blob dropped into the
        // commit log does not decrypt there.
        let b = seal(&KEY, "state", b"hello").unwrap();
        assert!(open(&KEY, "commits", &b).is_err());
    }

    #[test]
    fn flipping_one_ciphertext_bit_is_detected() {
        let mut b = seal(&KEY, "commits", b"hello").unwrap();
        let last = b.len() - 1;
        b[last] ^= 1;
        assert!(open(&KEY, "commits", &b).is_err());
    }

    #[test]
    fn a_short_blob_is_rejected_cleanly() {
        let err = open(&KEY, "commits", b"tiny").unwrap_err();
        assert!(matches!(err, KeyError::Malformed(_)), "got {err:?}");
    }

    #[test]
    fn missing_files_read_as_empty_not_as_errors() {
        let d = tmpdir();
        assert_eq!(read_sealed(&d.join("nope"), &KEY, "state").unwrap(), None);
        assert!(read_lines(&d.join("nope.jsonl"), &KEY, "commits")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn sealed_file_round_trips() {
        let d = tmpdir();
        let p = d.join("state.bin");
        write_sealed(&p, &KEY, "state", b"{}").unwrap();
        assert_eq!(read_sealed(&p, &KEY, "state").unwrap().unwrap(), b"{}");
    }

    #[test]
    fn log_appends_and_reads_back_in_order() {
        let d = tmpdir();
        let p = d.join("commits.jsonl");
        for i in 0..5u8 {
            append_line(&p, &KEY, "commits", &[i]).unwrap();
        }
        let got = read_lines(&p, &KEY, "commits").unwrap();
        assert_eq!(got, (0..5u8).map(|i| vec![i]).collect::<Vec<_>>());
    }

    #[test]
    fn a_torn_final_append_loses_only_that_record() {
        let d = tmpdir();
        let p = d.join("commits.jsonl");
        append_line(&p, &KEY, "commits", b"first").unwrap();
        append_line(&p, &KEY, "commits", b"second").unwrap();

        // Simulate a crash partway through the last line.
        let text = std::fs::read_to_string(&p).unwrap();
        let cut = text.len() - 10;
        std::fs::write(&p, &text[..cut]).unwrap();

        let got = read_lines(&p, &KEY, "commits").unwrap();
        assert_eq!(got, vec![b"first".to_vec()]);
    }

    #[test]
    fn a_wrong_key_on_a_complete_log_is_an_error_not_an_empty_store() {
        // The regression that motivated the ends-with-newline rule: a
        // one-record log read with the wrong key must NOT look empty.
        let d = tmpdir();
        let p = d.join("commits.jsonl");
        append_line(&p, &KEY, "commits", b"only").unwrap();
        assert!(read_lines(&p, &[9u8; 32], "commits").is_err());
    }

    #[test]
    fn a_damaged_record_in_the_middle_is_not_tolerated() {
        // Mid-file damage is not a torn write — it means the file was
        // modified. Silently skipping it would hide exactly the tampering
        // the AEAD exists to detect.
        let d = tmpdir();
        let p = d.join("commits.jsonl");
        append_line(&p, &KEY, "commits", b"first").unwrap();
        append_line(&p, &KEY, "commits", b"second").unwrap();

        let mut lines: Vec<String> = std::fs::read_to_string(&p)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        lines[0] = data_encoding::BASE64.encode(&[0u8; 64]);
        std::fs::write(&p, lines.join("\n") + "\n").unwrap();

        assert!(read_lines(&p, &KEY, "commits").is_err());
    }
}
