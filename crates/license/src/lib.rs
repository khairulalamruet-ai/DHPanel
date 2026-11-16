//! License documents shared by the license authority and the DHPanel installations (T17.03,
//! T17.04, T17.09): deterministic JSON ([`canonical`]), Ed25519 keys ([`keys`]) and the signed
//! entitlement ([`document`]).

pub mod canonical;
pub mod document;
pub mod hash;
pub mod keys;
pub mod release;
pub mod update;

pub use canonical::{CanonicalError, canonical_json};
pub use document::{Claims, Document, DocumentError, Expect, SCHEMA_VERSION};
pub use hash::sha256;
pub use keys::{FileSigningKey, KeyError, PublicKeys, SigningKey, verify_with};
pub use release::{Component, ReleaseError, ReleaseManifest, SignedManifest};
