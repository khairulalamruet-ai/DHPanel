//! An account's own services (T10.12): its containers, listed and restarted.
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/services` | the owner |
//! | POST | `/api/v1/client/services/{name}/restart` | the owner |
//!
//! Only containers that carry the account's id are listed or restarted: the node, not this
//! module, decides that (`restart_container`), so another account's container, the node's
//! database or a name that does not exist all answer `404`.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
    routing::{get, post},
};
use dhpanel_database::{
    accounts::DbAccountKind,
    operations::{self, NewOperation, Status},
    sqlx::PgPool,
    tenants, users,
};
use dhpanel_http_kit::{AppError, ProblemDetails};
use dhpanel_node_protocol::{Operation, Output};
use dhpanel_rbac::Principal;
use dhpanel_session::SessionStore;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/services/module.toml");

/// How long a request waits for the node by default.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(20);

/// What the module's handlers share.
#[derive(Clone)]
pub struct ServicesState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// How long a request waits for the node before answering `202`.
    pub wait: Duration,
}

impl FromRef<ServicesState> for PgPool {
    fn from_ref(s: &ServicesState) -> Self {
        s.db.clone()
    }
}

impl FromRef<ServicesState> for SessionStore {
    fn from_ref(s: &ServicesState) -> Self {
        s.sessions.clone()
    }
}

/// Routes for hosting accounts.
pub fn client_router() -> Router<ServicesState> {
    Router::new()
        .route("/api/v1/client/services", get(list_services))
        .route(
            "/api/v1/client/services/{name}/restart",
            post(restart_service),
        )
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// One of the account's containers.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceView {
    /// Its name (`dh-<account>-<role>`).
    pub name: String,
    /// What it is for (`web`, `php`).
    pub role: String,
    /// The image it runs.
    pub image: String,
    /// `running`, `exited`, `restarting`, `created`, ...
    pub state: String,
    /// Docker's words ("Up 3 hours").
    pub status: String,
    /// When it was made (Unix seconds).
    pub created: i64,
}

/// The services.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceList {
    /// By name.
    pub items: Vec<ServiceView>,
}

/// A restarted service.
#[derive(Debug, Serialize, ToSchema)]
pub struct Restarted {
    /// The container.
    pub name: String,
    /// Its state now.
    pub state: String,
}

/// The node did not answer in time.
#[derive(Debug, Serialize, ToSchema)]
pub struct Pending {
    /// The operation.
    pub operation_id: Uuid,
}

async fn tenant_of(state: &ServicesState, who: &Principal) -> Result<tenants::Tenant, AppError> {
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

/// Asks the node and waits; `None` when it did not answer in time.
async fn ask(
    state: &ServicesState,
    tenant: &tenants::Tenant,
    who: &Principal,
    op: &Operation,
) -> Result<Result<Output, Uuid>, AppError> {
    let payload = serde_json::to_value(op).map_err(internal)?;
    let queued = operations::enqueue(
        &state.db,
        &NewOperation {
            node_id: tenant.node_id,
            kind: op.type_name(),
            tenant_id: Some(tenant.id),
            payload: &payload,
            idempotency_key: &format!("service:{}", Uuid::new_v4().simple()),
            created_by: Some(who.account_id()),
            ttl_secs: 120,
            max_attempts: 1,
        },
    )
    .await
    .map_err(|e| match e {
        operations::EnqueueError::UnknownNode => AppError::conflict(
            "node_unavailable",
            "The node of this account no longer exists.",
        ),
        other => internal(other),
    })?;
    let id = queued.operation().id;
    let started = std::time::Instant::now();
    loop {
        let row = operations::get(&state.db, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| AppError::not_found("operation"))?;
        match row.status {
            Status::Succeeded => {
                let output: Output = row
                    .output
                    .and_then(|o| serde_json::from_value(o).ok())
                    .ok_or_else(|| internal(std::io::Error::other("an unreadable answer")))?;
                return Ok(Ok(output));
            }
            Status::Failed => {
                let message = row
                    .error
                    .as_ref()
                    .and_then(|e| e.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                return Err(if message.contains("no such container") {
                    AppError::not_found("service")
                } else {
                    AppError::unavailable(
                        "node_unavailable",
                        "The node could not do this; try again in a moment.",
                    )
                });
            }
            Status::Pending | Status::Dispatched | Status::Retryable => {}
        }
        if started.elapsed() >= state.wait {
            return Ok(Err(id));
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
}

/// The account's containers.
#[utoipa::path(
    get,
    path = "/api/v1/client/services",
    tag = "services",
    operation_id = "clientListServices",
    security(("session" = [])),
    responses(
        (status = 200, description = "The services", body = ServiceList),
        (status = 202, description = "The node is slow; ask again", body = Pending),
        (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "`node_unavailable`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list_services(
    State(state): State<ServicesState>,
    who: Principal,
) -> Result<Response, AppError> {
    let tenant = tenant_of(&state, &who).await?;
    let op = Operation::ListContainers {
        tenant_id: tenant.id,
    };
    match ask(&state, &tenant, &who, &op).await? {
        Ok(Output::Containers { items }) => Ok(Json(ServiceList {
            items: items
                .into_iter()
                .map(|c| ServiceView {
                    name: c.name,
                    role: c.role,
                    image: c.image,
                    state: c.state,
                    status: c.status,
                    created: c.created,
                })
                .collect(),
        })
        .into_response()),
        Ok(_) => Err(internal(std::io::Error::other("an unexpected answer"))),
        Err(operation_id) => {
            Ok((StatusCode::ACCEPTED, Json(Pending { operation_id })).into_response())
        }
    }
}

/// Restart one of the account's services.
#[utoipa::path(
    post,
    path = "/api/v1/client/services/{name}/restart",
    tag = "services",
    operation_id = "clientRestartService",
    security(("session" = [])),
    params(
        ("name" = String, Path, description = "The container's name"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 200, description = "Restarted", body = Restarted),
        (status = 202, description = "The node is slow; the restart goes on", body = Pending),
        (status = 404, description = "The account has no such service", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "`node_unavailable`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn restart_service(
    State(state): State<ServicesState>,
    who: Principal,
    Path(name): Path<String>,
) -> Result<Response, AppError> {
    let tenant = tenant_of(&state, &who).await?;
    if tenant.status.as_str() != "active" {
        return Err(AppError::conflict(
            "tenant_not_running",
            "Only a running account can restart its services.",
        ));
    }
    let op = Operation::RestartContainer {
        tenant_id: tenant.id,
        name: name.clone(),
    };
    match ask(&state, &tenant, &who, &op).await? {
        Ok(Output::ContainerRestarted { name, state: s }) => {
            dhpanel_audit_entry(&state, &who, &tenant, &name).await?;
            Ok(Json(Restarted { name, state: s }).into_response())
        }
        Ok(_) => Err(internal(std::io::Error::other("an unexpected answer"))),
        Err(operation_id) => {
            Ok((StatusCode::ACCEPTED, Json(Pending { operation_id })).into_response())
        }
    }
}

async fn dhpanel_audit_entry(
    state: &ServicesState,
    who: &Principal,
    tenant: &tenants::Tenant,
    name: &str,
) -> Result<(), AppError> {
    sqlx_insert_audit(&state.db, who, tenant, name)
        .await
        .map_err(internal)
}

async fn sqlx_insert_audit(
    db: &PgPool,
    who: &Principal,
    tenant: &tenants::Tenant,
    name: &str,
) -> Result<(), dhpanel_database::sqlx::Error> {
    dhpanel_audit::insert(
        db,
        &dhpanel_audit::AuditEntry::new("service.restarted", dhpanel_audit::Outcome::Success)
            .actor(who.account_id())
            .tenant(tenant.id)
            .metadata(serde_json::json!({ "container": name })),
    )
    .await
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(list_services, restart_service),
    components(schemas(ServiceView, ServiceList, Restarted, Pending)),
    tags((name = "services", description = "An account's own services"))
)]
pub struct ServicesApi;
