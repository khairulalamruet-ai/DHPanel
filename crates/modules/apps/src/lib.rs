//! Which application runs where (T12.01).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/apps[?domain_id=]` | the owner |
//! | GET | `/api/v1/client/apps/{id}` | the owner |
//! | POST | `/api/v1/client/apps/wordpress` | the owner; the `databases` feature |
//! | GET | `/api/v1/admin/tenants/{id}/apps` | `admin.apps.read` |
//!
//! An application is a catalog kind (WordPress, Joomla, a Node.js app, ...) at a place: a
//! domain and a path under it. There is one application per place, so the panel always knows
//! what answers on `example.com/blog`. The installers (T12.02, T12.05) create the rows and
//! move them from `installing` to `active` or `failed`; this module shows them.

use axum::{
    Json, Router,
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    routing::get,
};
use dhpanel_audit::{AuditEntry, Outcome};
use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_database::{
    accounts::DbAccountKind,
    apps as store, databases as dbstore,
    operations::{self, EnqueueError, NewOperation},
    plans,
    sqlx::PgPool,
    tenants, users,
};
use dhpanel_domain::TenantUsername;
use dhpanel_http_kit::{AppError, FieldError, ProblemDetails};
use dhpanel_node_protocol::{DbPassword, Operation};
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
    /// Seals the passwords an install needs.
    pub master_key: MasterKey,
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
        .route(
            "/api/v1/client/apps/wordpress",
            axum::routing::post(install_wordpress),
        )
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

/// A WordPress site to install.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallWordPress {
    /// The domain it is served on (one of the account's).
    pub domain_id: Uuid,
    /// The path under the domain (`/` when absent, `/blog`).
    #[serde(default)]
    pub path: Option<String>,
    /// The site's title.
    pub title: String,
    /// The first administrator's login (3 to 60: letters, digits, `_`, `.`, `-`).
    pub admin_user: String,
    /// Their email address.
    pub admin_email: String,
    /// Their password (12 to 128 characters).
    pub admin_password: String,
    /// A locale such as `bn_BD` (`en_US` when absent).
    #[serde(default)]
    pub locale: Option<String>,
    /// Plugins of wordpress.org to install and switch on (at most 10).
    #[serde(default)]
    pub plugins: Vec<String>,
}

impl std::fmt::Debug for InstallWordPress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallWordPress")
            .field("domain_id", &self.domain_id)
            .field("path", &self.path)
            .field("admin_password", &"..")
            .finish_non_exhaustive()
    }
}

const OPERATION_TTL_SECS: i32 = 6 * 3600;
const OPERATION_ATTEMPTS: i32 = 5;

fn plain(text: &str, min: usize, max: usize, extra: &str) -> bool {
    (min..=max).contains(&text.len())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

/// A strong password made here (about 100 bits).
fn generate_password() -> String {
    loop {
        let a = Uuid::new_v4().simple().to_string();
        let b = Uuid::new_v4().simple().to_string();
        let candidate = format!("{}-{}", a[..10].to_uppercase(), &b[..14]);
        if dhpanel_domain::databases::check_password(&candidate).is_ok() {
            return candidate;
        }
    }
}

fn bad(field: &'static str, message: &str) -> AppError {
    AppError::validation(vec![FieldError::new(field, "invalid", message)])
}

/// Install WordPress on a domain (and path) of the account: a database and a user of its own
/// are made, then WordPress is downloaded, checked, configured and installed on the node. The
/// answer is the application, `installing`; it turns `active` (or `failed`, with the reason)
/// within a minute or two.
#[utoipa::path(post, path = "/api/v1/client/apps/wordpress", tag = "apps", operation_id = "clientInstallWordPress",
    security(("session" = [])), params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = InstallWordPress,
    responses((status = 202, description = "Queued", body = AppView),
              (status = 403, description = "`client_only` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 409, description = "`place_taken`, `limit_reached` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 422, description = "A value that is not valid, or a domain that is not the account's", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn install_wordpress(
    State(state): State<AppsState>,
    who: Principal,
    Json(req): Json<InstallWordPress>,
) -> Result<(StatusCode, Json<AppView>), AppError> {
    let tenant = client_tenant(&state, &who).await?;
    if tenant.status.as_str() != "active" {
        return Err(AppError::conflict(
            "tenant_not_running",
            "Only a running account can install applications.",
        ));
    }
    let plan = plans::get(&state.db, tenant.plan_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal(std::io::Error::other("the plan of a tenant vanished")))?;
    if !plans::includes_feature(&state.db, plan.id, "databases")
        .await
        .map_err(internal)?
    {
        return Err(AppError::forbidden(
            "feature_disabled",
            "Databases are not included in this plan.",
        ));
    }
    // What was asked for.
    let path = req
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("/");
    let folder = path.trim_start_matches('/');
    if !path.starts_with('/')
        || path.len() > 200
        || (!folder.is_empty()
            && !folder
                .split('/')
                .all(|part| plain(part, 1, 100, "._-") && part != "." && part != ".."))
    {
        return Err(bad("path", "A path such as / or /blog."));
    }
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 100 || title.chars().any(char::is_control) {
        return Err(bad("title", "1 to 100 characters."));
    }
    if !plain(&req.admin_user, 3, 60, "_.-") {
        return Err(bad("admin_user", "3 to 60 letters, digits, _ . -"));
    }
    let email = req.admin_email.trim();
    if !email.contains('@') || !plain(email, 3, 200, "@._+-") {
        return Err(bad("admin_email", "An email address."));
    }
    if req.admin_password.chars().count() < 12
        || req.admin_password.len() > 128
        || req.admin_password.chars().any(char::is_control)
    {
        return Err(bad("admin_password", "12 to 128 characters."));
    }
    if req.locale.as_deref().is_some_and(|l| !plain(l, 2, 10, "_"))
        || req.plugins.len() > 10
        || req.plugins.iter().any(|p| !plain(p, 1, 100, "-"))
    {
        return Err(bad(
            "plugins",
            "A locale such as bn_BD; up to 10 plugin slugs.",
        ));
    }
    let fqdn = dhpanel_database::sqlx::query_scalar::<_, String>(
        "SELECT fqdn FROM domains WHERE id = $1 AND tenant_id = $2",
    )
    .bind(req.domain_id)
    .bind(tenant.id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?
    .ok_or_else(|| bad("domain_id", "Not one of your domains."))?;
    let url = if folder.is_empty() {
        format!("https://{fqdn}")
    } else {
        format!("https://{fqdn}/{folder}")
    };
    let tenant_name = TenantUsername::parse(&tenant.username).map_err(internal)?;
    let stem = format!("{}_wp", tenant_name.as_str());
    let number = store::next_name_number(&state.db, tenant.id, &stem)
        .await
        .map_err(internal)?;
    let name = format!("{stem}{number}");
    if name.len() > dhpanel_domain::databases::MAX_NAME_LEN {
        return Err(bad(
            "domain_id",
            "The account's name is too long for a database name.",
        ));
    }
    let settings = serde_json::json!({
        "stage": "setup",
        "folder": folder,
        "url": url,
        "title": title,
        "admin_user": req.admin_user,
        "admin_email": email,
        "locale": req.locale,
        "plugins": req.plugins,
        "database": name,
    });
    let app_id = store::create(
        &state.db,
        &store::NewApp {
            tenant_id: tenant.id,
            domain_id: Some(req.domain_id),
            path: if folder.is_empty() { "/" } else { path },
            kind: "wordpress",
            name: title,
            version: None,
            database_id: None,
            settings: &settings,
        },
    )
    .await
    .map_err(|e| match e {
        store::CreateError::Taken => {
            AppError::conflict("place_taken", "Something is installed there already.")
        }
        store::CreateError::NotOwned => bad("domain_id", "Not one of your domains."),
        store::CreateError::Database(db) => internal(db),
    })?;
    let result = queue_setup(
        &state,
        &who,
        &tenant,
        &plan,
        app_id,
        &name,
        &req.admin_password,
    )
    .await;
    if let Err(err) = result {
        let _ = store::set_status(
            &state.db,
            app_id,
            "failed",
            None,
            Some("could not be set up"),
        )
        .await;
        return Err(err);
    }
    let app = store::get(&state.db, tenant.id, app_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("app"))?;
    Ok((StatusCode::ACCEPTED, Json(view(app))))
}

/// The database, its user, the sealed passwords and the first operations, in one transaction.
async fn queue_setup(
    state: &AppsState,
    who: &Principal,
    tenant: &tenants::Tenant,
    plan: &plans::Plan,
    app_id: Uuid,
    name: &str,
    admin_password: &str,
) -> Result<(), AppError> {
    let limit = |key: &str| {
        let value = plan
            .limits
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(10);
        (value >= 0).then_some(value)
    };
    let mut tx = state.db.begin().await.map_err(internal)?;
    dhpanel_database::domains::lock_tenant(&mut tx, tenant.id)
        .await
        .map_err(internal)?;
    if let Some(max) = limit("max_databases")
        && dbstore::count(&mut tx, tenant.id).await.map_err(internal)? >= max
    {
        return Err(AppError::conflict(
            "limit_reached",
            format!("This plan allows {max} databases."),
        ));
    }
    if let Some(max) = limit("max_db_users")
        && dbstore::count_users(&mut tx, tenant.id)
            .await
            .map_err(internal)?
            >= max
    {
        return Err(AppError::conflict(
            "limit_reached",
            format!("This plan allows {max} database users."),
        ));
    }
    let taken = |e: dbstore::AddError| match e {
        dbstore::AddError::Taken => AppError::conflict("name_taken", "The database name is taken."),
        dbstore::AddError::Database(db) => internal(db),
    };
    let db = dbstore::create(&mut tx, tenant.id, name)
        .await
        .map_err(taken)?;
    let user = dbstore::create_user(&mut tx, tenant.id, name)
        .await
        .map_err(taken)?;
    let password = generate_password();
    let seal = |context: Vec<u8>, text: &str| {
        state
            .master_key
            .seal(&context, text.as_bytes())
            .map_err(internal)
    };
    dbstore::keep_password(
        &mut tx,
        user.id,
        &seal(dbstore::password_context(user.id), &password)?,
    )
    .await
    .map_err(internal)?;
    store::keep_secret(
        &mut tx,
        app_id,
        "db_password",
        &seal(store::secret_context(app_id, "db_password"), &password)?,
    )
    .await
    .map_err(internal)?;
    store::keep_secret(
        &mut tx,
        app_id,
        "admin_password",
        &seal(
            store::secret_context(app_id, "admin_password"),
            admin_password,
        )?,
    )
    .await
    .map_err(internal)?;
    let queue = |op: Operation, key: String| {
        let payload = serde_json::to_value(&op);
        (op, key, payload)
    };
    for (op, key, payload) in [
        queue(
            Operation::CreateDatabase {
                name: name.to_owned(),
            },
            format!("dbcreate:{}", db.id.simple()),
        ),
        queue(
            Operation::UpsertDbUser {
                username: tenant.username.clone(),
                name: name.to_owned(),
                password: None::<DbPassword>,
            },
            format!("dbuser:{}:{}", user.id.simple(), Uuid::now_v7().simple()),
        ),
    ] {
        let payload = payload.map_err(internal)?;
        operations::enqueue_in(
            &mut tx,
            &NewOperation {
                node_id: tenant.node_id,
                kind: op.type_name(),
                tenant_id: Some(tenant.id),
                payload: &payload,
                idempotency_key: &key,
                created_by: Some(who.account_id()),
                ttl_secs: OPERATION_TTL_SECS,
                max_attempts: OPERATION_ATTEMPTS,
            },
        )
        .await
        .map_err(|e| match e {
            EnqueueError::UnknownNode => AppError::conflict(
                "node_unavailable",
                "The node of this account no longer exists.",
            ),
            other => internal(other),
        })?;
    }
    dhpanel_database::sqlx::query(
        "UPDATE apps SET database_id = $2, settings = settings || jsonb_build_object('db_id', $2::text, 'user_id', $3::text) WHERE id = $1",
    )
    .bind(app_id)
    .bind(db.id)
    .bind(user.id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    dhpanel_audit::insert(
        &mut *tx,
        &AuditEntry::new("app.install_requested", Outcome::Success)
            .actor(who.account_id())
            .tenant(tenant.id)
            .target("app", app_id)
            .metadata(serde_json::json!({ "kind": "wordpress", "database": name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(())
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(client_list, client_get, admin_list, install_wordpress),
    components(schemas(AppView, AppList, InstallWordPress)),
    tags((name = "apps", description = "Applications on domains and paths"))
)]
pub struct AppsApi;
