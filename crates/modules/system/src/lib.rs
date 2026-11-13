//! The system module: the panel's own machinery.
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/admin/modules` | `admin.modules.read` |
//! | PUT | `/api/v1/admin/modules/{name}` | `admin.modules.manage`, sudo mode |
//!
//! Switching a module changes the database and, right after the commit, the
//! routes the server answers ([`dhpanel_kernel::ModuleMount`]): no restart.
//!
//! Rules, enforced here and again by the table and the mount:
//! * a core module cannot be switched off;
//! * a module cannot be switched off while modules that need it are on, and
//!   cannot be switched on while modules it needs are off — unless the request
//!   says `cascade`, which switches those too. Without `cascade` the answer is
//!   409 and names them, so the interface can ask first;
//! * every module that changes is written to the audit log in the same
//!   transaction.

use std::collections::BTreeSet;

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    routing::{get, put},
};
use dhpanel_audit::{AuditEntry, Outcome};
use dhpanel_database::{modules as db, sqlx::PgPool};
use dhpanel_http_kit::{AppError, ProblemDetails};
use dhpanel_kernel::{Event, ModuleMount};
use dhpanel_rbac::{
    Require, RequireRecent,
    permissions::{ModulesManage, ModulesRead},
};
use dhpanel_session::SessionStore;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

/// The system module's manifest text.
pub const MANIFEST: &str = include_str!("../../../../modules/system/module.toml");

/// Dependencies of the system handlers.
#[derive(Clone)]
pub struct SystemState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// The live module routes; also knows the modules of this build.
    pub mount: ModuleMount,
}

impl FromRef<SystemState> for PgPool {
    fn from_ref(s: &SystemState) -> Self {
        s.db.clone()
    }
}

impl FromRef<SystemState> for SessionStore {
    fn from_ref(s: &SystemState) -> Self {
        s.sessions.clone()
    }
}

/// Routes with full paths; merge into the application router.
pub fn router() -> Router<SystemState> {
    Router::new()
        .route("/api/v1/admin/modules", get(list_modules))
        .route("/api/v1/admin/modules/{name}", put(set_module))
}

/// A module as shown to administrators.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleView {
    /// Module name.
    #[schema(example = "domains")]
    pub name: String,
    /// Display name by language code.
    #[schema(value_type = Object)]
    pub title: serde_json::Value,
    /// Version of the build.
    pub version: String,
    /// `core`, `web`, `db`, `mail`, `files`, `security`, `system` or `apps`.
    pub category: String,
    /// Cannot be switched off.
    pub core: bool,
    /// Switched on by an administrator.
    pub enabled: bool,
    /// Part of the running build. A module that is not is listed for history
    /// and cannot be switched.
    pub present: bool,
    /// Its routes are being served right now (switched on, with everything it needs).
    pub active: bool,
    /// Modules it needs, directly.
    pub depends_on: Vec<String>,
    /// Modules that need it, directly or indirectly: switching it off stops these too.
    pub dependents: Vec<String>,
}

/// All modules.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleList {
    /// By name.
    pub items: Vec<ModuleView>,
}

/// A change of a module's switch.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetModule {
    /// On or off.
    pub enabled: bool,
    /// Also switch what has to change with it: when switching off, the modules
    /// that need this one; when switching on, the modules this one needs.
    #[serde(default)]
    pub cascade: bool,
}

/// What a switch changed.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleChange {
    /// Modules whose switch changed, including the one asked for (empty when
    /// it already was in the requested state).
    pub changed: Vec<String>,
    /// Modules whose routes are being served after the change.
    pub active: Vec<String>,
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// List the modules of this installation.
#[utoipa::path(
    get,
    path = "/api/v1/admin/modules",
    tag = "system",
    operation_id = "listModules",
    security(("session" = [])),
    responses(
        (status = 200, description = "All modules, by name", body = ModuleList),
        (status = 403, description = "`permission_denied` (needs admin.modules.read) or `mfa_setup_required`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list_modules(
    State(state): State<SystemState>,
    _who: Require<ModulesRead>,
) -> Result<Json<ModuleList>, AppError> {
    let rows = db::list(&state.db).await.map_err(internal)?;
    let active = state.mount.mounted();
    let registry = state.mount.registry();
    let items = rows
        .into_iter()
        .map(|row| {
            let manifest = registry.get(&row.name).map(|m| m.manifest());
            ModuleView {
                title: row
                    .manifest
                    .pointer("/module/title")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
                depends_on: manifest
                    .map(|m| m.module.depends_on.clone())
                    .unwrap_or_default(),
                dependents: registry
                    .dependents_of(&row.name)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                active: active.contains(&row.name),
                name: row.name,
                version: row.version,
                category: row.category,
                core: row.core,
                enabled: row.enabled,
                present: row.present,
            }
        })
        .collect();
    Ok(Json(ModuleList { items }))
}

/// Switch a module on or off. Takes effect at once, without a restart.
#[utoipa::path(
    put,
    path = "/api/v1/admin/modules/{name}",
    tag = "system",
    operation_id = "setModule",
    security(("session" = [])),
    params(
        ("name" = String, Path, description = "Module name"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    request_body = SetModule,
    responses(
        (status = 200, description = "Done (also when nothing had to change)", body = ModuleChange),
        (status = 403, description = "`permission_denied` (needs admin.modules.manage), `reauth_required` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such module in this build", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`module_is_core`; `module_has_dependents` (switching off: the detail names the modules that need it); `module_needs_dependencies` (switching on: the detail names the modules it needs). Repeat with `cascade` to switch those too.", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn set_module(
    State(state): State<SystemState>,
    RequireRecent(who, _): RequireRecent<ModulesManage>,
    Path(name): Path<String>,
    Json(change): Json<SetModule>,
) -> Result<Json<ModuleChange>, AppError> {
    let registry = state.mount.registry();
    let Some(module) = registry.get(&name) else {
        return Err(AppError::not_found("module"));
    };
    if module.manifest().module.core && !change.enabled {
        return Err(AppError::conflict(
            "module_is_core",
            "This module is part of the core and cannot be switched off.",
        ));
    }

    let mut tx = state.db.begin().await.map_err(internal)?;
    db::lock(&mut tx).await.map_err(internal)?;
    let enabled: BTreeSet<String> = db::enabled_names_in(&mut tx)
        .await
        .map_err(internal)?
        .into_iter()
        .collect();

    // What else has to change for the result to make sense.
    let others: Vec<String> = if change.enabled {
        registry
            .dependencies_of(&name)
            .into_iter()
            .filter(|dep| !enabled.contains(*dep))
            .map(str::to_owned)
            .collect()
    } else {
        registry
            .dependents_of(&name)
            .into_iter()
            .filter(|dep| enabled.contains(*dep))
            .map(str::to_owned)
            .collect()
    };
    if !others.is_empty() && !change.cascade {
        let list = others.join(", ");
        return Err(if change.enabled {
            AppError::conflict(
                "module_needs_dependencies",
                format!("This module needs modules that are switched off: {list}."),
            )
        } else {
            AppError::conflict(
                "module_has_dependents",
                format!("Modules that are switched on need this module: {list}."),
            )
        });
    }
    if !change.enabled
        && let Some(core) = others
            .iter()
            .find(|n| registry.get(n).is_some_and(|m| m.manifest().module.core))
    {
        // Cannot happen with a valid registry (core never depends on optional), but
        // the consequence would be switching off a core module, so check anyway.
        return Err(AppError::conflict(
            "module_is_core",
            format!("The core module {core} needs this module."),
        ));
    }

    let mut names = others;
    names.push(name.clone());
    let changed = db::set_enabled(&mut tx, &names, change.enabled)
        .await
        .map_err(internal)?;
    for module in &changed {
        dhpanel_audit::insert(
            &mut *tx,
            &AuditEntry::new(
                if change.enabled {
                    "admin.module_enabled"
                } else {
                    "admin.module_disabled"
                },
                Outcome::Success,
            )
            .actor(who.account_id())
            .target("module", module)
            .metadata(serde_json::json!({ "requested": name, "cascade": change.cascade })),
        )
        .await
        .map_err(internal)?;
        // Announced with the change: if the transaction fails, nobody hears of it.
        let event = if change.enabled {
            Event::ModuleEnabled {
                name: module.clone(),
            }
        } else {
            Event::ModuleDisabled {
                name: module.clone(),
            }
        };
        dhpanel_events::publish(&mut tx, &event)
            .await
            .map_err(internal)?;
    }
    let now_enabled: BTreeSet<String> = db::enabled_names_in(&mut tx)
        .await
        .map_err(internal)?
        .into_iter()
        .collect();
    tx.commit().await.map_err(internal)?;

    // After the commit: the routes follow the database, never lead it.
    if state.mount.apply(&now_enabled) {
        tracing::info!(module = %name, enabled = change.enabled, changed = ?changed, "module routes swapped");
    }
    Ok(Json(ModuleChange {
        changed,
        active: state.mount.mounted().into_iter().collect(),
    }))
}

/// OpenAPI fragment (full paths; merge into the root document).
#[derive(OpenApi)]
#[openapi(
    paths(list_modules, set_module),
    components(schemas(ModuleView, ModuleList, SetModule, ModuleChange)),
    tags((name = "system", description = "Modules and settings"))
)]
pub struct SystemApi;
