//! The header of an account export (T09.11): who and what it is, and the backup's key,
//! wrapped with a passphrase the person chose.
//!
//! An export is one file (a tar) holding `export.json` (this header), `manifest.json.enc` and
//! every part of one backup, exactly as stored (encrypted with the backup's key). Taking it
//! to another panel needs the key: the header carries it, encrypted with a key derived from
//! the passphrase (PBKDF2-HMAC-SHA256, 600 000 rounds, a random salt). The passphrase is never
//! stored; without it the file is as unreadable as the backup was.

use std::io::{Read, Write};

use hmac::{Hmac, KeyInit as _, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{KEY_LEN, StreamReader, StreamWriter, manifest::hex};

/// Rounds used when wrapping.
pub const ROUNDS: u32 = 600_000;
const MIN_ROUNDS: u32 = 1_000;
const MAX_ROUNDS: u32 = 5_000_000;
/// Shortest passphrase accepted.
pub const MIN_PASSPHRASE: usize = 12;
/// The header's name in the export file.
pub const HEADER: &str = "export.json";

/// Why a header or a passphrase is not usable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    /// Not a header this version reads.
    #[error("not an account export")]
    Unreadable,
    /// The passphrase is wrong (or the header was changed).
    #[error("the passphrase is wrong")]
    WrongPassphrase,
    /// The system's random source failed.
    #[error("no random numbers")]
    Random,
}

/// The wrapped key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    kdf: String,
    rounds: u32,
    salt: String,
    wrapped: String,
}

/// An export's header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The header's format (1).
    pub format: u32,
    /// The backup's id (the folder name it keeps when imported).
    pub backup_id: String,
    /// The hosting account it was made from.
    pub account: String,
    /// When the backup was made (Unix seconds).
    pub created_at: i64,
    /// The backup's key, wrapped.
    pub key: WrappedKey,
}

fn pbkdf2(passphrase: &str, salt: &[u8], rounds: u32) -> [u8; KEY_LEN] {
    let prf = |data: &[u8]| -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(passphrase.as_bytes())
            .unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
        mac.update(data);
        mac.finalize().into_bytes().into()
    };
    // One block of 32 bytes is all a 256-bit key needs.
    let mut first = salt.to_vec();
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = prf(&first);
    let mut out = u;
    for _ in 1..rounds {
        u = prf(&u);
        for (o, b) in out.iter_mut().zip(u) {
            *o ^= b;
        }
    }
    out
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

impl Header {
    /// A header for `backup_id` with `key` wrapped by `passphrase`.
    ///
    /// # Errors
    /// [`ExportError::Random`].
    pub fn new(
        backup_id: &str,
        account: &str,
        created_at: i64,
        key: &[u8; KEY_LEN],
        passphrase: &str,
    ) -> Result<Self, ExportError> {
        Self::with_rounds(backup_id, account, created_at, key, passphrase, ROUNDS)
    }

    fn with_rounds(
        backup_id: &str,
        account: &str,
        created_at: i64,
        key: &[u8; KEY_LEN],
        passphrase: &str,
        rounds: u32,
    ) -> Result<Self, ExportError> {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).map_err(|_| ExportError::Random)?;
        let kek = pbkdf2(passphrase, &salt, rounds);
        let mut sealed = StreamWriter::new(Vec::new(), &kek).map_err(|_| ExportError::Random)?;
        sealed.write_all(key).map_err(|_| ExportError::Random)?;
        let sealed = sealed.finish().map_err(|_| ExportError::Random)?;
        Ok(Self {
            format: 1,
            backup_id: backup_id.to_owned(),
            account: account.to_owned(),
            created_at,
            key: WrappedKey {
                kdf: "pbkdf2-sha256".into(),
                rounds,
                salt: hex(&salt),
                wrapped: hex(&sealed),
            },
        })
    }

    /// The header as JSON.
    ///
    /// # Errors
    /// [`ExportError::Unreadable`].
    pub fn to_json(&self) -> Result<Vec<u8>, ExportError> {
        serde_json::to_vec(self).map_err(|_| ExportError::Unreadable)
    }

    /// Reads a header (at most 16 KiB).
    ///
    /// # Errors
    /// [`ExportError::Unreadable`].
    pub fn from_reader(r: impl Read) -> Result<Self, ExportError> {
        let mut bytes = Vec::new();
        r.take(16 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ExportError::Unreadable)?;
        if bytes.len() > 16 * 1024 {
            return Err(ExportError::Unreadable);
        }
        let header: Self = serde_json::from_slice(&bytes).map_err(|_| ExportError::Unreadable)?;
        let plain_id =
            header.backup_id.len() == 32 && header.backup_id.bytes().all(|b| b.is_ascii_hexdigit());
        if header.format != 1
            || !plain_id
            || header.key.kdf != "pbkdf2-sha256"
            || !(MIN_ROUNDS..=MAX_ROUNDS).contains(&header.key.rounds)
        {
            return Err(ExportError::Unreadable);
        }
        Ok(header)
    }

    /// The backup's key, opened with the passphrase.
    ///
    /// # Errors
    /// [`ExportError::WrongPassphrase`] or [`ExportError::Unreadable`].
    pub fn unwrap_key(&self, passphrase: &str) -> Result<[u8; KEY_LEN], ExportError> {
        let salt = from_hex(&self.key.salt).ok_or(ExportError::Unreadable)?;
        let sealed = from_hex(&self.key.wrapped).ok_or(ExportError::Unreadable)?;
        let kek = pbkdf2(passphrase, &salt, self.key.rounds);
        let mut plain = Vec::new();
        StreamReader::new(&sealed[..], &kek)
            .read_to_end(&mut plain)
            .map_err(|_| ExportError::WrongPassphrase)?;
        plain.try_into().map_err(|_| ExportError::Unreadable)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_matches_the_published_vector() {
        // RFC 7914 section 11: PBKDF2-HMAC-SHA-256, "passwd", "salt", 1 round, first 32 bytes.
        let out = pbkdf2("passwd", b"salt", 1);
        assert_eq!(
            hex(&out),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc"
        );
        let out = pbkdf2("Password", b"NaCl", 80_000);
        assert_eq!(
            hex(&out),
            "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56"
        );
    }

    #[test]
    fn the_key_comes_back_with_the_passphrase_only() {
        let key = [9u8; KEY_LEN];
        let id = "0123456789abcdef0123456789abcdef";
        let h = Header::with_rounds(
            id,
            "alice",
            1_700_000_000,
            &key,
            "correct horse battery",
            2000,
        )
        .unwrap();
        let json = h.to_json().unwrap();
        assert!(!String::from_utf8_lossy(&json).contains("correct horse"));
        let read = Header::from_reader(&json[..]).unwrap();
        assert_eq!(read, h);
        assert_eq!(read.unwrap_key("correct horse battery").unwrap(), key);
        assert_eq!(
            read.unwrap_key("correct horse batterz"),
            Err(ExportError::WrongPassphrase)
        );
        // A header with too few or too many rounds, or a bad id, is not read.
        let mut weak = h.clone();
        weak.key.rounds = 10;
        assert!(Header::from_reader(&weak.to_json().unwrap()[..]).is_err());
        let mut odd = h;
        odd.backup_id = "../etc".into();
        assert!(Header::from_reader(&odd.to_json().unwrap()[..]).is_err());
    }
}
