//! Adding servers (nodes) to the panel.
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | POST   | `/api/v1/admin/nodes` | `admin.nodes.manage`, sudo mode |
//! | GET    | `/api/v1/admin/nodes/enrollments` | `admin.nodes.read` |
//! | DELETE | `/api/v1/admin/nodes/enrollments/{id}` | `admin.nodes.manage`, sudo mode |
//! | POST   | `/api/v1/public/nodes/enroll` | the enrollment token itself (see [`enroll`]) |
//!
//! Adding a node starts with an **enrollment token**: `POST /api/v1/admin/nodes`
//! with the planned node name returns the token **once**, together with the
//! command to run on the new server. The agent presents the token with its
//! certificate request (T02.06), which uses it up.
//!
//! * A token is 256 random bits; only its SHA-256 is stored.
//! * It is valid for `agent.enrollment_token_minutes` (default 15), usable
//!   once, and can be cancelled while pending.
//! * Creating and cancelling are audited in the same transaction as the change.
//!   The token never appears in the audit log or in any log line.

pub mod enroll;
pub mod token;

use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use dhpanel_audit::{AuditEntry, AuditWriter, Outcome};
use dhpanel_cache::{
    RedisHandle,
    lockout::{FailureLimiter, LockoutPolicy},
};
use dhpanel_database::{
    node_enrollments::{self, EnrollmentToken, RevokeOutcome},
    sqlx::PgPool,
};
use dhpanel_http_kit::{AppError, FieldError, ProblemDetails};
use dhpanel_pki::CertificateAuthority;
use dhpanel_rbac::{
    Require, RequireRecent,
    permissions::{NodesManage, NodesRead},
};
use dhpanel_session::{SessionStore, rfc3339};
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// Most tokens listed.
pub const LIST_LIMIT: i64 = 100;

/// Dependencies of the nodes handlers.
#[derive(Clone)]
pub struct NodesState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// How agents reach the control plane (`agent.public_url`); used for the
    /// enrol command. Validated at startup to be safe in a shell command.
    pub public_url: Option<Arc<str>>,
    /// How long a new token stays valid.
    pub token_ttl: Duration,
    /// The node CA. Without it enrollment answers 503 `enrollment_unavailable`.
    pub ca: Option<Arc<CertificateAuthority>>,
    /// Counts failed enrollment attempts per client address.
    pub limiter: FailureLimiter,
    /// Audit log for outcomes that have no transaction of their own.
    pub audit: AuditWriter,
}

/// Failed enrollment attempts from one address (an IPv6 /64) allowed inside
/// [`ENROLL_WINDOW`] before it is locked out.
pub const ENROLL_MAX_FAILURES: u32 = 10;
/// Window in which failures are counted.
pub const ENROLL_WINDOW: Duration = Duration::from_secs(10 * 60);
/// How long a locked-out address must wait.
pub const ENROLL_LOCKOUT: Duration = Duration::from_secs(15 * 60);

/// The limiter used for enrollment attempts.
#[must_use]
pub fn enrollment_limiter(redis: RedisHandle) -> FailureLimiter {
    FailureLimiter::new(
        redis,
        "node-enroll:ip",
        LockoutPolicy {
            threshold: ENROLL_MAX_FAILURES,
            window: ENROLL_WINDOW,
            lockout: ENROLL_LOCKOUT,
        },
    )
}

impl FromRef<NodesState> for PgPool {
    fn from_ref(s: &NodesState) -> Self {
        s.db.clone()
    }
}

impl FromRef<NodesState> for SessionStore {
    fn from_ref(s: &NodesState) -> Self {
        s.sessions.clone()
    }
}

/// Routes with full paths; merge into the application router.
pub fn router() -> Router<NodesState> {
    Router::new()
        .route(
            "/api/v1/admin/nodes",
            axum::routing::post(create_enrollment),
        )
        .route("/api/v1/admin/nodes/enrollments", get(list_enrollments))
        .route(
            "/api/v1/admin/nodes/enrollments/{id}",
            delete(revoke_enrollment),
        )
        .route(
            "/api/v1/public/nodes/enroll",
            axum::routing::post(enroll::enroll_node),
        )
}

/// Request to add a node.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateEnrollment {
    /// Name for the new node: 2-63 characters, a-z, 0-9 and `-`, starting with a letter.
    #[schema(example = "eu-west-1")]
    pub name: String,
}

/// A newly created enrollment token. **The token is shown only in this response.**
#[derive(Serialize, ToSchema)]
pub struct CreatedEnrollment {
    /// Token id (use it to cancel the token).
    pub id: Uuid,
    /// The node name this token is for.
    pub name: String,
    /// The one-time token. It cannot be shown again.
    #[schema(example = "dhn_…")]
    pub token: String,
    /// When the token stops working (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub expires_at: String,
    /// Seconds until then.
    pub expires_in: u64,
    /// Command to run on the new server. Absent when `agent.public_url` is not configured.
    pub enroll_command: Option<String>,
}

impl std::fmt::Debug for CreatedEnrollment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedEnrollment")
            .field("id", &self.id)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// An enrollment token as listed (without the token itself).
#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollmentView {
    /// Token id.
    pub id: Uuid,
    /// The node name it is for.
    pub name: String,
    /// `pending`, `expired`, `consumed` or `revoked`.
    #[schema(example = "pending")]
    pub status: String,
    /// Account that created it, if it still exists.
    pub created_by: Option<Uuid>,
    /// Creation time (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub created_at: String,
    /// Expiry (RFC 3339, UTC).
    #[schema(format = DateTime)]
    pub expires_at: String,
}

impl From<EnrollmentToken> for EnrollmentView {
    fn from(t: EnrollmentToken) -> Self {
        Self {
            id: t.id,
            name: t.name,
            status: t.status.as_str().to_owned(),
            created_by: t.created_by,
            created_at: rfc3339(t.created_at),
            expires_at: rfc3339(t.expires_at),
        }
    }
}

/// The most recent enrollment tokens.
#[derive(Debug, Serialize, ToSchema)]
pub struct EnrollmentList {
    /// Newest first (at most 100).
    pub items: Vec<EnrollmentView>,
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// Node names follow the database rule: 2-63 characters of `a-z`, `0-9` and
/// `-`, starting with a letter.
fn valid_node_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && (2..=63).contains(&name.len())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Add a node: creates a one-time enrollment token.
#[utoipa::path(
    post,
    path = "/api/v1/admin/nodes",
    tag = "nodes",
    operation_id = "createNodeEnrollment",
    security(("session" = [])),
    params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = CreateEnrollment,
    responses(
        (status = 201, description = "Token created; shown only now", body = CreatedEnrollment),
        (status = 403, description = "`permission_denied` (needs admin.nodes.manage), `reauth_required`, `mfa_setup_required` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`node_name_taken`: a node or a valid token already uses the name", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid name", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn create_enrollment(
    State(state): State<NodesState>,
    RequireRecent(who, _): RequireRecent<NodesManage>,
    Json(req): Json<CreateEnrollment>,
) -> Result<Response, AppError> {
    let name = req.name.trim().to_owned();
    if !valid_node_name(&name) {
        return Err(AppError::validation(vec![FieldError::new(
            "name",
            "invalid_format",
            "Use 2-63 characters: a-z, 0-9 and '-', starting with a letter.",
        )]));
    }
    let (secret, hash) = token::generate().map_err(internal)?;
    let ttl_secs = i64::try_from(state.token_ttl.as_secs()).map_err(internal)?;

    let mut tx = state.db.begin().await.map_err(internal)?;
    if node_enrollments::name_in_use(&mut tx, &name)
        .await
        .map_err(internal)?
    {
        return Err(AppError::conflict(
            "node_name_taken",
            "A node or an unused enrollment token already has this name.",
        ));
    }
    let created = node_enrollments::insert_token(&mut tx, &name, &hash, who.account_id(), ttl_secs)
        .await
        .map_err(internal)?;
    dhpanel_audit::insert(
        &mut *tx,
        &AuditEntry::new("admin.node_enrollment_created", Outcome::Success)
            .actor(who.account_id())
            .target("node_enrollment", created.id)
            .metadata(serde_json::json!({ "name": name, "expires_in": ttl_secs })),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    let token_text = secret.expose_secret().to_owned();
    // The CA fingerprint lets the agent pin the control plane's CA instead of
    // trusting whoever answers at that address.
    let pin = state
        .ca
        .as_deref()
        .map(|ca| format!(" --ca-fingerprint sha256:{}", ca.fingerprint_sha256()))
        .unwrap_or_default();
    let enroll_command = state
        .public_url
        .as_deref()
        .map(|url| format!("dhpanel-agent enroll --server {url}{pin} --token {token_text}"));
    let body = CreatedEnrollment {
        id: created.id,
        name: created.name.clone(),
        token: token_text,
        expires_at: rfc3339(created.expires_at),
        expires_in: state.token_ttl.as_secs(),
        enroll_command,
    };
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// List recent enrollment tokens (never the tokens themselves).
#[utoipa::path(
    get,
    path = "/api/v1/admin/nodes/enrollments",
    tag = "nodes",
    operation_id = "listNodeEnrollments",
    security(("session" = [])),
    responses(
        (status = 200, description = "Recent tokens, newest first", body = EnrollmentList),
        (status = 403, description = "`permission_denied` (needs admin.nodes.read) or `mfa_setup_required`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list_enrollments(
    State(state): State<NodesState>,
    _who: Require<NodesRead>,
) -> Result<Json<EnrollmentList>, AppError> {
    let items = node_enrollments::list_tokens(&state.db, LIST_LIMIT)
        .await
        .map_err(internal)?
        .into_iter()
        .map(EnrollmentView::from)
        .collect();
    Ok(Json(EnrollmentList { items }))
}

/// Cancel a token that has not been used yet.
#[utoipa::path(
    delete,
    path = "/api/v1/admin/nodes/enrollments/{id}",
    tag = "nodes",
    operation_id = "revokeNodeEnrollment",
    security(("session" = [])),
    params(
        ("id" = Uuid, Path, description = "Token id"),
        ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")
    ),
    responses(
        (status = 204, description = "Cancelled"),
        (status = 403, description = "`permission_denied`, `reauth_required` or `csrf_failed`", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "No such token", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`enrollment_not_pending`: already used, cancelled or expired", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn revoke_enrollment(
    State(state): State<NodesState>,
    RequireRecent(who, _): RequireRecent<NodesManage>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id: Uuid = id
        .parse()
        .map_err(|_| AppError::not_found("node_enrollment"))?;
    let mut tx = state.db.begin().await.map_err(internal)?;
    match node_enrollments::revoke_token(&mut tx, id)
        .await
        .map_err(internal)?
    {
        RevokeOutcome::Unknown => return Err(AppError::not_found("node_enrollment")),
        RevokeOutcome::NotPending => {
            return Err(AppError::conflict(
                "enrollment_not_pending",
                "This token was already used, cancelled or has expired.",
            ));
        }
        RevokeOutcome::Revoked => {}
    }
    dhpanel_audit::insert(
        &mut *tx,
        &AuditEntry::new("admin.node_enrollment_revoked", Outcome::Success)
            .actor(who.account_id())
            .target("node_enrollment", id),
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment (full paths; merge into the root document).
#[derive(OpenApi)]
#[openapi(
    paths(create_enrollment, list_enrollments, revoke_enrollment, enroll::enroll_node),
    components(schemas(
        CreateEnrollment, CreatedEnrollment, EnrollmentView, EnrollmentList,
        enroll::EnrollRequest, enroll::EnrollResponse
    )),
    tags((name = "nodes", description = "Add and manage servers (nodes)"))
)]
pub struct NodesApi;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn node_names_follow_the_database_rule() {
        for ok in ["ab", "eu-west-1", "node1", &format!("a{}", "b".repeat(62))] {
            assert!(valid_node_name(ok), "{ok}");
        }
        for bad in [
            "",
            "a",
            "Node",
            "1node",
            "-node",
            "no_de",
            "no de",
            "node.1",
            "nöde",
            &format!("a{}", "b".repeat(63)),
        ] {
            assert!(!valid_node_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_created_token_never_shows_in_debug_output() {
        let created = CreatedEnrollment {
            id: Uuid::now_v7(),
            name: "n1".into(),
            token: "dhn_secret-token-value".into(),
            expires_at: String::new(),
            expires_in: 900,
            enroll_command: Some("dhpanel-agent enroll --token dhn_secret-token-value".into()),
        };
        let dbg = format!("{created:?}");
        assert!(
            dbg.contains("n1") && !dbg.contains("secret-token-value"),
            "{dbg}"
        );
    }

    #[test]
    fn openapi_fragment_lists_the_paths() {
        let json = NodesApi::openapi().to_json().expect("json");
        for p in [
            "/api/v1/admin/nodes",
            "/api/v1/admin/nodes/enrollments",
            "/api/v1/admin/nodes/enrollments/{id}",
        ] {
            assert!(json.contains(p), "{p}");
        }
    }
}
