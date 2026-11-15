//! Which application runs where (T12.01).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/apps[?domain_id=]` | the owner |
//! | GET | `/api/v1/client/apps/{id}` | the owner |
//! | GET | `/api/v1/admin/tenants/{id}/apps` | `admin.apps.read` |
//!
//! An application is a catalog kind (WordPress, Joomla, a Node.js app, ...) at a place: a
//! domain and a path under it. There is one application per place, so the panel always knows
//! what answers on `example.com/blog`. The installers (T12.02, T12.05) create the rows and
//! move them from `installing` to `active` or `failed`; this module shows them.

use axum::{
    Json, Router,
    extract::{FromRef, Path, Query, State},
    routing::get,
};
use dhpanel_database::{accounts::DbAccountKind, apps as store, sqlx::PgPool, tenants, users};
use dhpanel_http_kit::{AppError, ProblemDetails};
use dhpanel_rbac::{Principal, Require, permissions::AppsRead};
use dhpanel_session::SessionStore;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/apps/module.toml");

/// What the module's handlers share.
#[derive(Clone)]
pub struct AppsState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
}

impl FromRef<AppsState> for PgPool {
    fn from_ref(s: &AppsState) -> Self {
        s.db.clone()
    }
}

impl FromRef<AppsState> for SessionStore {
    fn from_ref(s: &AppsState) -> Self {
        s.sessions.clone()
    }
}

/// Routes for hosting accounts.
pub fn client_router() -> Router<AppsState> {
    Router::new()
        .route("/api/v1/client/apps", get(client_list))
        .route("/api/v1/client/apps/{id}", get(client_get))
}

/// Routes for administrators.
pub fn admin_router() -> Router<AppsState> {
    Router::new().route("/api/v1/admin/tenants/{id}/apps", get(admin_list))
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// One application.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppView {
    /// Id.
    pub id: Uuid,
    /// The domain it is served on (`null`: the domain was removed).
    pub domain_id: Option<Uuid>,
    /// The domain's name.
    pub fqdn: Option<String>,
    /// The path under the domain (`/`, `/blog`).
    pub path: String,
    /// The catalog slug (`wordpress`).
    pub kind: String,
    /// What the owner calls it.
    pub name: String,
    /// The version installed.
    pub version: Option<String>,
    /// `installing`, `active`, `failed` or `removing`.
    pub status: String,
    /// The database it uses.
    pub database_id: Option<Uuid>,
    /// Why an install failed.
    pub error: Option<String>,
    /// Created (Unix seconds).
    pub created_at: i64,
}

/// The applications.
#[derive(Debug, Serialize, ToSchema)]
pub struct AppList {
    /// Oldest first.
    pub items: Vec<AppView>,
}

fn view(a: store::App) -> AppView {
    AppView {
        id: a.id,
        domain_id: a.domain_id,
        fqdn: a.fqdn,
        path: a.path,
        kind: a.kind,
        name: a.name,
        version: a.version,
        status: a.status,
        database_id: a.database_id,
        error: a.error,
        created_at: a.created_at,
    }
}

/// Which applications to list.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
    /// Only those served on this domain.
    pub domain_id: Option<Uuid>,
}

async fn client_tenant(state: &AppsState, who: &Principal) -> Result<tenants::Tenant, AppError> {
    if who.kind != DbAccountKind::Client {
        return Err(AppError::forbidden(
            "client_only",
            "This is for hosting accounts.",
        ));
    }
    let user = users::get(&state.db, who.account_id())
        .await
        .map_err(internal)?;
    let owned = user.and_then(|u| u.tenant).ok_or_else(|| {
        AppError::conflict("no_hosting", "This account has no hosting account yet.")
    })?;
    tenants::get(&state.db, owned.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))
}

/// The account's applications.
#[utoipa::path(get, path = "/api/v1/client/apps", tag = "apps", operation_id = "clientListApps",
    security(("session" = [])), params(ListQuery),
    responses((status = 200, description = "The applications", body = AppList),
              (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn client_list(
    State(state): State<AppsState>,
    who: Principal,
    Query(q): Query<ListQuery>,
) -> Result<Json<AppList>, AppError> {
    let tenant = client_tenant(&state, &who).await?;
    let items = store::list(&state.db, tenant.id, q.domain_id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(view)
        .collect();
    Ok(Json(AppList { items }))
}

/// One of the account's applications.
#[utoipa::path(get, path = "/api/v1/client/apps/{id}", tag = "apps", operation_id = "clientGetApp",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Application id")),
    responses((status = 200, description = "The application", body = AppView),
              (status = 404, description = "Not the account's", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn client_get(
    State(state): State<AppsState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<AppView>, AppError> {
    let tenant = client_tenant(&state, &who).await?;
    store::get(&state.db, tenant.id, id)
        .await
        .map_err(internal)?
        .map(|a| Json(view(a)))
        .ok_or_else(|| AppError::not_found("app"))
}

/// A hosting account's applications.
#[utoipa::path(get, path = "/api/v1/admin/tenants/{id}/apps", tag = "apps", operation_id = "adminListApps",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Hosting account id")),
    responses((status = 200, description = "The applications", body = AppList),
              (status = 404, description = "No such hosting account", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_list(
    State(state): State<AppsState>,
    _who: Require<AppsRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<AppList>, AppError> {
    tenants::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("tenant"))?;
    let items = store::list(&state.db, id, None)
        .await
        .map_err(internal)?
        .into_iter()
        .map(view)
        .collect();
    Ok(Json(AppList { items }))
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(client_list, client_get, admin_list),
    components(schemas(AppView, AppList)),
    tags((name = "apps", description = "Applications on domains and paths"))
)]
pub struct AppsApi;
