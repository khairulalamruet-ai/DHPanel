//! An account's own services (T10.12): its containers, listed and restarted.
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/services` | the owner |
//! | POST | `/api/v1/client/services/{name}/restart` | the owner |
//! | GET | `/api/v1/admin/nodes/{id}/services` | `admin.nodes.read` |
//! | POST | `/api/v1/admin/nodes/{id}/services/{name}/restart` | `admin.nodes.manage` |
//! | GET | `/api/v1/admin/nodes/{id}/services/{name}/log` | `admin.nodes.read` |
//!
//! The node's core services (`database`, `phpmyadmin`, `sftp`, `caddy`, `agent`) are
//! watched every half minute; one that stops is announced once as `node_service_down`
//! (and `node_service_recovered`), which the notification module (phase 18) turns into mail.
//! Only the first three can be restarted or have their log read; the web server and the agent
//! are host services (a restart of those needs the privileged helper: not part of this step).
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
use dhpanel_rbac::{
    Principal, Require,
    permissions::{NodesManage, NodesRead},
};
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

/// Routes for administrators.
pub fn admin_router() -> Router<ServicesState> {
    Router::new()
        .route("/api/v1/admin/nodes/{id}/services", get(node_services))
        .route(
            "/api/v1/admin/nodes/{id}/services/{name}/restart",
            post(restart_node_service),
        )
        .route(
            "/api/v1/admin/nodes/{id}/services/{name}/log",
            get(node_service_log),
        )
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

/// A core service of a node.
#[derive(Debug, Serialize, ToSchema)]
pub struct NodeServiceView {
    /// `database`, `phpmyadmin`, `sftp`, `caddy` or `agent`.
    pub name: String,
    /// `running`, `stopped`, `unreachable` or `not_installed`.
    pub state: String,
    /// Docker's words, or what went wrong.
    pub detail: String,
    /// Since when it is in this state (Unix seconds).
    pub since: i64,
    /// When it was last looked at (Unix seconds). A node that is off shows an old time.
    pub checked_at: i64,
}

/// The services of a node.
#[derive(Debug, Serialize, ToSchema)]
pub struct NodeServiceList {
    /// By name.
    pub items: Vec<NodeServiceView>,
}

/// The core services of a node, as it last reported them.
#[utoipa::path(
    get,
    path = "/api/v1/admin/nodes/{id}/services",
    tag = "services",
    operation_id = "adminNodeServices",
    security(("session" = [])),
    params(("id" = Uuid, Path, description = "Node id")),
    responses(
        (status = 200, description = "The services", body = NodeServiceList),
        (status = 403, description = "`permission_denied` (needs admin.nodes.read)", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn node_services(
    State(state): State<ServicesState>,
    _who: Require<NodesRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<NodeServiceList>, AppError> {
    let items = dhpanel_database::node_services::list(&state.db, id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|s| NodeServiceView {
            name: s.name,
            state: s.state,
            detail: s.detail,
            since: s.since,
            checked_at: s.checked_at,
        })
        .collect();
    Ok(Json(NodeServiceList { items }))
}

/// Asks a node (no account) and waits.
async fn ask_node(
    state: &ServicesState,
    node_id: Uuid,
    who: &Principal,
    op: &Operation,
) -> Result<Result<Output, Uuid>, AppError> {
    let payload = serde_json::to_value(op).map_err(internal)?;
    let queued = operations::enqueue(
        &state.db,
        &NewOperation {
            node_id,
            kind: op.type_name(),
            tenant_id: None,
            payload: &payload,
            idempotency_key: &format!("nodeservice:{}", Uuid::new_v4().simple()),
            created_by: Some(who.account_id()),
            ttl_secs: 120,
            max_attempts: 1,
        },
    )
    .await
    .map_err(|e| match e {
        operations::EnqueueError::UnknownNode => AppError::not_found("node"),
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
                    .unwrap_or_default()
                    .to_owned();
                return Err(
                    if message.contains("not a core service") || message.contains("not installed") {
                        AppError::not_found("service")
                    } else {
                        AppError::unavailable(
                            "node_unavailable",
                            "The node could not do this; try again in a moment.",
                        )
                    },
                );
            }
            Status::Pending | Status::Dispatched | Status::Retryable => {}
        }
        if started.elapsed() >= state.wait {
            return Ok(Err(id));
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
}

/// Restart a core service of a node (`database`, `phpmyadmin` or `sftp`).
#[utoipa::path(
    post,
    path = "/api/v1/admin/nodes/{id}/services/{name}/restart",
    tag = "services",
    operation_id = "adminRestartNodeService",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Node id"),
        ("name" = String, Path, description = "`database`, `phpmyadmin` or `sftp`"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 200, description = "Restarted", body = Restarted),
        (status = 202, description = "The node is slow; the restart goes on", body = Pending),
        (status = 404, description = "No such node, service, or it is not installed there", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "`node_unavailable`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn restart_node_service(
    State(state): State<ServicesState>,
    who: Require<NodesManage>,
    Path((id, name)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let op = Operation::RestartNodeService { name: name.clone() };
    match ask_node(&state, id, &who.0, &op).await? {
        Ok(Output::ContainerRestarted { name, state: s }) => {
            dhpanel_audit::insert(
                &state.db,
                &dhpanel_audit::AuditEntry::new(
                    "node_service.restarted",
                    dhpanel_audit::Outcome::Success,
                )
                .actor(who.0.account_id())
                .target("node", id)
                .metadata(serde_json::json!({ "service": name })),
            )
            .await
            .map_err(internal)?;
            Ok(Json(Restarted { name, state: s }).into_response())
        }
        Ok(_) => Err(internal(std::io::Error::other("an unexpected answer"))),
        Err(operation_id) => {
            Ok((StatusCode::ACCEPTED, Json(Pending { operation_id })).into_response())
        }
    }
}

/// Which log.
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct ServiceLogQuery {
    /// How many of the newest lines (1-2000, default 200).
    pub lines: Option<u32>,
}

/// One line of a service's log.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceLogLine {
    /// When (Unix milliseconds).
    pub at_ms: i64,
    /// The line.
    pub text: String,
}

/// The end of a service's log.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceLog {
    /// Oldest first.
    pub lines: Vec<ServiceLogLine>,
    /// Older lines were left out.
    pub truncated: bool,
}

/// The end of the log of a core service of a node (`database`, `phpmyadmin` or `sftp`).
#[utoipa::path(
    get,
    path = "/api/v1/admin/nodes/{id}/services/{name}/log",
    tag = "services",
    operation_id = "adminNodeServiceLog",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Node id"),
        ("name" = String, Path, description = "`database`, `phpmyadmin` or `sftp`"),
        ServiceLogQuery
    ),
    responses(
        (status = 200, description = "The lines", body = ServiceLog),
        (status = 202, description = "The node is slow; ask again", body = Pending),
        (status = 404, description = "No such node or service", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "A count out of range", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn node_service_log(
    State(state): State<ServicesState>,
    who: Require<NodesRead>,
    Path((id, name)): Path<(Uuid, String)>,
    axum::extract::Query(q): axum::extract::Query<ServiceLogQuery>,
) -> Result<Response, AppError> {
    let lines = q.lines.unwrap_or(200);
    if !(1..=dhpanel_node_protocol::MAX_LOG_LINES).contains(&lines) {
        return Err(AppError::validation(vec![
            dhpanel_http_kit::FieldError::new("lines", "out_of_range", "Between 1 and 2000 lines."),
        ]));
    }
    let op = Operation::ReadServiceLog { name, tail: lines };
    match ask_node(&state, id, &who.0, &op).await? {
        Ok(Output::LogLines { lines, truncated }) => Ok(Json(ServiceLog {
            lines: lines
                .into_iter()
                .map(|l| ServiceLogLine {
                    at_ms: l.at_ms,
                    text: l.text,
                })
                .collect(),
            truncated,
        })
        .into_response()),
        Ok(_) => Err(internal(std::io::Error::other("an unexpected answer"))),
        Err(operation_id) => {
            Ok((StatusCode::ACCEPTED, Json(Pending { operation_id })).into_response())
        }
    }
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(
        list_services, restart_service, node_services, restart_node_service, node_service_log
    ),
    components(schemas(
        ServiceView, ServiceList, Restarted, Pending, NodeServiceView, NodeServiceList,
        ServiceLogLine, ServiceLog
    )),
    tags((name = "services", description = "An account's own services"))
)]
pub struct ServicesApi;
