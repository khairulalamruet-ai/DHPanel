//! Module trait, module registry, event bus and core error types.
//!
//! So far: the module manifest ([`manifest`], T03.01). The `Module` trait and
//! the registry arrive with T03.02.

pub mod manifest;

pub use manifest::{Manifest, ManifestError};
