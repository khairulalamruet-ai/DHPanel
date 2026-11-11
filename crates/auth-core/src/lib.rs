//! Authentication primitives shared by the control plane.
//!
//! * [`password`] — argon2id hashing and verification (with rehash detection
//!   and a timing-equalising dummy verify for unknown accounts).
//! * [`policy`] — password strength rules (length, personal info, zxcvbn).
//! * [`secretbox`] — XChaCha20-Poly1305 encryption of secrets at rest.
//! * [`totp`] — RFC 6238 one-time codes and single-use backup codes.

pub mod password;
pub mod policy;
pub mod secretbox;
pub mod totp;
