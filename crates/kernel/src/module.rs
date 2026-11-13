//! What a module is to the rest of the system (T03.02).
//!
//! Modules are compiled in (workspace crates), not loaded at run time: there
//! is no plugin ABI to defend. A module is an object that hands out its
//! manifest and, optionally, routes, migrations, background jobs, reactions to
//! events and a health check. Everything except the manifest has a default, so
//! a small module is a few lines.

use std::{future::Future, pin::Pin, time::Duration};

use axum::Router;

use crate::manifest::Manifest;

/// A boxed future, so the trait can be used as `dyn Module`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The HTTP routes of a module, already carrying their state. Paths are full
/// paths (`/api/v1/admin/...`), as everywhere in this code base.
#[derive(Default)]
pub struct ModuleRoutes {
    /// For administrators (`/api/v1/admin/...`).
    pub admin: Option<Router>,
    /// For customers (`/api/v1/client/...`).
    pub client: Option<Router>,
    /// Without a session (`/api/v1/public/...`): sign-in, enrollment, webhooks.
    pub public: Option<Router>,
}

impl ModuleRoutes {
    /// No routes.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether the module has no routes at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.admin.is_none() && self.client.is_none() && self.public.is_none()
    }
}

/// A database migration owned by a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// Sorting key, unique across the whole system (`YYYYMMDDNNNNNN`).
    pub version: i64,
    /// Short description.
    pub description: &'static str,
    /// The SQL to apply.
    pub up: &'static str,
}

/// A background job a module wants the worker to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobSpec {
    /// Unique name: `<module>.<job>`.
    pub name: &'static str,
    /// How often it runs.
    pub every: Duration,
}

/// Something that happened, announced to every enabled module. The typed
/// domain events (tenant created, domain added, ...) are added by T03.05.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// A module was switched on.
    ModuleEnabled {
        /// Its name.
        name: String,
    },
    /// A module was switched off.
    ModuleDisabled {
        /// Its name.
        name: String,
    },
}

/// A module could not handle an event. The message is for the log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EventError(pub String);

/// How a module is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// Working.
    Ok,
    /// Working with a problem worth showing to an administrator.
    Degraded(String),
    /// Not working.
    Down(String),
}

/// The contract every module implements.
pub trait Module: Send + Sync + 'static {
    /// The module's manifest (name, dependencies, permissions, ...).
    fn manifest(&self) -> &Manifest;

    /// The module's HTTP routes. Only called for enabled modules.
    fn routes(&self) -> ModuleRoutes {
        ModuleRoutes::none()
    }

    /// The module's database migrations, oldest first.
    fn migrations(&self) -> &'static [Migration] {
        &[]
    }

    /// Background jobs for the worker.
    fn jobs(&self) -> Vec<JobSpec> {
        Vec::new()
    }

    /// Reacts to an event from another module (for example: a tenant was
    /// deleted, remove its domains). Must be safe to run more than once.
    fn on_event<'a>(&'a self, _event: &'a Event) -> BoxFuture<'a, Result<(), EventError>> {
        Box::pin(async { Ok(()) })
    }

    /// The module's own health.
    fn health(&self) -> BoxFuture<'_, Health> {
        Box::pin(async { Health::Ok })
    }
}

/// A module that is only its manifest: for modules whose code is not yet
/// behind the trait, and for tests.
#[derive(Debug, Clone)]
pub struct ManifestOnly(pub Manifest);

impl Module for ManifestOnly {
    fn manifest(&self) -> &Manifest {
        &self.0
    }
}
