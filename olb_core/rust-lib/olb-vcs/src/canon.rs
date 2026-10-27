//! Canonical JSON (docs/10 §1.1): JCS minus floats.
//!
//! Commit ids must be byte-identical on every machine, so this cannot lean on
//! `serde_json::to_vec`. That function preserves whatever ordering the `Map`
//! type happens to give, and serde_json's `Map` is only a `BTreeMap` because
//! nothing in our dependency graph enables `preserve_order`. Any future crate
//! that does would flip it to `IndexMap` through feature unification and
//! silently change every commit id in the store, with no compile error. Keys
//! are therefore sorted here, explicitly.
//!
//! **Floats are refused, not rounded.** Reproducing ECMAScript's
//! `Number::toString` is the hardest part of RFC 8785 and the only part with
//! no safe failure mode: a rejected float is a bug you find immediately, a
//! mis-encoded one is silent history corruption. A measured quantity belongs
//! in a record as a decimal string plus a unit, which docs/05 §1.1 already
//! implies with its `{name, value, unit, source}` parameters.

use crate::types::VcsError;
use serde_json::Value;

/// Serialize `v` canonically: object keys sorted by their UTF-8 bytes, no
/// insignificant whitespace, floats rejected.
pub fn to_vec(v: &Value) -> Result<Vec<u8>, VcsError> {
    let mut out = Vec::new();
    write(v, &mut out)?;
    Ok(out)
}

/// Convenience for callers that want a `String`. Canonical JSON is valid
/// UTF-8 by construction, so this cannot fail beyond what `to_vec` already
/// rejects.
pub fn to_string(v: &Value) -> Result<String, VcsError> {
    Ok(String::from_utf8(to_vec(v)?).expect("canonical JSON is UTF-8 by construction"))
}

/// Canonicalise any serialisable value by going through `serde_json::Value`.
pub fn to_vec_of<T: serde::Serialize>(t: &T) -> Result<Vec<u8>, VcsError> {
    let v = serde_json::to_value(t).map_err(|e| VcsError::Malformed(e.to_string()))?;
    to_vec(&v)
}

fn write(v: &Value, out: &mut Vec<u8>) -> Result<(), VcsError> {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.extend_from_slice(i.to_string().as_bytes());
            } else if let Some(u) = n.as_u64() {
                out.extend_from_slice(u.to_string().as_bytes());
            } else {
                return Err(VcsError::NotCanonical(format!(
                    "float {n} cannot be canonicalised; store measured values as a \
                     decimal string with a unit"
                )));
            }
        }
        Value::String(s) => write_str(s, out),
        Value::Array(a) => {
            out.push(b'[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write(item, out)?;
            }
            out.push(b']');
        }
        Value::Object(m) => {
            // Sorted by key. serde_json's iteration order is an
            // implementation detail we deliberately do not trust.
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_str(k, out);
                out.push(b':');
                write(&m[*k], out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn write_str(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for c in s.chars() {
        match c {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' => out.extend_from_slice(b"\\n"),
            '\r' => out.extend_from_slice(b"\\r"),
            '\t' => out.extend_from_slice(b"\\t"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\u{0c}' => out.extend_from_slice(b"\\f"),
            c if (c as u32) < 0x20 => {
                out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes())
            }
            c => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_are_sorted_regardless_of_input_order() {
        let a = to_string(&json!({"b": 1, "a": 2, "c": 3})).unwrap();
        let b = to_string(&json!({"c": 3, "a": 2, "b": 1})).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn nested_objects_are_sorted_too() {
        let s = to_string(&json!({"z": {"y": 1, "x": 2}, "a": [{"q": 1, "p": 2}]})).unwrap();
        assert_eq!(s, r#"{"a":[{"p":2,"q":1}],"z":{"x":2,"y":1}}"#);
    }

    #[test]
    fn array_order_is_preserved() {
        // Arrays are ordered data, not a set. Sorting them would change
        // meaning: `predictions[]` and `parents[1..]` are ordered by the
        // caller for a reason.
        let s = to_string(&json!([3, 1, 2])).unwrap();
        assert_eq!(s, "[3,1,2]");
    }

    #[test]
    fn floats_are_refused_not_rounded() {
        let err = to_vec(&json!({"mass": 1.5})).unwrap_err();
        assert!(matches!(err, VcsError::NotCanonical(_)), "got {err:?}");
    }

    #[test]
    fn a_float_nested_anywhere_is_still_refused() {
        assert!(to_vec(&json!({"a": [{"b": [0.1]}]})).is_err());
    }

    #[test]
    fn integers_at_the_edges_survive() {
        let s = to_string(&json!({"min": i64::MIN, "max": i64::MAX})).unwrap();
        assert_eq!(
            s,
            format!(r#"{{"max":{},"min":{}}}"#, i64::MAX, i64::MIN)
        );
    }

    #[test]
    fn control_characters_are_escaped() {
        let s = to_string(&json!("a\u{1}b\tc\nd\"e\\f")).unwrap();
        // Built rather than written as a literal: a raw string holding an
        // escape sequence for a control character is exactly the kind of
        // thing an editor silently turns into the character itself.
        let expected = String::from("\"a") + "\\u0001" + "b\\tc\\nd\\\"e\\\\f\"";
        assert_eq!(s, expected);
    }

    #[test]
    fn backspace_and_formfeed_use_short_escapes() {
        assert_eq!(to_string(&json!("\u{08}\u{0c}")).unwrap(), r#""\b\f""#);
    }

    #[test]
    fn non_ascii_is_not_escaped() {
        // JCS emits UTF-8 directly; escaping would be valid JSON but a
        // different byte string, and therefore a different commit id.
        assert_eq!(to_string(&json!("Ωmega · 日本")).unwrap(), "\"Ωmega · 日本\"");
    }

    #[test]
    fn empty_containers() {
        assert_eq!(to_string(&json!({})).unwrap(), "{}");
        assert_eq!(to_string(&json!([])).unwrap(), "[]");
    }

    #[test]
    fn output_is_stable_across_reparse() {
        let v = json!({"b": [1, {"d": null, "c": true}], "a": "x"});
        let once = to_string(&v).unwrap();
        let reparsed: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(once, to_string(&reparsed).unwrap());
    }
}
