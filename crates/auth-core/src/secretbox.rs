//! Encryption of secrets at rest (e.g. TOTP secrets) with XChaCha20-Poly1305.
//!
//! Blob layout: `version (1 byte, currently 1) || nonce (24 bytes) || ciphertext+tag`.
//! Every call draws a fresh random nonce; with 192-bit nonces random selection
//! is safe for any realistic number of messages.
//!
//! The caller passes *associated data* that binds a blob to its place (for
//! example `"account_mfa:totp:<account id>"`), so a ciphertext copied to
//! another row or account fails to decrypt.
//!
//! The 256-bit master key comes from configuration (a 0600 file); it is wiped
//! from memory on drop and never appears in `Debug` output.

use chacha20poly1305::{
    Key, KeyInit as _, XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Payload},
};

/// Blob format version written by [`MasterKey::seal`].
const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
/// Master key length in bytes.
pub const KEY_LEN: usize = 32;

/// Errors from sealing and opening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretBoxError {
    /// The key is not 32 bytes of standard base64.
    #[error("master key must be 32 bytes, base64-encoded")]
    InvalidKey,
    /// OS randomness unavailable.
    #[error("random number generator unavailable")]
    Rng,
    /// Wrong key, wrong associated data, or a corrupted/truncated blob.
    #[error("secret could not be decrypted")]
    Decrypt,
    /// Encryption failed (should not happen).
    #[error("secret could not be encrypted")]
    Encrypt,
}

/// The key that encrypts secrets at rest. Cheap to clone.
#[derive(Clone)]
pub struct MasterKey(XChaCha20Poly1305);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

impl MasterKey {
    /// From raw key bytes.
    #[must_use]
    pub fn from_bytes(key: &[u8; KEY_LEN]) -> Self {
        Self(XChaCha20Poly1305::new(&Key::from(*key)))
    }

    /// From the standard-base64 text of a 32-byte key (surrounding whitespace
    /// is ignored), as produced by `openssl rand -base64 32`.
    ///
    /// # Errors
    /// [`SecretBoxError::InvalidKey`].
    pub fn from_base64(text: &str) -> Result<Self, SecretBoxError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let mut bytes = STANDARD
            .decode(text.trim())
            .map_err(|_| SecretBoxError::InvalidKey)?;
        let key: Result<[u8; KEY_LEN], _> = bytes.as_slice().try_into();
        let result = key
            .map(|k| Self::from_bytes(&k))
            .map_err(|_| SecretBoxError::InvalidKey);
        bytes.fill(0);
        result
    }

    /// Encrypts `plaintext`, binding it to `associated_data`.
    ///
    /// # Errors
    /// RNG or encryption failure.
    pub fn seal(
        &self,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SecretBoxError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| SecretBoxError::Rng)?;
        let ciphertext = self
            .0
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: associated_data,
                },
            )
            .map_err(|_| SecretBoxError::Encrypt)?;
        let mut blob = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        blob.push(VERSION);
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(blob)
    }

    /// Decrypts a blob made by [`MasterKey::seal`] with the same associated data.
    ///
    /// # Errors
    /// [`SecretBoxError::Decrypt`] for any mismatch or corruption.
    pub fn open(&self, associated_data: &[u8], blob: &[u8]) -> Result<Vec<u8>, SecretBoxError> {
        if blob.len() < 1 + NONCE_LEN + TAG_LEN || blob[0] != VERSION {
            return Err(SecretBoxError::Decrypt);
        }
        let (nonce, ciphertext) = blob[1..].split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SecretBoxError::Decrypt)?;
        self.0
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: associated_data,
                },
            )
            .map_err(|_| SecretBoxError::Decrypt)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn key(byte: u8) -> MasterKey {
        MasterKey::from_bytes(&[byte; KEY_LEN])
    }

    #[test]
    fn round_trips_and_uses_a_fresh_nonce_each_time() {
        let k = key(7);
        let a = k.seal(b"row:1", b"totp-secret").unwrap();
        let b = k.seal(b"row:1", b"totp-secret").unwrap();
        assert_ne!(a, b, "nonce must differ");
        assert_eq!(a.len(), 1 + 24 + 11 + 16);
        assert_eq!(k.open(b"row:1", &a).unwrap(), b"totp-secret");
        assert_eq!(k.open(b"row:1", &b).unwrap(), b"totp-secret");
        assert!(!a.windows(11).any(|w| w == b"totp-secret"));
    }

    #[test]
    fn wrong_key_context_or_tampering_fails() {
        let k = key(7);
        let blob = k.seal(b"row:1", b"secret").unwrap();
        assert_eq!(key(8).open(b"row:1", &blob), Err(SecretBoxError::Decrypt));
        assert_eq!(k.open(b"row:2", &blob), Err(SecretBoxError::Decrypt));
        for i in [0, 1, 25, blob.len() - 1] {
            let mut bad = blob.clone();
            bad[i] ^= 1;
            assert_eq!(
                k.open(b"row:1", &bad),
                Err(SecretBoxError::Decrypt),
                "byte {i}"
            );
        }
        assert_eq!(k.open(b"row:1", &blob[..40]), Err(SecretBoxError::Decrypt));
        assert_eq!(k.open(b"row:1", b""), Err(SecretBoxError::Decrypt));
    }

    #[test]
    fn base64_keys_must_be_exactly_32_bytes() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let good = STANDARD.encode([9u8; 32]);
        let k = MasterKey::from_base64(&format!("  {good}\n")).unwrap();
        let blob = k.seal(b"x", b"y").unwrap();
        assert_eq!(key(9).open(b"x", &blob).unwrap(), b"y");
        for bad in [
            "",
            "not base64!",
            &STANDARD.encode([9u8; 31]),
            &STANDARD.encode([9u8; 33]),
        ] {
            assert_eq!(
                MasterKey::from_base64(bad).unwrap_err(),
                SecretBoxError::InvalidKey
            );
        }
    }

    #[test]
    fn debug_is_redacted() {
        assert_eq!(format!("{:?}", key(1)), "MasterKey(<redacted>)");
    }
}
