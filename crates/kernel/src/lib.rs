//! Module manifest, module trait, module registry and (later) the event bus.
//!
//! * [`manifest`]: `module.toml` (T03.01).
//! * [`module`]: the `Module` trait (T03.02).
//! * [`registry`]: all modules of a build, checked and ordered (T03.02).

pub mod manifest;
pub mod module;
pub mod registry;

pub use manifest::{Manifest, ManifestError};
pub use module::{Event, Health, ManifestOnly, Module, ModuleRoutes};
pub use registry::{Registry, RegistryError};
