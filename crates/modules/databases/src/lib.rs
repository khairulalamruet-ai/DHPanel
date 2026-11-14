//! Databases, database users and grants of a hosting account (T07.06).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET, POST | `/api/v1/client/databases` | a client; the `databases` feature to add |
//! | DELETE | `/api/v1/client/databases/{id}` | the owner |
//! | PUT, DELETE | `/api/v1/client/databases/{id}/grants/{user_id}` | the owner |
//! | GET, POST | `/api/v1/client/db-users` | a client; the `databases` feature to add |
//! | PUT | `/api/v1/client/db-users/{id}/password` | the owner |
//! | DELETE | `/api/v1/client/db-users/{id}` | the owner |
//! | GET, PUT | `/api/v1/client/db-users/{id}/remote-hosts` | the owner; the `db_remote` feature to change |
//! | POST, GET, PUT | `.../databases/{id}/imports`, `/api/v1/client/imports...` | SQL imports, see [`imports`] |
//! | POST, GET, DELETE | `.../databases/{id}/exports`, `/api/v1/client/exports...` | SQL exports, see [`exports`] |
//!
//! Names start with the account's system name and an underscore (the setting
//! `require_prefix`). The plan limits how many databases (`max_databases`) and
//! users (`max_db_users`) an account has. Everything is done on the node in the
//! background: a database or user is `pending` until the node has made it, then
//! `ready`, or `failed`. A user's password is shown once, when it is made or
//! changed (a given password is checked; without one a strong one is made); it
//! is kept sealed only until the node has it and never in the ledger or the log.

pub mod exports;
pub mod imports;
pub mod phpmyadmin;

use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::{delete, get, put},
};
use dhpanel_audit::{AuditEntry, Outcome};
use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_database::{
    accounts::DbAccountKind,
    databases::{self as store, AddError, Db, DbUser},
    operations::{self, EnqueueError, NewOperation},
    plans::{self, Plan},
    sqlx::PgPool,
    tenants, users,
};
use dhpanel_domain::databases::{Access, check_password, full_name, host_for_remote};
use dhpanel_http_kit::{AppError, FieldError, ProblemDetails};
use dhpanel_node_protocol::{DbPassword, MAX_DB_REMOTE_HOSTS, Operation};
use dhpanel_rbac::Principal;
use dhpanel_session::{SessionStore, rfc3339};
use dhpanel_settings::{Scope, SettingsRegistry};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/databases/module.toml");
/// The module's settings schema.
pub const SETTINGS_SCHEMA: &str =
    include_str!("../../../../modules/databases/settings.schema.json");

const OPERATION_TTL_SECS: i32 = 6 * 3600;
const OPERATION_ATTEMPTS: i32 = 5;

/// What the module's handlers share.
#[derive(Clone)]
pub struct DatabasesState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// The checked settings schemas of all modules.
    pub settings: Arc<SettingsRegistry>,
    /// Seals the passwords of database users until the node has them.
    pub master_key: MasterKey,
    /// Where uploaded SQL dumps wait for the node (`imports.spool_dir`).
    pub spool_dir: PathBuf,
    /// Where nodes reach this panel (`agent.public_url`); without it nothing can
    /// be imported.
    pub agent_url: Option<String>,
}

impl FromRef<DatabasesState> for PgPool {
    fn from_ref(s: &DatabasesState) -> Self {
        s.db.clone()
    }
}

impl FromRef<DatabasesState> for SessionStore {
    fn from_ref(s: &DatabasesState) -> Self {
        s.sessions.clone()
    }
}

/// Routes for clients, with full paths.
pub fn client_router() -> Router<DatabasesState> {
    Router::new()
        .route(
            "/api/v1/client/databases",
            get(list_databases).post(create_database),
        )
        .route("/api/v1/client/databases/{id}", delete(delete_database))
        .route(
            "/api/v1/client/databases/{id}/grants/{user_id}",
            put(set_grant).delete(remove_grant),
        )
        .route("/api/v1/client/db-users", get(list_users).post(create_user))
        .route("/api/v1/client/db-users/{id}", delete(delete_user))
        .route(
            "/api/v1/client/db-users/{id}/password",
            put(change_password),
        )
        .route(
            "/api/v1/client/db-users/{id}/remote-hosts",
            get(get_remote).put(set_remote),
        )
        .merge(imports::client_router())
        .merge(exports::client_router())
        .merge(phpmyadmin::client_router())
}

/// Routes without a session (the node fetching an SQL dump with its token).
pub fn public_router() -> Router<DatabasesState> {
    imports::public_router().merge(exports::public_router())
}

/// Routes for administrators (the overview arrives with T07.11).
pub fn admin_router() -> Router<DatabasesState> {
    Router::new()
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

struct Hosting {
    tenant: tenants::Tenant,
    plan: Plan,
}

async fn client_hosting(state: &DatabasesState, who: &Principal) -> Result<Hosting, AppError> {
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
    let tenant = tenants::get(&state.db, owned.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    let plan = plans::get(&state.db, tenant.plan_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal(std::io::Error::other("the plan of a tenant vanished")))?;
    Ok(Hosting { tenant, plan })
}

/// What a plan allows of a kind of limit; `None` is unlimited.
fn plan_limit(plan: &Plan, key: &str, default: i64) -> Option<i64> {
    let value = plan
        .limits
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(default);
    (value >= 0).then_some(value)
}

async fn require_feature(
    state: &DatabasesState,
    hosting: &Hosting,
    feature: &'static str,
    what: &'static str,
) -> Result<(), AppError> {
    if plans::includes_feature(&state.db, hosting.plan.id, feature)
        .await
        .map_err(internal)?
    {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "feature_disabled",
            format!("{what} is not included in this plan."),
        ))
    }
}

fn running(hosting: &Hosting) -> Result<(), AppError> {
    if hosting.tenant.status.as_str() == "active" {
        Ok(())
    } else {
        Err(AppError::conflict(
            "tenant_not_running",
            "Only a running account can change its databases.",
        ))
    }
}

async fn prefix_required(state: &DatabasesState) -> Result<bool, AppError> {
    let mut conn = state.db.acquire().await.map_err(internal)?;
    let values = dhpanel_settings::get(&state.settings, &mut conn, "databases", Scope::Global)
        .await
        .map_err(internal)?;
    Ok(values
        .get("require_prefix")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true))
}

fn entry(
    action: &'static str,
    who: &Principal,
    hosting: &Hosting,
    kind: &'static str,
    id: Uuid,
) -> AuditEntry {
    AuditEntry::new(action, Outcome::Success)
        .actor(who.account_id())
        .tenant(hosting.tenant.id)
        .target(kind, id)
}

/// Puts work for the node in the ledger, inside the caller's transaction.
async fn queue(
    tx: &mut dhpanel_database::sqlx::PgConnection,
    hosting: &Hosting,
    op: &Operation,
    key: String,
    who: &Principal,
) -> Result<Uuid, AppError> {
    let payload = serde_json::to_value(op).map_err(internal)?;
    let queued = operations::enqueue_in(
        tx,
        &NewOperation {
            node_id: hosting.tenant.node_id,
            kind: op.type_name(),
            tenant_id: Some(hosting.tenant.id),
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
    Ok(queued.operation().id)
}

fn name_error(err: dhpanel_domain::databases::DbNameError) -> AppError {
    AppError::validation(vec![FieldError::new("name", err.code(), err.to_string())])
}

/// A strong password made here (about 100 bits).
fn generate_password() -> String {
    loop {
        let a = Uuid::new_v4().simple().to_string();
        let b = Uuid::new_v4().simple().to_string();
        let candidate = format!("{}-{}", a[..10].to_uppercase(), &b[..14]);
        if check_password(&candidate).is_ok() {
            return candidate;
        }
    }
}

// ------------------------------------------------------------------- views

/// A user's access to a database.
#[derive(Debug, Serialize, ToSchema)]
pub struct GrantView {
    /// The user.
    pub user_id: Uuid,
    /// The user's name.
    pub user: String,
    /// `read_only`, `read_write` or `all`.
    #[schema(example = "read_write")]
    pub access: String,
}

/// A database.
#[derive(Debug, Serialize, ToSchema)]
pub struct DatabaseView {
    /// Id.
    pub id: Uuid,
    /// The whole name.
    #[schema(example = "alice_shop")]
    pub name: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub status: String,
    /// Size as last measured, in bytes (measured every few minutes).
    pub size_bytes: i64,
    /// When it was measured (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub size_checked_at: Option<String>,
    /// Who may use it.
    pub users: Vec<GrantView>,
}

/// The databases of an account.
#[derive(Debug, Serialize, ToSchema)]
pub struct DatabaseList {
    /// By name.
    pub items: Vec<DatabaseView>,
    /// Most databases the plan allows (`null`: no limit).
    pub limit: Option<i64>,
    /// What every name starts with (`alice_`), or empty when names are as typed.
    pub prefix: String,
}

/// A database user's access to one database.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserGrantView {
    /// The database.
    pub database_id: Uuid,
    /// The database's name.
    pub database: String,
    /// The access level.
    pub access: String,
}

/// A database user.
#[derive(Debug, Serialize, ToSchema)]
pub struct DbUserView {
    /// Id.
    pub id: Uuid,
    /// The whole name.
    #[schema(example = "alice_web")]
    pub name: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub status: String,
    /// What it may use.
    pub databases: Vec<UserGrantView>,
    /// Addresses it may connect from besides its own network.
    pub remote_hosts: Vec<String>,
}

/// The database users of an account.
#[derive(Debug, Serialize, ToSchema)]
pub struct DbUserList {
    /// By name.
    pub items: Vec<DbUserView>,
    /// Most users the plan allows (`null`: no limit).
    pub limit: Option<i64>,
}

/// A user that was made or whose password was changed.
#[derive(Debug, Serialize, ToSchema)]
pub struct DbUserCreated {
    /// The user.
    pub user: DbUserView,
    /// The password, shown this once (only when it was made here).
    pub password: Option<String>,
}

async fn database_views(
    state: &DatabasesState,
    tenant_id: Uuid,
    rows: Vec<Db>,
) -> Result<Vec<DatabaseView>, AppError> {
    let users = store::users(&state.db, tenant_id).await.map_err(internal)?;
    let grants = store::grants(&state.db, tenant_id)
        .await
        .map_err(internal)?;
    Ok(rows
        .into_iter()
        .map(|d| DatabaseView {
            users: grants
                .iter()
                .filter(|g| g.database_id == d.id)
                .filter_map(|g| {
                    users.iter().find(|u| u.id == g.user_id).map(|u| GrantView {
                        user_id: u.id,
                        user: u.name.clone(),
                        access: g.access.clone(),
                    })
                })
                .collect(),
            id: d.id,
            name: d.name,
            status: d.status,
            size_bytes: d.size_bytes,
            size_checked_at: d.size_checked_at.map(rfc3339),
        })
        .collect())
}

async fn user_views(
    state: &DatabasesState,
    tenant_id: Uuid,
    rows: Vec<DbUser>,
) -> Result<Vec<DbUserView>, AppError> {
    let dbs = store::list(&state.db, tenant_id).await.map_err(internal)?;
    let grants = store::grants(&state.db, tenant_id)
        .await
        .map_err(internal)?;
    let mut out = Vec::new();
    for u in rows {
        out.push(DbUserView {
            databases: grants
                .iter()
                .filter(|g| g.user_id == u.id)
                .filter_map(|g| {
                    dbs.iter()
                        .find(|d| d.id == g.database_id)
                        .map(|d| UserGrantView {
                            database_id: d.id,
                            database: d.name.clone(),
                            access: g.access.clone(),
                        })
                })
                .collect(),
            remote_hosts: store::remote_hosts(&state.db, u.id)
                .await
                .map_err(internal)?,
            id: u.id,
            name: u.name,
            status: u.status,
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------- inputs

/// A database to make.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddDatabase {
    /// The name; the account's prefix is put in front when it is not there.
    #[schema(example = "shop")]
    pub name: String,
}

/// A database user to make.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddDbUser {
    /// The name; the account's prefix is put in front when it is not there.
    #[schema(example = "web")]
    pub name: String,
    /// The password; a strong one is made (and shown once) when it is left out.
    pub password: Option<String>,
}

/// A new password.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPassword {
    /// The password; a strong one is made (and shown once) when it is left out.
    pub password: Option<String>,
}

/// An access level.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetAccess {
    /// `read_only`, `read_write` or `all`.
    #[schema(example = "read_write")]
    pub access: String,
}

/// Where a user may connect from.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoteHosts {
    /// IPv4 addresses or ranges, `203.0.113.7/32` or `198.51.100.0/24` (at most 20,
    /// none wider than a /8).
    pub cidrs: Vec<String>,
}

// ---------------------------------------------------------------- databases

/// The signed-in client's databases.
#[utoipa::path(
    get,
    path = "/api/v1/client/databases",
    tag = "databases",
    operation_id = "clientListDatabases",
    security(("session" = [])),
    responses(
        (status = 200, description = "The databases", body = DatabaseList),
        (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list_databases(
    State(state): State<DatabasesState>,
    who: Principal,
) -> Result<Json<DatabaseList>, AppError> {
    let h = client_hosting(&state, &who).await?;
    let rows = store::list(&state.db, h.tenant.id)
        .await
        .map_err(internal)?;
    Ok(Json(DatabaseList {
        items: database_views(&state, h.tenant.id, rows).await?,
        limit: plan_limit(&h.plan, "max_databases", 10),
        prefix: if prefix_required(&state).await? {
            format!("{}_", h.tenant.username)
        } else {
            String::new()
        },
    }))
}

/// Make a database.
#[utoipa::path(
    post,
    path = "/api/v1/client/databases",
    tag = "databases",
    operation_id = "clientCreateDatabase",
    security(("session" = [])),
    params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = AddDatabase,
    responses(
        (status = 201, description = "Queued; `status` becomes `ready` when the node has made it", body = DatabaseView),
        (status = 403, description = "`client_only`, `csrf_failed` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`limit_reached`, `name_taken` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "`invalid_db_name`, `db_name_too_long` or `reserved_db_name`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn create_database(
    State(state): State<DatabasesState>,
    who: Principal,
    Json(req): Json<AddDatabase>,
) -> Result<(StatusCode, Json<DatabaseView>), AppError> {
    let h = client_hosting(&state, &who).await?;
    require_feature(&state, &h, "databases", "Databases are").await?;
    running(&h)?;
    let tenant_name =
        dhpanel_domain::TenantUsername::parse(&h.tenant.username).map_err(internal)?;
    let name =
        full_name(&tenant_name, &req.name, prefix_required(&state).await?).map_err(name_error)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    dhpanel_database::domains::lock_tenant(&mut tx, h.tenant.id)
        .await
        .map_err(internal)?;
    if let Some(limit) = plan_limit(&h.plan, "max_databases", 10)
        && store::count(&mut tx, h.tenant.id).await.map_err(internal)? >= limit
    {
        return Err(AppError::conflict(
            "limit_reached",
            format!("This plan allows {limit} databases."),
        ));
    }
    let made = store::create(&mut tx, h.tenant.id, &name)
        .await
        .map_err(|e| match e {
            AddError::Taken => AppError::conflict("name_taken", "This name is taken."),
            AddError::Database(db) => internal(db),
        })?;
    queue(
        &mut tx,
        &h,
        &Operation::CreateDatabase { name: name.clone() },
        format!("dbcreate:{}", made.id.simple()),
        &who,
    )
    .await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("database.created", &who, &h, "database", made.id)
            .metadata(serde_json::json!({ "name": name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let view = database_views(&state, h.tenant.id, vec![made])
        .await?
        .remove(0);
    Ok((StatusCode::CREATED, Json(view)))
}

/// Remove a database and everything in it.
#[utoipa::path(
    delete,
    path = "/api/v1/client/databases/{id}",
    tag = "databases",
    operation_id = "clientDeleteDatabase",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 202, description = "Queued; the database disappears from the list when the node has removed it"),
        (status = 403, description = "`client_only` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such database (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn delete_database(
    State(state): State<DatabasesState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let h = client_hosting(&state, &who).await?;
    running(&h)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    let gone = store::mark_deleting(&mut tx, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("database"))?;
    queue(
        &mut tx,
        &h,
        &Operation::DropDatabase {
            name: gone.name.clone(),
        },
        format!("dbdrop:{}", gone.id.simple()),
        &who,
    )
    .await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("database.deleted", &who, &h, "database", id)
            .metadata(serde_json::json!({ "name": gone.name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::ACCEPTED)
}

// -------------------------------------------------------------------- users

/// The signed-in client's database users.
#[utoipa::path(
    get,
    path = "/api/v1/client/db-users",
    tag = "databases",
    operation_id = "clientListDbUsers",
    security(("session" = [])),
    responses(
        (status = 200, description = "The users", body = DbUserList),
        (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list_users(
    State(state): State<DatabasesState>,
    who: Principal,
) -> Result<Json<DbUserList>, AppError> {
    let h = client_hosting(&state, &who).await?;
    let rows = store::users(&state.db, h.tenant.id)
        .await
        .map_err(internal)?;
    Ok(Json(DbUserList {
        items: user_views(&state, h.tenant.id, rows).await?,
        limit: plan_limit(&h.plan, "max_db_users", 10),
    }))
}

/// Tells the node which addresses may reach its database server from outside: the
/// union of every user's remote addresses on the node, as the rows stand in this
/// transaction. The node sets its packet filter first and opens the port after.
async fn queue_remote_access(
    tx: &mut dhpanel_database::sqlx::PgConnection,
    h: &Hosting,
    who: &Principal,
) -> Result<(), AppError> {
    let cidrs = store::remote_union(tx, h.tenant.node_id)
        .await
        .map_err(internal)?;
    queue(
        tx,
        h,
        &Operation::SetDbRemoteAccess { cidrs },
        format!(
            "dbaccess:{}:{}",
            h.tenant.node_id.simple(),
            Uuid::now_v7().simple()
        ),
        who,
    )
    .await
    .map(|_| ())
}

/// Seals a password for the dispatcher to deliver and queues the work.
async fn queue_user(
    state: &DatabasesState,
    tx: &mut dhpanel_database::sqlx::PgConnection,
    h: &Hosting,
    who: &Principal,
    user: &DbUser,
    password: &str,
) -> Result<(), AppError> {
    let sealed = state
        .master_key
        .seal(&store::password_context(user.id), password.as_bytes())
        .map_err(internal)?;
    store::keep_password(tx, user.id, &sealed)
        .await
        .map_err(internal)?;
    queue(
        tx,
        h,
        &Operation::UpsertDbUser {
            username: h.tenant.username.clone(),
            name: user.name.clone(),
            // Not in the ledger: the dispatcher opens the sealed one at send time.
            password: None::<DbPassword>,
        },
        format!("dbuser:{}:{}", user.id.simple(), Uuid::now_v7().simple()),
        who,
    )
    .await
    .map(|_| ())
}

/// The password to use: the one given (checked), or a new strong one.
fn password_for(given: Option<String>) -> Result<(String, bool), AppError> {
    match given {
        Some(p) => {
            check_password(&p).map_err(|e| {
                AppError::validation(vec![FieldError::new("password", e.code(), e.to_string())])
            })?;
            Ok((p, false))
        }
        None => Ok((generate_password(), true)),
    }
}

/// Make a database user.
#[utoipa::path(
    post,
    path = "/api/v1/client/db-users",
    tag = "databases",
    operation_id = "clientCreateDbUser",
    security(("session" = [])),
    params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = AddDbUser,
    responses(
        (status = 201, description = "Queued; the password is shown this once if it was made here", body = DbUserCreated),
        (status = 403, description = "`client_only`, `csrf_failed` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`limit_reached`, `name_taken` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "A bad name or password (`password_length`, `password_weak`, ...)", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn create_user(
    State(state): State<DatabasesState>,
    who: Principal,
    Json(req): Json<AddDbUser>,
) -> Result<(StatusCode, Json<DbUserCreated>), AppError> {
    let h = client_hosting(&state, &who).await?;
    require_feature(&state, &h, "databases", "Databases are").await?;
    running(&h)?;
    let tenant_name =
        dhpanel_domain::TenantUsername::parse(&h.tenant.username).map_err(internal)?;
    let name =
        full_name(&tenant_name, &req.name, prefix_required(&state).await?).map_err(name_error)?;
    let (password, generated) = password_for(req.password)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    dhpanel_database::domains::lock_tenant(&mut tx, h.tenant.id)
        .await
        .map_err(internal)?;
    if let Some(limit) = plan_limit(&h.plan, "max_db_users", 10)
        && store::count_users(&mut tx, h.tenant.id)
            .await
            .map_err(internal)?
            >= limit
    {
        return Err(AppError::conflict(
            "limit_reached",
            format!("This plan allows {limit} database users."),
        ));
    }
    let made = store::create_user(&mut tx, h.tenant.id, &name)
        .await
        .map_err(|e| match e {
            AddError::Taken => AppError::conflict("name_taken", "This name is taken."),
            AddError::Database(db) => internal(db),
        })?;
    queue_user(&state, &mut tx, &h, &who, &made, &password).await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("db_user.created", &who, &h, "db_user", made.id)
            .metadata(serde_json::json!({ "name": name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let view = user_views(&state, h.tenant.id, vec![made]).await?.remove(0);
    Ok((
        StatusCode::CREATED,
        Json(DbUserCreated {
            user: view,
            password: generated.then_some(password),
        }),
    ))
}

/// Change a database user's password.
#[utoipa::path(
    put,
    path = "/api/v1/client/db-users/{id}/password",
    tag = "databases",
    operation_id = "clientChangeDbPassword",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "User id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    request_body = NewPassword,
    responses(
        (status = 200, description = "Queued; the password is shown this once if it was made here", body = DbUserCreated),
        (status = 403, description = "`client_only` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such user (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`not_ready` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "A bad password", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn change_password(
    State(state): State<DatabasesState>,
    who: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<NewPassword>,
) -> Result<Json<DbUserCreated>, AppError> {
    let h = client_hosting(&state, &who).await?;
    running(&h)?;
    let user = store::user(&state.db, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    if user.status != "ready" {
        return Err(AppError::conflict(
            "not_ready",
            "The user is not ready yet (or is being removed).",
        ));
    }
    let (password, generated) = password_for(req.password)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    queue_user(&state, &mut tx, &h, &who, &user, &password).await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("db_user.password_changed", &who, &h, "db_user", id)
            .metadata(serde_json::json!({ "name": user.name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let view = user_views(&state, h.tenant.id, vec![user]).await?.remove(0);
    Ok(Json(DbUserCreated {
        user: view,
        password: generated.then_some(password),
    }))
}

/// Remove a database user.
#[utoipa::path(
    delete,
    path = "/api/v1/client/db-users/{id}",
    tag = "databases",
    operation_id = "clientDeleteDbUser",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "User id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 202, description = "Queued"),
        (status = 403, description = "`client_only` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such user (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn delete_user(
    State(state): State<DatabasesState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let h = client_hosting(&state, &who).await?;
    running(&h)?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    let gone = store::mark_user_deleting(&mut tx, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    queue(
        &mut tx,
        &h,
        &Operation::DropDbUser {
            name: gone.name.clone(),
        },
        format!("dbuserdrop:{}", gone.id.simple()),
        &who,
    )
    .await?;
    // What it was allowed from no longer counts for the node's packet filter.
    if !store::remote_hosts(&state.db, gone.id)
        .await
        .map_err(internal)?
        .is_empty()
    {
        queue_remote_access(&mut tx, &h, &who).await?;
    }
    dhpanel_audit::insert(
        &mut *tx,
        &entry("db_user.deleted", &who, &h, "db_user", id)
            .metadata(serde_json::json!({ "name": gone.name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::ACCEPTED)
}

// ------------------------------------------------------------------- grants

async fn ready_pair(
    state: &DatabasesState,
    h: &Hosting,
    database_id: Uuid,
    user_id: Uuid,
) -> Result<(Db, DbUser), AppError> {
    let db = store::get(&state.db, h.tenant.id, database_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("database"))?;
    let user = store::user(&state.db, h.tenant.id, user_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    if db.status != "ready" || user.status != "ready" {
        return Err(AppError::conflict(
            "not_ready",
            "The database and the user must both be ready first.",
        ));
    }
    Ok((db, user))
}

/// Give a user access to a database (replacing the access it had).
#[utoipa::path(
    put,
    path = "/api/v1/client/databases/{id}/grants/{user_id}",
    tag = "databases",
    operation_id = "clientGrantDatabase",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("user_id" = Uuid, Path, description = "User id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    request_body = SetAccess,
    responses(
        (status = 200, description = "Queued", body = DatabaseView),
        (status = 403, description = "`client_only` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such database or user (or not yours)", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`not_ready` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "`unknown_access`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn set_grant(
    State(state): State<DatabasesState>,
    who: Principal,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetAccess>,
) -> Result<Json<DatabaseView>, AppError> {
    let h = client_hosting(&state, &who).await?;
    running(&h)?;
    let access = Access::parse(&req.access).ok_or_else(|| {
        AppError::validation(vec![FieldError::new(
            "access",
            "unknown_access",
            "The access level is read_only, read_write or all.",
        )])
    })?;
    let (db, user) = ready_pair(&state, &h, id, user_id).await?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    store::set_grant(&mut tx, h.tenant.id, db.id, user.id, access.as_str())
        .await
        .map_err(internal)?;
    queue(
        &mut tx,
        &h,
        &Operation::GrantDb {
            database: db.name.clone(),
            user: user.name.clone(),
            access: access.as_str().to_owned(),
        },
        format!(
            "dbgrant:{}:{}:{}",
            db.id.simple(),
            user.id.simple(),
            Uuid::now_v7().simple()
        ),
        &who,
    )
    .await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("database.grant_set", &who, &h, "database", db.id).metadata(
            serde_json::json!({ "database": db.name, "user": user.name, "access": access.as_str() }),
        ),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let fresh = store::get(&state.db, h.tenant.id, db.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("database"))?;
    Ok(Json(
        database_views(&state, h.tenant.id, vec![fresh])
            .await?
            .remove(0),
    ))
}

/// Take a user's access to a database away.
#[utoipa::path(
    delete,
    path = "/api/v1/client/databases/{id}/grants/{user_id}",
    tag = "databases",
    operation_id = "clientRevokeDatabase",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Database id"),
        ("user_id" = Uuid, Path, description = "User id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 202, description = "Queued"),
        (status = 403, description = "`client_only` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such database, user or grant", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn remove_grant(
    State(state): State<DatabasesState>,
    who: Principal,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let h = client_hosting(&state, &who).await?;
    running(&h)?;
    let db = store::get(&state.db, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("database"))?;
    let user = store::user(&state.db, h.tenant.id, user_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    if !store::remove_grant(&mut tx, h.tenant.id, db.id, user.id)
        .await
        .map_err(internal)?
    {
        return Err(AppError::not_found("grant"));
    }
    queue(
        &mut tx,
        &h,
        &Operation::RevokeDb {
            database: db.name.clone(),
            user: user.name.clone(),
        },
        format!(
            "dbrevoke:{}:{}:{}",
            db.id.simple(),
            user.id.simple(),
            Uuid::now_v7().simple()
        ),
        &who,
    )
    .await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("database.grant_removed", &who, &h, "database", db.id)
            .metadata(serde_json::json!({ "database": db.name, "user": user.name })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::ACCEPTED)
}

// ------------------------------------------------------------ remote access

/// The addresses a user may connect from besides its own network.
#[utoipa::path(
    get,
    path = "/api/v1/client/db-users/{id}/remote-hosts",
    tag = "databases",
    operation_id = "clientGetDbRemoteHosts",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "User id")),
    responses(
        (status = 200, description = "The addresses", body = RemoteHosts),
        (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such user (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn get_remote(
    State(state): State<DatabasesState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<RemoteHosts>, AppError> {
    let h = client_hosting(&state, &who).await?;
    store::user(&state.db, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    Ok(Json(RemoteHosts {
        cidrs: store::remote_hosts(&state.db, id).await.map_err(internal)?,
    }))
}

/// Replace the addresses a user may connect from (an empty list ends remote access).
#[utoipa::path(
    put,
    path = "/api/v1/client/db-users/{id}/remote-hosts",
    tag = "databases",
    operation_id = "clientSetDbRemoteHosts",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "User id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    request_body = RemoteHosts,
    responses(
        (status = 200, description = "Saved and queued", body = RemoteHosts),
        (status = 403, description = "`client_only`, `csrf_failed` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such user (or it is not yours)", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`not_ready` or `tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "`invalid_cidr` or `too_many_hosts`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn set_remote(
    State(state): State<DatabasesState>,
    who: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<RemoteHosts>,
) -> Result<Json<RemoteHosts>, AppError> {
    let h = client_hosting(&state, &who).await?;
    require_feature(&state, &h, "db_remote", "Remote database access is").await?;
    running(&h)?;
    let user = store::user(&state.db, h.tenant.id, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    if user.status != "ready" {
        return Err(AppError::conflict(
            "not_ready",
            "The user is not ready yet.",
        ));
    }
    if req.cidrs.len() > MAX_DB_REMOTE_HOSTS {
        return Err(AppError::validation(vec![FieldError::new(
            "cidrs",
            "too_many_hosts",
            format!("At most {MAX_DB_REMOTE_HOSTS} addresses."),
        )]));
    }
    let mut cidrs: Vec<String> = Vec::new();
    for cidr in &req.cidrs {
        let cidr = cidr.trim().to_owned();
        if host_for_remote(&cidr).is_none() {
            return Err(AppError::validation(vec![FieldError::new(
                "cidrs",
                "invalid_cidr",
                "An address is IPv4 as a.b.c.d/n, the start of its range, no wider than /8.",
            )]));
        }
        if !cidrs.contains(&cidr) {
            cidrs.push(cidr);
        }
    }
    let mut tx = state.db.begin().await.map_err(internal)?;
    store::set_remote_hosts(&mut tx, user.id, &cidrs)
        .await
        .map_err(internal)?;
    queue(
        &mut tx,
        &h,
        &Operation::SetDbRemoteHosts {
            username: h.tenant.username.clone(),
            name: user.name.clone(),
            cidrs: cidrs.clone(),
        },
        format!("dbremote:{}:{}", user.id.simple(), Uuid::now_v7().simple()),
        &who,
    )
    .await?;
    queue_remote_access(&mut tx, &h, &who).await?;
    dhpanel_audit::insert(
        &mut *tx,
        &entry("db_user.remote_hosts_set", &who, &h, "db_user", user.id)
            .metadata(serde_json::json!({ "user": user.name, "count": cidrs.len() })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(RemoteHosts { cidrs }))
}

/// OpenAPI fragment (full paths; merge into the root document).
#[derive(OpenApi)]
#[openapi(
    paths(
        list_databases, create_database, delete_database, list_users, create_user,
        change_password, delete_user, set_grant, remove_grant, get_remote, set_remote,
        imports::start_import, imports::list_imports, imports::get_import,
        imports::put_chunk, imports::complete_import, imports::cancel_import,
        exports::start_export, exports::list_exports, exports::get_export,
        exports::make_link, exports::delete_export,
        phpmyadmin::open_phpmyadmin
    ),
    components(schemas(
        GrantView, DatabaseView, DatabaseList, UserGrantView, DbUserView, DbUserList,
        DbUserCreated, AddDatabase, AddDbUser, NewPassword, SetAccess, RemoteHosts,
        imports::ImportView, imports::ImportList, imports::StartImport,
        exports::ExportView, exports::ExportList, exports::DownloadLink,
        phpmyadmin::PhpMyAdminLink
    )),
    tags((name = "databases", description = "Databases, their users and grants"))
)]
pub struct DatabasesApi;
