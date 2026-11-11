//! Authentication primitives shared by the control plane.
//!
//! * [`password`] — argon2id hashing and verification (with rehash detection
//!   and a timing-equalising dummy verify for unknown accounts).
//! * [`policy`] — password strength rules (length, personal info, zxcvbn).
//!
//! Sessions, TOTP and CSRF are added by later tasks (T01.04+).

pub mod password;
pub mod policy;
