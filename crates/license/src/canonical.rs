//! Deterministic JSON (RFC 8785, the JSON Canonicalization Scheme) for what is signed (T17.04).
//!
//! The same value always gives the same bytes: no whitespace, object members sorted by the
//! UTF-16 code units of their names, strings escaped the one way the RFC says, numbers in
//! their shortest form. Only what a license needs is accepted: whole numbers (no fractions,
//! no exponents, nothing beyond ±2^53 so every reader agrees on them), strings, booleans,
//! null, arrays and objects. Anything else is refused rather than guessed at.

use std::fmt::Write as _;

use serde_json::Value;

/// The largest whole number every JSON reader holds exactly (2^53 - 1).
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Deepest nesting accepted.
pub const MAX_DEPTH: usize = 32;

/// Why a value could not be made canonical.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalError {
    /// A number that is not a whole number within ±(2^53 - 1).
    #[error("only whole numbers within +-9007199254740991 are allowed")]
    Number,
    /// Nesting deeper than [`MAX_DEPTH`].
    #[error("nested too deeply")]
    Depth,
}

/// The canonical bytes of `value`.
///
/// # Errors
/// [`CanonicalError`].
pub fn canonical_json(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = String::new();
    write(value, &mut out, 0)?;
    Ok(out.into_bytes())
}

fn write(value: &Value, out: &mut String, depth: usize) -> Result<(), CanonicalError> {
    if depth > MAX_DEPTH {
        return Err(CanonicalError::Depth);
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            let whole = n
                .as_i64()
                .filter(|i| i.abs() <= MAX_SAFE_INTEGER)
                .ok_or(CanonicalError::Number)?;
            let _ = write!(out, "{whole}");
        }
        Value::String(s) => string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(key, out);
                out.push(':');
                if let Some(v) = map.get(key) {
                    write(v, out, depth + 1)?;
                }
            }
            out.push('}');
        }
    }
    Ok(())
}

fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text(v: &Value) -> String {
        String::from_utf8(canonical_json(v).unwrap()).unwrap()
    }

    #[test]
    fn the_rfc_examples_come_out_as_written() {
        // RFC 8785 section 3.2.3: members sorted by UTF-16 code units.
        let v: Value = serde_json::from_str(
            r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#,
        )
        .unwrap();
        assert_eq!(
            text(&v),
            "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",\"\u{f6}\":\"Latin Small Letter O With Diaeresis\",\"\u{20ac}\":\"Euro Sign\",\"\u{1f600}\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}"
        );
        assert_eq!(
            text(&json!({"b": [1, 2, {"d": true, "c": null}], "a": "x"})),
            "{\"a\":\"x\",\"b\":[1,2,{\"c\":null,\"d\":true}]}"
        );
        assert_eq!(text(&json!("a\"b\\c\n\u{1}")), "\"a\\\"b\\\\c\\n\\u0001\"");
    }

    #[test]
    fn numbers_that_readers_could_disagree_about_are_refused() {
        assert!(canonical_json(&json!(1.5)).is_err());
        assert!(canonical_json(&json!(1e3)).is_err());
        assert!(canonical_json(&json!(9_007_199_254_740_992_i64)).is_err());
        assert!(canonical_json(&json!(u64::MAX)).is_err());
        assert_eq!(
            text(&json!(-9_007_199_254_740_991_i64)),
            "-9007199254740991"
        );
        let mut deep = json!(1);
        for _ in 0..40 {
            deep = json!([deep]);
        }
        assert_eq!(canonical_json(&deep), Err(CanonicalError::Depth));
    }

    /// A small deterministic generator, so the property test needs no extra crate.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    fn random_value(rng: &mut Rng, depth: usize) -> Value {
        match rng.next() % if depth > 3 { 4 } else { 6 } {
            0 => Value::Null,
            1 => Value::Bool(rng.next().is_multiple_of(2)),
            2 => json!(i64::try_from(rng.next() % 2_000_000).unwrap() - 1_000_000),
            3 => {
                let n = rng.next() % 8;
                let s: String = (0..n)
                    .map(|_| {
                        char::from_u32(u32::try_from(rng.next() % 0x2fff).unwrap() + 1)
                            .unwrap_or('?')
                    })
                    .collect();
                Value::String(s)
            }
            4 => Value::Array(
                (0..rng.next() % 4)
                    .map(|_| random_value(rng, depth + 1))
                    .collect(),
            ),
            _ => {
                let mut m = serde_json::Map::new();
                for _ in 0..rng.next() % 4 {
                    let key: String = (0..=rng.next() % 3)
                        .map(|_| {
                            char::from_u32(u32::try_from(rng.next() % 0x3000).unwrap() + 0x20)
                                .unwrap_or('k')
                        })
                        .collect();
                    m.insert(key, random_value(rng, depth + 1));
                }
                Value::Object(m)
            }
        }
    }

    #[test]
    fn the_same_value_is_the_same_bytes_however_it_was_built_or_read() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..500 {
            let v = random_value(&mut rng, 0);
            let first = canonical_json(&v).unwrap();
            // Written out and read back (member order may change), and pretty printed.
            let again: Value = serde_json::from_slice(&serde_json::to_vec(&v).unwrap()).unwrap();
            let pretty: Value =
                serde_json::from_str(&serde_json::to_string_pretty(&v).unwrap()).unwrap();
            assert_eq!(canonical_json(&again).unwrap(), first);
            assert_eq!(canonical_json(&pretty).unwrap(), first);
            // And the canonical text reads back to the same value.
            let read: Value = serde_json::from_slice(&first).unwrap();
            assert_eq!(read, v);
            assert_eq!(canonical_json(&read).unwrap(), first);
        }
    }
}
