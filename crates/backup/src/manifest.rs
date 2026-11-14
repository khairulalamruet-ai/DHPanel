//! The manifest of a backup: which parts there are and what they must hash to.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The version of the format.
pub const FORMAT_VERSION: u32 = 1;
/// Largest manifest accepted.
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

/// What a part holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    /// The account's files (`files.tar.zst.enc`).
    Files,
    /// One database's dump (`db-<name>.sql.zst.enc`).
    Database,
    /// The DNS records (`dns.json.enc`).
    Dns,
    /// What is needed to set the account up again (`meta.json.enc`).
    Meta,
}

/// One part, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    /// Its file name at the destination.
    pub name: String,
    /// What it holds.
    pub kind: PartKind,
    /// Size as stored (encrypted), in bytes.
    pub stored_bytes: u64,
    /// SHA-256 of the part as stored, in hex.
    pub sha256: String,
    /// For a database: its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// The list of parts of one backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The format's version.
    pub version: u32,
    /// The backup's id.
    pub backup_id: String,
    /// The hosting account's system name.
    pub account: String,
    /// When it was made (Unix seconds).
    pub created_at: i64,
    /// The parts.
    pub parts: Vec<Part>,
}

/// Why a manifest or a part was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// Not a manifest of a version this build reads.
    #[error("the manifest is not readable")]
    Unreadable,
    /// A part's name is not one the format makes (it could point elsewhere).
    #[error("a part has a name that is not allowed")]
    BadName,
    /// The same part twice, or two parts of a kind that has one.
    #[error("a part is listed twice")]
    Duplicate,
    /// A part is missing from the destination.
    #[error("part {0} is missing")]
    Missing(String),
    /// A part is not the size the manifest says.
    #[error("part {0} has the wrong size")]
    WrongSize(String),
    /// A part does not hash to what the manifest says: it was changed or damaged.
    #[error("part {0} does not match its checksum")]
    WrongChecksum(String),
}

/// Whether `name` is a part name the format makes: no folders, nothing odd.
#[must_use]
pub fn valid_part_name(name: &str) -> bool {
    let plain = |s: &str| {
        !s.is_empty()
            && s.len() <= 80
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
    };
    match name {
        "files.tar.zst.enc" | "dns.json.enc" | "meta.json.enc" => true,
        _ => name
            .strip_prefix("db-")
            .and_then(|r| r.strip_suffix(".sql.zst.enc"))
            .is_some_and(plain),
    }
}

impl Manifest {
    /// A new, empty manifest.
    #[must_use]
    pub fn new(backup_id: &str, account: &str, created_at: i64) -> Self {
        Self {
            version: FORMAT_VERSION,
            backup_id: backup_id.to_owned(),
            account: account.to_owned(),
            created_at,
            parts: Vec::new(),
        }
    }

    /// Checks the manifest as text: version, names, no duplicates.
    ///
    /// # Errors
    /// [`ManifestError`].
    pub fn check(&self) -> Result<(), ManifestError> {
        if self.version != FORMAT_VERSION {
            return Err(ManifestError::Unreadable);
        }
        let mut seen = std::collections::HashSet::new();
        for part in &self.parts {
            if !valid_part_name(&part.name) {
                return Err(ManifestError::BadName);
            }
            if !seen.insert(part.name.as_str()) {
                return Err(ManifestError::Duplicate);
            }
            if part.sha256.len() != 64 || !part.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(ManifestError::Unreadable);
            }
        }
        Ok(())
    }

    /// The manifest as JSON.
    ///
    /// # Errors
    /// Never in practice.
    pub fn to_json(&self) -> Result<Vec<u8>, ManifestError> {
        serde_json::to_vec(self).map_err(|_| ManifestError::Unreadable)
    }

    /// Reads a manifest (at most [`MAX_MANIFEST_BYTES`]) and checks it as text.
    ///
    /// # Errors
    /// [`ManifestError`].
    pub fn from_reader(r: impl Read) -> Result<Self, ManifestError> {
        let mut bytes = Vec::new();
        r.take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ManifestError::Unreadable)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(ManifestError::Unreadable);
        }
        let manifest: Self =
            serde_json::from_slice(&bytes).map_err(|_| ManifestError::Unreadable)?;
        manifest.check()?;
        Ok(manifest)
    }

    /// Checks a part against the manifest: it must be listed, be the size said and hash
    /// to what is said. `stored` is the part's bytes as stored.
    ///
    /// # Errors
    /// [`ManifestError`].
    pub fn verify_part(&self, name: &str, stored: impl Read) -> Result<(), ManifestError> {
        let part = self
            .parts
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| ManifestError::Missing(name.to_owned()))?;
        let (size, sha) = hash_of(stored).map_err(|_| ManifestError::Missing(name.to_owned()))?;
        if size != part.stored_bytes {
            return Err(ManifestError::WrongSize(name.to_owned()));
        }
        if sha != part.sha256 {
            return Err(ManifestError::WrongChecksum(name.to_owned()));
        }
        Ok(())
    }
}

/// Size and SHA-256 (hex) of what `r` gives.
///
/// # Errors
/// Whatever `r` says.
pub fn hash_of(mut r: impl Read) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

/// Lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A writer that counts and hashes what passes through it (the stored size and checksum of
/// a part, found while it is written).
pub struct Counting<W: Write> {
    inner: W,
    hasher: Sha256,
    size: u64,
}

impl<W: Write> Counting<W> {
    /// Wraps a writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            size: 0,
        }
    }

    /// The writer back, with the size and SHA-256 (hex) of everything written.
    pub fn finish(self) -> (W, u64, String) {
        (self.inner, self.size, hex(&self.hasher.finalize()))
    }
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(data)?;
        self.hasher.update(&data[..n]);
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn part(name: &str, data: &[u8]) -> Part {
        let (stored_bytes, sha256) = hash_of(data).unwrap();
        Part {
            name: name.into(),
            kind: PartKind::Files,
            stored_bytes,
            sha256,
            database: None,
        }
    }

    #[test]
    fn part_names_are_only_what_the_format_makes() {
        for good in [
            "files.tar.zst.enc",
            "dns.json.enc",
            "meta.json.enc",
            "db-alice_shop.sql.zst.enc",
        ] {
            assert!(valid_part_name(good), "{good}");
        }
        for bad in [
            "",
            "../files.tar.zst.enc",
            "db-.sql.zst.enc",
            "db-A.sql.zst.enc",
            "db-a/b.sql.zst.enc",
            "other.txt",
            "files.tar.zst",
        ] {
            assert!(!valid_part_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_part_is_checked_against_the_manifest() {
        let mut m = Manifest::new("b1", "alice", 1);
        m.parts.push(part("files.tar.zst.enc", b"stored bytes"));
        m.check().unwrap();
        m.verify_part("files.tar.zst.enc", &b"stored bytes"[..])
            .unwrap();
        assert_eq!(
            m.verify_part("files.tar.zst.enc", &b"stored bytez"[..]),
            Err(ManifestError::WrongChecksum("files.tar.zst.enc".into()))
        );
        assert_eq!(
            m.verify_part("files.tar.zst.enc", &b"stored byte"[..]),
            Err(ManifestError::WrongSize("files.tar.zst.enc".into()))
        );
        assert_eq!(
            m.verify_part("dns.json.enc", &b""[..]),
            Err(ManifestError::Missing("dns.json.enc".into()))
        );
    }

    #[test]
    fn a_manifest_with_odd_names_or_twice_the_same_part_is_refused() {
        let mut m = Manifest::new("b1", "alice", 1);
        m.parts.push(part("../x", b"a"));
        assert_eq!(m.check(), Err(ManifestError::BadName));
        let mut m = Manifest::new("b1", "alice", 1);
        m.parts.push(part("files.tar.zst.enc", b"a"));
        m.parts.push(part("files.tar.zst.enc", b"b"));
        assert_eq!(m.check(), Err(ManifestError::Duplicate));
        let mut m = Manifest::new("b1", "alice", 1);
        m.version = 9;
        assert_eq!(m.check(), Err(ManifestError::Unreadable));
        assert!(Manifest::from_reader(&b"not json"[..]).is_err());
        let ok = Manifest::new("b1", "alice", 1).to_json().unwrap();
        assert_eq!(Manifest::from_reader(&ok[..]).unwrap().backup_id, "b1");
    }

    #[test]
    fn counting_finds_the_size_and_checksum_while_writing() {
        let mut w = Counting::new(Vec::new());
        w.write_all(b"stored ").unwrap();
        w.write_all(b"bytes").unwrap();
        let (inner, size, sha) = w.finish();
        assert_eq!(inner, b"stored bytes");
        assert_eq!((size, sha), hash_of(&b"stored bytes"[..]).unwrap());
    }
}
