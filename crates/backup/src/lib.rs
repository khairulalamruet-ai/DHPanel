//! The backup format (T09.01), and the checks on it (T09.06).
//!
//! # What a backup is
//!
//! A backup of a hosting account is a **set of parts** and a **manifest**, all stored
//! under one name (`<backup id>/`) at a destination:
//!
//! | Part | Name | What |
//! |---|---|---|
//! | files | `files.tar.zst.enc` | the account's files, as a tar, compressed with zstd |
//! | database | `db-<name>.sql.zst.enc` | one per database: a consistent dump, compressed |
//! | dns | `dns.json.enc` | the zone records of the account's domains |
//! | meta | `meta.json.enc` | what is needed to set the account up again (plan, domains, PHP versions, ...) |
//! | manifest | `manifest.json.enc` | the list of the parts above with their sizes and SHA-256 |
//!
//! Every part is encrypted ([`crypto`]) with **XChaCha20-Poly1305 in 64 KiB chunks**: each
//! chunk is authenticated on its own, a chunk cannot be moved or dropped (its number and
//! "last" flag are part of what is authenticated), and a part that ends early is an error,
//! so a damaged or cut backup is found when it is read, not when it is restored half way.
//! The key is made new for every backup (256 random bits), sealed with the panel's master
//! key and kept in the panel's database; the node gets it for the one operation that needs
//! it. The destination (a disk, an S3 bucket, an SFTP folder) never sees a key or a plain
//! byte.
//!
//! # Checks
//!
//! The manifest lists the SHA-256 of every part **as stored** (encrypted), so a part can be
//! checked without any key (a destination can verify its files, and a restore starts by
//! checking all of them before it touches anything); the manifest itself is encrypted and
//! its SHA-256 is kept by the panel with the backup, so replacing a part *and* the manifest
//! is found too.
//!
//! # What is not decided here
//!
//! **Incremental backups.** The first version makes full backups: simple to restore and to
//! check, and the retention rules (daily N, weekly M) keep their number small. A deduplicating
//! scheme (restic-style chunks) would change the destination layout and the manifest, so it
//! comes as a new format version, not as an addition to this one.

pub mod crypto;
pub mod export;
pub mod manifest;
pub mod schedule;

pub use crypto::{KEY_LEN, StreamReader, StreamWriter};
pub use manifest::{Manifest, ManifestError, Part, PartKind};
