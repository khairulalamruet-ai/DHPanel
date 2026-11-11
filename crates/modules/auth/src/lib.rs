//! Auth module: login, logout and the current account.
//!
//! | Method | Path | Auth |
//! |---|---|---|
//! | POST | `/api/v1/public/auth/login` | none |
//! | POST | `/api/v1/auth/logout` | session |
//! | GET  | `/api/v1/auth/me` | session |
//!
//! Security properties:
//! * Unknown accounts and wrong passwords give the same 401 `invalid_credentials`
//!   and take the same time (dummy argon2 verification).
//! * Account status (suspended/disabled) is only revealed after a correct password.
//! * Any session presented at login is revoked first (session fixation).
//! * Every attempt is recorded in `login_events`.
//! * Oversized input is rejected before hashing (bounded CPU per request).
//! * Lockout (see [`LoginLimits`]): failed logins are counted per account
//!   (per hashed login name when no account matches, so unknown and existing
//!   names behave the same) and per client address. Reaching a threshold
//!   answers 429 `too_many_attempts` with `Retry-After` until the lock expires,
//!   even for the correct password. A successful login clears the account count.
//!
//! The server must be run with `into_make_service_with_connect_info::<SocketAddr>()`
//! so the client IP is available. CSRF (T01.07) is layered on separately.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{ConnectInfo, FromRef, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dhpanel_auth_core::password::PasswordHasher;
use dhpanel_cache::{
    RedisHandle,
    lockout::{FailureLimiter, LockoutPolicy},
};
use dhpanel_database::{
    accounts::{self, AccountProfile, DbAccountKind, DbAccountStatus, LoginEvent, LoginOutcome},
    sqlx::PgPool,
};
use dhpanel_domain::{Email, Username};
use dhpanel_http_kit::{
    AppError, ProblemDetails,
    client_ip::{client_ip, rate_limit_key},
};
use dhpanel_session::{
    Authenticated, NewSession, RevokeReason, SessionStore, clear_cookie, cookie_value, set_cookie,
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use utoipa::{
    Modify, OpenApi, ToSchema,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
};
use uuid::Uuid;

/// Longest accepted login identifier (characters).
pub const MAX_LOGIN_CHARS: usize = 254;
/// Longest accepted password at login (bytes); longer input is rejected
/// without hashing so a request cannot buy unbounded CPU.
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// Dependencies of the auth handlers.
#[derive(Clone)]
pub struct AuthState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// Password hasher with the configured cost.
    pub hasher: PasswordHasher,
    /// Failed-login lockout.
    pub limits: LoginLimits,
    /// Reverse proxies whose `X-Forwarded-For` is trusted.
    pub trusted_proxies: Arc<[IpAddr]>,
}

/// Failed-login counters: one per account (or login name), one per client
/// address (IPv6 per /64).
#[derive(Debug, Clone)]
pub struct LoginLimits {
    /// Per account / login name.
    pub account: FailureLimiter,
    /// Per client address.
    pub ip: FailureLimiter,
}

impl LoginLimits {
    /// Builds both limiters on one Redis handle.
    #[must_use]
    pub fn new(redis: RedisHandle, account: LockoutPolicy, ip: LockoutPolicy) -> Self {
        Self {
            account: FailureLimiter::new(redis.clone(), "login:account", account),
            ip: FailureLimiter::new(redis, "login:ip", ip),
        }
    }
}

/// Lockout subject for a login attempt: the account when one matched,
/// otherwise a hash of the normalised login name (never raw input in keys).
fn lockout_subject(account_id: Option<Uuid>, normalised_login: &str) -> String {
    match account_id {
        Some(id) => format!("a:{}", id.simple()),
        None => format!(
            "n:{}",
            URL_SAFE_NO_PAD.encode(Sha256::digest(normalised_login.as_bytes()))
        ),
    }
}

fn too_many_attempts(retry_after: Duration) -> AppError {
    AppError::too_many_requests(
        "too_many_attempts",
        "Too many failed login attempts. Try again later.",
        retry_after,
    )
}

impl FromRef<AuthState> for SessionStore {
    fn from_ref(s: &AuthState) -> Self {
        s.sessions.clone()
    }
}

/// Routes with full paths; merge into the application router.
pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/api/v1/public/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
}

/// Login request body.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    /// Username or email address.
    #[schema(example = "khairul")]
    pub login: String,
    /// Password.
    #[schema(format = Password)]
    pub password: String,
}

impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("login", &self.login)
            .finish_non_exhaustive()
    }
}

/// The signed-in account.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    /// Account id.
    pub id: Uuid,
    /// `admin`, `reseller` or `client`.
    #[schema(value_type = String, example = "admin")]
    pub kind: DbAccountKind,
    /// Username.
    pub username: String,
    /// Email.
    pub email: String,
    /// UI locale (`en` or `bn`).
    pub locale: String,
}

impl From<AccountProfile> for Me {
    fn from(p: AccountProfile) -> Self {
        Self {
            id: p.id,
            kind: p.kind,
            username: p.username,
            email: p.email,
            locale: p.locale,
        }
    }
}

fn invalid_credentials() -> AppError {
    AppError::authentication_failed("invalid_credentials", "Invalid username or password.")
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}

async fn count_failure(limiter: &FailureLimiter, subject: &str) -> Result<(), AppError> {
    let recorded = limiter
        .record_failure(subject)
        .await
        .map_err(AppError::internal)?;
    if recorded.locked {
        let policy = limiter.policy();
        tracing::warn!(
            subject,
            failures = recorded.failures,
            lockout_secs = policy.lockout.as_secs(),
            "login lockout started"
        );
    }
    Ok(())
}

async fn record(db: &PgPool, ev: LoginEvent<'_>) -> Result<(), AppError> {
    accounts::insert_login_event(db, &ev)
        .await
        .map_err(AppError::internal)
}

/// Log in with username or email and password.
#[utoipa::path(
    post,
    path = "/api/v1/public/auth/login",
    tag = "auth",
    operation_id = "login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in; session cookie set", body = Me,
         headers(("set-cookie" = String, description = "__Host-dh_session=…; Path=/; Secure; HttpOnly; SameSite=Strict"))),
        (status = 401, description = "Invalid username or password", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Correct password but account suspended or disabled", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Malformed body", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many failed attempts for this account or address", body = ProblemDetails, content_type = "application/problem+json",
         headers(("retry-after" = u64, description = "Seconds until the lock expires")))
    )
)]
pub async fn login(
    State(state): State<AuthState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let ip = client_ip(peer.ip(), &headers, &state.trusted_proxies);
    let ip_subject = rate_limit_key(ip);
    let ua = user_agent(&headers);
    let attempted = req.login.trim();
    let password = SecretString::from(req.password);

    let event = |account_id: Option<Uuid>, outcome| LoginEvent {
        account_id,
        attempted,
        ip,
        user_agent: ua,
        outcome,
    };

    if let Some(left) = state
        .limits
        .ip
        .locked_for(&ip_subject)
        .await
        .map_err(AppError::internal)?
    {
        record(&state.db, event(None, LoginOutcome::RateLimited)).await?;
        return Err(too_many_attempts(left));
    }

    if attempted.chars().count() > MAX_LOGIN_CHARS
        || secrecy::ExposeSecret::expose_secret(&password).len() > MAX_PASSWORD_BYTES
    {
        count_failure(&state.limits.ip, &ip_subject).await?;
        record(&state.db, event(None, LoginOutcome::BadCredentials)).await?;
        return Err(invalid_credentials());
    }

    // Identifiers that cannot exist (bad format, reserved) simply match nothing.
    let (username, email) = if attempted.contains('@') {
        (None, Email::parse(attempted).ok())
    } else {
        (Username::parse(attempted).ok(), None)
    };
    let account = if username.is_some() || email.is_some() {
        accounts::find_for_login(&state.db, username.as_ref(), email.as_ref())
            .await
            .map_err(AppError::internal)?
    } else {
        None
    };

    let normalised = match (&username, &email) {
        (Some(u), _) => u.as_str().to_owned(),
        (_, Some(e)) => e.as_str().to_owned(),
        _ => attempted.to_lowercase(),
    };
    let subject = lockout_subject(account.as_ref().map(|a| a.id), &normalised);
    let account_id = account.as_ref().map(|a| a.id);
    if let Some(left) = state
        .limits
        .account
        .locked_for(&subject)
        .await
        .map_err(AppError::internal)?
    {
        record(&state.db, event(account_id, LoginOutcome::LockedOut)).await?;
        return Err(too_many_attempts(left));
    }

    let Some(account) = account else {
        let _ = state.hasher.verify_dummy_async(password).await;
        count_failure(&state.limits.account, &subject).await?;
        count_failure(&state.limits.ip, &ip_subject).await?;
        record(&state.db, event(None, LoginOutcome::BadCredentials)).await?;
        return Err(invalid_credentials());
    };

    let verification = state
        .hasher
        .verify_async(password.clone(), account.password_hash.clone())
        .await
        .map_err(AppError::internal)?;
    if !verification.matches {
        count_failure(&state.limits.account, &subject).await?;
        count_failure(&state.limits.ip, &ip_subject).await?;
        record(
            &state.db,
            event(Some(account.id), LoginOutcome::BadCredentials),
        )
        .await?;
        return Err(invalid_credentials());
    }

    match account.status {
        DbAccountStatus::Active => {}
        DbAccountStatus::Suspended => {
            record(
                &state.db,
                event(Some(account.id), LoginOutcome::AccountSuspended),
            )
            .await?;
            return Err(AppError::forbidden(
                "account_suspended",
                "This account is suspended.",
            ));
        }
        DbAccountStatus::Disabled => {
            record(
                &state.db,
                event(Some(account.id), LoginOutcome::AccountDisabled),
            )
            .await?;
            return Err(AppError::forbidden(
                "account_disabled",
                "This account is disabled.",
            ));
        }
    }

    if let Err(err) = state.limits.account.reset(&subject).await {
        tracing::warn!(error = %err, "failed-login counter not reset");
    }

    if verification.needs_rehash {
        match state.hasher.hash_async(password).await {
            Ok(hash) => {
                if let Err(err) = accounts::update_password_hash(&state.db, account.id, &hash).await
                {
                    tracing::warn!(error = %err, "password rehash not stored");
                }
            }
            Err(err) => tracing::warn!(error = %err, "password rehash failed"),
        }
    }

    // Session fixation: never keep a session that existed before authentication.
    if let Some(previous) = cookie_value(&headers)
        && let Err(err) = state.sessions.revoke(previous, RevokeReason::Logout).await
    {
        tracing::warn!(error = %err, "previous session not revoked at login");
    }

    let (token, _session) = state
        .sessions
        .create(NewSession {
            account_id: account.id,
            ip: Some(ip),
            user_agent: ua.map(str::to_owned),
        })
        .await
        .map_err(AppError::internal)?;
    record(&state.db, event(Some(account.id), LoginOutcome::Ok)).await?;

    let profile = accounts::find_profile(&state.db, account.id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| {
            AppError::internal(std::io::Error::other("account vanished during login"))
        })?;

    let cookie = HeaderValue::from_str(&set_cookie(&token)).map_err(AppError::internal)?;
    let mut response = Json(Me::from(profile)).into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// End the current session.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    operation_id = "logout",
    security(("session" = [])),
    responses(
        (status = 204, description = "Logged out; cookie cleared"),
        (status = 401, description = "Not logged in", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn logout(
    State(state): State<AuthState>,
    _auth: Authenticated,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(value) = cookie_value(&headers) {
        state
            .sessions
            .revoke(value, RevokeReason::Logout)
            .await
            .map_err(AppError::internal)?;
    }
    let cookie = HeaderValue::from_str(&clear_cookie()).map_err(AppError::internal)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(response)
}

/// The signed-in account.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    operation_id = "getMe",
    security(("session" = [])),
    responses(
        (status = 200, description = "Current account", body = Me),
        (status = 401, description = "Not logged in", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn me(
    State(state): State<AuthState>,
    Authenticated(session): Authenticated,
    headers: HeaderMap,
) -> Result<Json<Me>, AppError> {
    let profile = accounts::find_profile(&state.db, session.account_id)
        .await
        .map_err(AppError::internal)?;
    match profile {
        Some(p) if p.status == DbAccountStatus::Active => Ok(Json(Me::from(p))),
        _ => {
            // Account deleted or blocked after login: end the session.
            if let Some(value) = cookie_value(&headers) {
                let _ = state.sessions.revoke(value, RevokeReason::Security).await;
            }
            Err(AppError::Unauthorized)
        }
    }
}

struct SessionCookieScheme;

impl Modify for SessionCookieScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                dhpanel_session::COOKIE_NAME,
                "HttpOnly session cookie set by POST /api/v1/public/auth/login",
            ))),
        );
    }
}

/// OpenAPI fragment for this module (full paths; merge into the root document).
#[derive(OpenApi)]
#[openapi(
    paths(login, logout, me),
    components(schemas(LoginRequest, Me)),
    modifiers(&SessionCookieScheme),
    tags((name = "auth", description = "Login, logout and the current account"))
)]
pub struct AuthApi;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn login_request_debug_hides_password() {
        let req = LoginRequest {
            login: "alice".into(),
            password: "hunter2-secret".into(),
        };
        let dbg = format!("{req:?}");
        assert!(dbg.contains("alice") && !dbg.contains("hunter2"), "{dbg}");
    }

    #[test]
    fn lockout_subjects_never_contain_raw_input() {
        let id = Uuid::now_v7();
        assert_eq!(
            lockout_subject(Some(id), "ignored"),
            format!("a:{}", id.simple())
        );
        let s = lockout_subject(None, "victim@example.com");
        assert!(s.starts_with("n:") && !s.contains("victim"), "{s}");
        assert_eq!(s.len(), 2 + 43);
        assert_eq!(s, lockout_subject(None, "victim@example.com"));
        assert_ne!(s, lockout_subject(None, "other@example.com"));
    }

    #[test]
    fn openapi_fragment_has_paths_and_cookie_scheme() {
        let doc = AuthApi::openapi();
        let json = doc.to_json().expect("json");
        for p in [
            "/api/v1/public/auth/login",
            "/api/v1/auth/logout",
            "/api/v1/auth/me",
        ] {
            assert!(json.contains(p), "{p}");
        }
        assert!(json.contains("__Host-dh_session"));
    }
}
