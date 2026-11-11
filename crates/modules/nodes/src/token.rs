//! Enrollment tokens: 256 random bits, `dhn_` + base64url.
//!
//! Only the SHA-256 of a token is stored. A token carries 256 bits of
//! randomness, so a fast hash is enough; it cannot be guessed from the hash.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use secrecy::SecretString;
use sha2::{Digest as _, Sha256};

/// Every token starts with this, which makes leaked tokens easy to spot in
/// logs and secret scanners.
pub const TOKEN_PREFIX: &str = "dhn_";
const RANDOM_BYTES: usize = 32;
/// Length of a well-formed token: prefix plus 43 base64url characters.
pub const TOKEN_LEN: usize = TOKEN_PREFIX.len() + 43;

/// The OS random number generator failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RngError;

impl std::fmt::Display for RngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("random number generator unavailable")
    }
}

impl std::error::Error for RngError {}

/// A fresh token and its hash. Show the token once; store only the hash.
///
/// # Errors
/// [`RngError`] if the OS random number generator is unavailable.
pub fn generate() -> Result<(SecretString, [u8; 32]), RngError> {
    let mut raw = [0u8; RANDOM_BYTES];
    getrandom::fill(&mut raw).map_err(|_| RngError)?;
    let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(raw));
    let hash = Sha256::digest(token.as_bytes()).into();
    Ok((SecretString::from(token), hash))
}

/// Hash of a presented token, or `None` when it cannot be a token we issued
/// (wrong prefix, length or alphabet). Callers treat `None` exactly like an
/// unknown token.
#[must_use]
pub fn hash(token: &str) -> Option<[u8; 32]> {
    let body = token.strip_prefix(TOKEN_PREFIX)?;
    let well_formed = token.len() == TOKEN_LEN
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && URL_SAFE_NO_PAD
            .decode(body)
            .is_ok_and(|bytes| bytes.len() == RANDOM_BYTES);
    well_formed.then(|| Sha256::digest(token.as_bytes()).into())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use secrecy::ExposeSecret as _;

    use super::*;

    #[test]
    fn tokens_are_long_random_prefixed_and_shell_safe() {
        let (a, ha) = generate().unwrap();
        let (b, hb) = generate().unwrap();
        let text = a.expose_secret();
        assert!(
            text.starts_with("dhn_") && text.len() == TOKEN_LEN,
            "{text}"
        );
        assert!(
            text.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        );
        assert_ne!(text, b.expose_secret());
        assert_ne!(ha, hb);
    }

    #[test]
    fn the_hash_of_a_token_is_what_generate_returned() {
        let (token, hash_at_creation) = generate().unwrap();
        assert_eq!(hash(token.expose_secret()), Some(hash_at_creation));
    }

    #[test]
    fn malformed_tokens_have_no_hash() {
        let (token, _) = generate().unwrap();
        let good = token.expose_secret().to_owned();
        let wrong_prefix = good.replacen("dhn_", "dhx_", 1);
        for bad in [
            "",
            "dhn_",
            &good[..good.len() - 1],
            &format!("{good}A"),
            &wrong_prefix,
            &good.replace("dhn_", ""),
            &format!("{} ", good),
            &format!("dhn_{}", "!".repeat(43)),
            &format!("dhn_{}", "=".repeat(43)),
        ] {
            assert_eq!(hash(bad), None, "{bad:?}");
        }
        // Tokens are case sensitive: lower-casing a token changes it.
        assert_ne!(hash(&good.to_lowercase()), hash(&good));
    }
}
