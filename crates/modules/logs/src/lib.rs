//! An account's logs, for its owner (T10.04).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET | `/api/v1/client/logs` | the owner; the `logs` feature |
//!
//! The three logs the node can read (`web_access`, `web_error`, `php_error`: what the
//! account's containers write, kept by Docker), the newest lines, from a time on, filtered by
//! a text. The request waits a few seconds for the node; a live view asks again with the time
//! of the newest line it has (`since_ms` is exclusive of that line).

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::{IntoResponse as _, Response},
    routing::get,
};
use builtpanel_database::{
    accounts::DbAccountKind,
    operations::{self, NewOperation, Status},
    plans,
    sqlx::PgPool,
    tenants,
};
use builtpanel_http_kit::{AppError, FieldError, ProblemDetails};
use builtpanel_node_protocol::{LogLine, LogSource, MAX_LOG_LINES, Operation, Output};
use builtpanel_rbac::Principal;
use builtpanel_session::SessionStore;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/logs/module.toml");

/// What the module's handlers share.
#[derive(Clone)]
pub struct LogsState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// How long a request waits for the node before answering `202`.
    pub wait: Duration,
}

impl FromRef<LogsState> for PgPool {
    fn from_ref(s: &LogsState) -> Self {
        s.db.clone()
    }
}

impl FromRef<LogsState> for SessionStore {
    fn from_ref(s: &LogsState) -> Self {
        s.sessions.clone()
    }
}

/// How long a request waits for the node by default.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(8);

/// Routes for hosting accounts.
pub fn client_router() -> Router<LogsState> {
    Router::new().route("/api/v1/client/logs", get(read_logs))
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// Which log, from when, what to look for.
#[derive(Debug, Deserialize, IntoParams)]
pub struct LogQuery {
    /// `web_access`, `web_error` or `php_error`.
    pub source: String,
    /// Only lines after this time (Unix milliseconds). Default: any.
    #[serde(default)]
    pub since_ms: i64,
    /// How many of the newest lines (1-2000). Default 200.
    pub lines: Option<u32>,
    /// Only lines that contain this text (case-insensitive, up to 200 characters).
    pub search: Option<String>,
}

/// The lines.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogView {
    /// Oldest first.
    pub lines: Vec<LogLineView>,
    /// Older lines were left out.
    pub truncated: bool,
}

/// One line.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogLineView {
    /// When it was written (Unix milliseconds).
    pub at_ms: i64,
    /// The line.
    pub text: String,
}

/// The node did not answer in time.
#[derive(Debug, Serialize, ToSchema)]
pub struct Pending {
    /// The operation; asking again gives a fresh answer.
    pub operation_id: Uuid,
}

fn bad(name: &'static str, code: &'static str, message: &str) -> AppError {
    AppError::validation(vec![FieldError::new(name, code, message)])
}

/// Read the end of one of the account's logs.
#[utoipa::path(
    get,
    path = "/api/v1/client/logs",
    tag = "logs",
    operation_id = "clientReadLog",
    security(("session" = [])),
    params(LogQuery),
    responses(
        (status = 200, description = "The lines", body = LogView),
        (status = 202, description = "The node is slow; ask again", body = Pending),
        (status = 403, description = "`client_only` or `feature_disabled`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`tenant_not_running`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "A source, count or search that is not usable", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "`node_unavailable`, or the account has no such log yet", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn read_logs(
    State(state): State<LogsState>,
    who: Principal,
    Query(q): Query<LogQuery>,
) -> Result<Response, AppError> {
    if who.kind != DbAccountKind::Client {
        return Err(AppError::forbidden(
            "client_only",
            "This is for hosting accounts.",
        ));
    }
    let user = builtpanel_database::users::get(&state.db, who.account_id())
        .await
        .map_err(internal)?;
    let owned = user.and_then(|u| u.tenant).ok_or_else(|| {
        AppError::conflict("no_hosting", "This account has no hosting account yet.")
    })?;
    let tenant = tenants::get(&state.db, owned.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))?;
    if !plans::includes_feature(&state.db, tenant.plan_id, "logs")
        .await
        .map_err(internal)?
    {
        return Err(AppError::forbidden(
            "feature_disabled",
            "Logs are not included in this plan.",
        ));
    }
    if tenant.status.as_str() != "active" {
        return Err(AppError::conflict(
            "tenant_not_running",
            "Only a running account has logs to read.",
        ));
    }
    let source = match q.source.as_str() {
        "web_access" => LogSource::WebAccess,
        "web_error" => LogSource::WebError,
        "php_error" => LogSource::PhpError,
        _ => {
            return Err(bad(
                "source",
                "invalid",
                "web_access, web_error or php_error.",
            ));
        }
    };
    let lines = q.lines.unwrap_or(200);
    if !(1..=MAX_LOG_LINES).contains(&lines) || q.since_ms < 0 {
        return Err(bad("lines", "out_of_range", "Between 1 and 2000 lines."));
    }
    let search = q.search.filter(|s| !s.is_empty());
    if search
        .as_ref()
        .is_some_and(|s| s.chars().count() > 200 || s.contains('\0'))
    {
        return Err(bad("search", "invalid", "At most 200 characters."));
    }
    let op = Operation::ReadLog {
        tenant_id: tenant.id,
        username: tenant.username.clone(),
        source,
        // Lines at the cursor's own time are the ones the viewer has.
        since_ms: q.since_ms.saturating_add(i64::from(q.since_ms > 0)),
        tail: lines,
        contains: search,
    };
    let payload = serde_json::to_value(&op).map_err(internal)?;
    let queued = operations::enqueue(
        &state.db,
        &NewOperation {
            node_id: tenant.node_id,
            kind: op.type_name(),
            tenant_id: Some(tenant.id),
            payload: &payload,
            idempotency_key: &format!("log:{}", Uuid::new_v4().simple()),
            created_by: Some(who.account_id()),
            ttl_secs: 60,
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
                let Output::LogLines { lines, truncated } = output else {
                    return Err(internal(std::io::Error::other("an unexpected answer")));
                };
                return Ok(Json(LogView {
                    lines: lines
                        .into_iter()
                        .map(|LogLine { at_ms, text }| LogLineView { at_ms, text })
                        .collect(),
                    truncated,
                })
                .into_response());
            }
            Status::Failed => {
                return Err(AppError::unavailable(
                    "node_unavailable",
                    "The node could not read this log (the account may have no such log yet).",
                ));
            }
            Status::Pending | Status::Dispatched | Status::Retryable => {}
        }
        if started.elapsed() >= state.wait {
            return Ok((StatusCode::ACCEPTED, Json(Pending { operation_id: id })).into_response());
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(read_logs),
    components(schemas(LogView, LogLineView, Pending)),
    tags((name = "logs", description = "Logs"))
)]
pub struct LogsApi;
