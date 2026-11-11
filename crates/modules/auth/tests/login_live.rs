//! Login / logout / me against real PostgreSQL and Redis.
//! Ignored by default: `eval "$(deploy/dev/devenv.sh test-env)" && cargo test -p dhpanel-mod-auth -- --include-ignored`
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use dhpanel_auth_core::password::{Argon2Params, PasswordHasher};
use dhpanel_cache::{
    RedisHandle,
    lockout::{FailureLimiter, LockoutPolicy},
};
use dhpanel_mod_auth::{AuthState, LoginLimits, router};
use dhpanel_session::{SessionConfig, SessionStore};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const PASSWORD: &str = "Orbit-Lantern-Quiet-Harbor-42";

fn fast_params(iterations: u32) -> Argon2Params {
    Argon2Params {
        memory_kib: 19 * 1024,
        iterations,
        parallelism: 1,
    }
}

fn state(pool: &PgPool, hasher: PasswordHasher) -> AuthState {
    limited_state(pool, hasher, 5, 50, &[])
}

/// Lockout counters live in a namespace unique to this state, so tests and
/// repeated runs never share Redis keys.
fn limited_state(
    pool: &PgPool,
    hasher: PasswordHasher,
    account_threshold: u32,
    ip_threshold: u32,
    trusted_proxies: &[IpAddr],
) -> AuthState {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let redis_handle = redis.clone();
    let ns = Uuid::now_v7().simple().to_string();
    let policy = |threshold| LockoutPolicy {
        threshold,
        window: Duration::from_secs(900),
        lockout: Duration::from_secs(900),
    };
    AuthState {
        db: pool.clone(),
        sessions: SessionStore::new(redis.clone(), pool.clone(), SessionConfig::default()),
        hasher,
        limits: LoginLimits {
            account: FailureLimiter::new(
                redis.clone(),
                format!("test:{ns}:account"),
                policy(account_threshold),
            ),
            ip: FailureLimiter::new(redis, format!("test:{ns}:ip"), policy(ip_threshold)),
        },
        trusted_proxies: trusted_proxies.to_vec().into(),
        redis: redis_handle,
        master_key: dhpanel_auth_core::secretbox::MasterKey::from_bytes(&[42; 32]),
        totp_issuer: "DHPanel Test".into(),
    }
}

fn app(state: AuthState) -> Router {
    router()
        .with_state(state)
        .layer(MockConnectInfo(SocketAddr::from((
            [198, 51, 100, 23],
            40000,
        ))))
}

async fn create_account(
    pool: &PgPool,
    hasher: &PasswordHasher,
    username: &str,
    status: &str,
) -> Uuid {
    let hash = hasher
        .hash(&SecretString::from(PASSWORD.to_owned()))
        .unwrap();
    sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash, status)
         VALUES ('admin', $1, $1 || '@example.com', $2, $3::account_status) RETURNING id",
    )
    .bind(username)
    .bind(hash)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

struct Resp {
    status: StatusCode,
    set_cookie: Option<String>,
    retry_after: Option<u64>,
    body: Value,
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Resp {
    send_with(app, method, uri, cookie, body, &[]).await
}

async fn send_with(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
    extra_headers: &[(&str, &str)],
) -> Resp {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::USER_AGENT, "test-agent/1.0");
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_owned());
    let retry_after = res
        .headers()
        .get(header::RETRY_AFTER)
        .map(|v| v.to_str().unwrap().parse().unwrap());
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Resp {
        status,
        set_cookie,
        retry_after,
        body,
    }
}

fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_owned()
}

async fn login(app: &Router, login: &str, password: &str, cookie: Option<&str>) -> Resp {
    send(
        app,
        "POST",
        "/api/v1/public/auth/login",
        cookie,
        Some(json!({"login": login, "password": password})),
    )
    .await
}

async fn events(pool: &PgPool) -> Vec<(Option<Uuid>, String, bool)> {
    sqlx::query_as("SELECT account_id, reason, success FROM login_events ORDER BY at, id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn login_me_logout_flow(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &hasher, "alice", "active").await;
    let app = app(state(&pool, hasher));

    let r = login(&app, "ALICE", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["username"], "alice");
    assert_eq!(r.body["kind"], "admin");
    assert!(r.body.get("password_hash").is_none());
    let set = r.set_cookie.expect("cookie");
    for attr in [
        "__Host-dh_session=",
        "Secure",
        "HttpOnly",
        "SameSite=Strict",
        "Path=/",
    ] {
        assert!(set.contains(attr), "{set}");
    }
    let cookie = cookie_pair(&set);
    let csrf = r.body["csrf_token"]
        .as_str()
        .expect("csrf_token")
        .to_owned();
    assert_eq!(csrf.len(), 43);

    let r = send(&app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["id"], id.to_string());
    assert_eq!(r.body["csrf_token"], csrf.as_str());

    let r = send_with(
        &app,
        "POST",
        "/api/v1/auth/logout",
        Some(&cookie),
        None,
        &[("x-csrf-token", &csrf)],
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(r.set_cookie.unwrap().contains("Max-Age=0"));

    let r = send(&app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    let (ip, ua): (String, String) =
        sqlx::query_as("SELECT host(ip), user_agent FROM login_events WHERE success")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (ip.as_str(), ua.as_str()),
        ("198.51.100.23", "test-agent/1.0")
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn login_by_email_is_case_insensitive(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "bob", "active").await;
    let app = app(state(&pool, hasher));
    let r = login(&app, "  Bob@Example.COM ", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn wrong_password_and_unknown_user_are_indistinguishable(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &hasher, "carol", "active").await;
    let app = app(state(&pool, hasher));

    let wrong = login(&app, "carol", "Not-The-Password-1", None).await;
    let unknown = login(&app, "nobody-here", PASSWORD, None).await;
    let malformed = login(&app, "x", PASSWORD, None).await;
    for r in [&wrong, &unknown, &malformed] {
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.body["code"], "invalid_credentials");
        assert_eq!(r.body["detail"], wrong.body["detail"]);
        assert!(r.set_cookie.is_none());
    }
    assert_eq!(
        events(&pool).await,
        vec![
            (Some(id), "bad_credentials".to_owned(), false),
            (None, "bad_credentials".to_owned(), false),
            (None, "bad_credentials".to_owned(), false),
        ]
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn suspended_status_is_revealed_only_after_correct_password(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "dave", "suspended").await;
    let app = app(state(&pool, hasher));

    let r = login(&app, "dave", "Wrong-Password-Here-9", None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["code"], "invalid_credentials");

    let r = login(&app, "dave", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], "account_suspended");
    assert!(r.set_cookie.is_none());
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn login_rotates_any_previous_session(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "erin", "active").await;
    let app = app(state(&pool, hasher));

    let first = cookie_pair(
        &login(&app, "erin", PASSWORD, None)
            .await
            .set_cookie
            .unwrap(),
    );
    let second = cookie_pair(
        &login(&app, "erin", PASSWORD, Some(&first))
            .await
            .set_cookie
            .unwrap(),
    );
    assert_ne!(first, second);
    assert_eq!(
        send(&app, "GET", "/api/v1/auth/me", Some(&first), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, "GET", "/api/v1/auth/me", Some(&second), None)
            .await
            .status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn weaker_stored_hash_is_upgraded_on_login(pool: PgPool) {
    let old = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &old, "frank", "active").await;
    let current = PasswordHasher::new(fast_params(3)).unwrap();
    let app = app(state(&pool, current));

    assert_eq!(
        login(&app, "frank", PASSWORD, None).await.status,
        StatusCode::OK
    );
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(hash.contains("t=3"), "{hash}");
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn oversized_password_is_rejected_without_hashing(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "grace", "active").await;
    let app = app(state(&pool, hasher));
    let started = std::time::Instant::now();
    let r = login(&app, "grace", &"x".repeat(5000), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn me_ends_session_when_account_is_disabled_after_login(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &hasher, "heidi", "active").await;
    let app = app(state(&pool, hasher));
    let cookie = cookie_pair(
        &login(&app, "heidi", PASSWORD, None)
            .await
            .set_cookie
            .unwrap(),
    );

    sqlx::query("UPDATE accounts SET status = 'disabled' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(&app, "GET", "/api/v1/auth/me", Some(&cookie), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let reason: String =
        sqlx::query_scalar("SELECT revoke_reason FROM sessions WHERE account_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason, "security");
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn unknown_fields_and_non_json_bodies_are_rejected(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let app = app(state(&pool, hasher));
    let r = send(
        &app,
        "POST",
        "/api/v1/public/auth/login",
        None,
        Some(json!({"login": "a", "password": "b", "admin": true})),
    )
    .await;
    assert!(r.status.is_client_error());
    // A cross-site HTML form cannot send application/json.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/public/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("login=a&password=b"))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn sixth_attempt_after_five_failures_is_429_even_with_correct_password(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &hasher, "ivan", "active").await;
    let app = app(state(&pool, hasher));

    // Mixing username and email still counts against the one account.
    for (n, who) in [
        "ivan",
        "IVAN@example.com",
        "ivan",
        "ivan@example.com",
        "ivan",
    ]
    .iter()
    .enumerate()
    {
        let r = login(&app, who, "Wrong-Password-Here-9", None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "attempt {}", n + 1);
    }
    let r = login(&app, "ivan", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{:?}", r.body);
    assert_eq!(r.body["code"], "too_many_attempts");
    assert!(r.set_cookie.is_none());
    let retry = r.retry_after.expect("Retry-After");
    assert!((1..=900).contains(&retry), "{retry}");

    let evs = events(&pool).await;
    assert_eq!(evs.len(), 6);
    assert_eq!(evs[5], (Some(id), "locked_out".to_owned(), false));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn unknown_login_names_lock_out_the_same_way(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let app = app(state(&pool, hasher));
    for _ in 0..5 {
        let r = login(&app, "ghost-user", PASSWORD, None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    let r = login(&app, "Ghost-User", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // A different name from the same address is not affected by the per-name lock.
    let r = login(&app, "someone-else", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn successful_login_resets_the_failure_count(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "judy", "active").await;
    let app = app(state(&pool, hasher));
    for _ in 0..4 {
        login(&app, "judy", "Wrong-Password-Here-9", None).await;
    }
    assert_eq!(
        login(&app, "judy", PASSWORD, None).await.status,
        StatusCode::OK
    );
    for _ in 0..4 {
        let r = login(&app, "judy", "Wrong-Password-Here-9", None).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        login(&app, "judy", PASSWORD, None).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn spraying_many_names_from_one_address_locks_the_address(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "kate", "active").await;
    let app = app(limited_state(&pool, hasher, 5, 3, &[]));
    for name in ["spray-a", "spray-b", "spray-c"] {
        assert_eq!(
            login(&app, name, PASSWORD, None).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    // Even a real account with the right password is refused from this address.
    let r = login(&app, "kate", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.retry_after.is_some());
    let last = events(&pool).await.pop().unwrap();
    assert_eq!(last, (None, "rate_limited".to_owned(), false));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn client_ip_comes_from_trusted_proxy_header_only(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "leo", "active").await;
    // The mock peer (198.51.100.23) plays the trusted reverse proxy.
    let proxy: IpAddr = "198.51.100.23".parse().unwrap();
    let app = app(limited_state(&pool, hasher, 5, 2, &[proxy]));
    let body = || Some(json!({"login": "leo", "password": "Wrong-Password-Here-9"}));
    let uri = "/api/v1/public/auth/login";

    // Two clients behind the proxy, each with a spoofed left-most entry.
    for client in ["203.0.113.1", "203.0.113.2"] {
        let xff = format!("6.6.6.6, {client}");
        let r = send_with(
            &app,
            "POST",
            uri,
            None,
            body(),
            &[("x-forwarded-for", &xff)],
        )
        .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    // Each client has one failure; neither address is locked (threshold 2).
    let r = send_with(
        &app,
        "POST",
        uri,
        None,
        body(),
        &[("x-forwarded-for", "203.0.113.1")],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // That was 203.0.113.1's second failure: it is now locked, the other is not.
    let r = send_with(
        &app,
        "POST",
        uri,
        None,
        body(),
        &[("x-forwarded-for", "203.0.113.1")],
    )
    .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let r = send_with(
        &app,
        "POST",
        uri,
        None,
        body(),
        &[("x-forwarded-for", "203.0.113.2")],
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    let ips: Vec<String> = sqlx::query_scalar("SELECT host(ip) FROM login_events ORDER BY at, id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        ips,
        [
            "203.0.113.1",
            "203.0.113.2",
            "203.0.113.1",
            "203.0.113.1",
            "203.0.113.2"
        ]
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn logout_without_csrf_header_is_403_and_keeps_the_session(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "mallory", "active").await;
    let app = app(state(&pool, hasher));
    let r = login(&app, "mallory", PASSWORD, None).await;
    let cookie = cookie_pair(&r.set_cookie.unwrap());
    let csrf = r.body["csrf_token"].as_str().unwrap().to_owned();

    // What a cross-site request riding the cookie would look like.
    let r = send(&app, "POST", "/api/v1/auth/logout", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], "csrf_failed");
    assert!(r.set_cookie.is_none());

    // Change the last character, guaranteeing a different token.
    let last = if csrf.ends_with('A') { 'B' } else { 'A' };
    let wrong = format!("{}{last}", &csrf[..42]);
    assert_ne!(wrong, csrf);
    let r = send_with(
        &app,
        "POST",
        "/api/v1/auth/logout",
        Some(&cookie),
        None,
        &[("x-csrf-token", &wrong)],
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // The session is untouched by the rejected requests.
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::OK);

    // A new login issues a new CSRF token; the old one no longer works.
    let r2 = login(&app, "mallory", PASSWORD, Some(&cookie)).await;
    let cookie2 = cookie_pair(&r2.set_cookie.unwrap());
    let csrf2 = r2.body["csrf_token"].as_str().unwrap().to_owned();
    assert_ne!(csrf, csrf2);
    let r = send_with(
        &app,
        "POST",
        "/api/v1/auth/logout",
        Some(&cookie2),
        None,
        &[("x-csrf-token", &csrf)],
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = send_with(
        &app,
        "POST",
        "/api/v1/auth/logout",
        Some(&cookie2),
        None,
        &[("x-csrf-token", &csrf2)],
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
}

// ---------------------------------------------------------------- 2FA (T01.08)

use dhpanel_auth_core::totp::TotpSecret;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A logged-in browser: session cookie + CSRF token.
struct Browser {
    cookie: String,
    csrf: String,
}

impl Browser {
    async fn post_empty(&self, app: &Router, uri: &str) -> Resp {
        send_with(
            app,
            "POST",
            uri,
            Some(&self.cookie),
            None,
            &[("x-csrf-token", &self.csrf)],
        )
        .await
    }

    async fn post(&self, app: &Router, uri: &str, body: Value) -> Resp {
        send_with(
            app,
            "POST",
            uri,
            Some(&self.cookie),
            Some(body),
            &[("x-csrf-token", &self.csrf)],
        )
        .await
    }
}

async fn sign_in(app: &Router, name: &str) -> Browser {
    let r = login(app, name, PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    Browser {
        cookie: cookie_pair(&r.set_cookie.unwrap()),
        csrf: r.body["csrf_token"].as_str().unwrap().to_owned(),
    }
}

/// Enrols and confirms TOTP; returns the secret and the backup codes.
async fn enable_totp(app: &Router, b: &Browser) -> (TotpSecret, Vec<String>) {
    let r = b.post_empty(app, "/api/v1/auth/mfa/totp/enroll").await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let base32 = r.body["secret"].as_str().unwrap();
    assert!(
        r.body["otpauth_uri"]
            .as_str()
            .unwrap()
            .contains(&format!("secret={base32}"))
    );
    let secret = secret_from_base32(base32);
    let r = b
        .post(
            app,
            "/api/v1/auth/mfa/totp/confirm",
            json!({"code": secret.code_at(unix_now())}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let codes: Vec<String> = r.body["backup_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(codes.len(), 10);
    (secret, codes)
}

/// RFC 4648 base32 (no padding) decoder, enough for 20-byte secrets.
fn secret_from_base32(text: &str) -> TotpSecret {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = 0u32;
    let mut acc = 0u32;
    let mut out = Vec::new();
    for c in text.bytes() {
        let v = ALPHABET.iter().position(|a| *a == c).expect("base32") as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    TotpSecret::from_bytes(&out).expect("20 bytes")
}

/// First login step for a 2FA account; returns the pending cookie.
async fn password_step(app: &Router, name: &str) -> String {
    let r = login(app, name, PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    assert_eq!(r.body["mfa_required"], true);
    assert!(r.body.get("csrf_token").is_none());
    let set = r.set_cookie.expect("pending cookie");
    for attr in [
        "__Host-dh_mfa=",
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
        "Max-Age=300",
    ] {
        assert!(set.contains(attr), "{set}");
    }
    assert!(
        !set.contains("__Host-dh_session"),
        "no session before the second factor"
    );
    cookie_pair(&set)
}

async fn mfa_step(app: &Router, pending: Option<&str>, code: &str) -> Resp {
    send(
        app,
        "POST",
        "/api/v1/public/auth/login/mfa",
        pending,
        Some(json!({"code": code})),
    )
    .await
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn account_with_2fa_cannot_log_in_without_the_second_step(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    let id = create_account(&pool, &hasher, "nina", "active").await;
    let app = app(state(&pool, hasher));
    let browser = sign_in(&app, "nina").await;

    let r = send(&app, "GET", "/api/v1/auth/mfa", Some(&browser.cookie), None).await;
    assert_eq!(
        r.body,
        json!({"totp_enabled": false, "backup_codes_remaining": 0})
    );
    let (secret, _codes) = enable_totp(&app, &browser).await;
    let r = send(&app, "GET", "/api/v1/auth/mfa", Some(&browser.cookie), None).await;
    assert_eq!(
        r.body,
        json!({"totp_enabled": true, "backup_codes_remaining": 10})
    );
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&browser.cookie), None).await;
    assert_eq!(r.body["mfa_enabled"], true);

    // The secret is not readable in the database.
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT secret_enc FROM account_mfa WHERE account_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!stored.windows(20).any(|w| w == secret.as_bytes()));

    // Password alone: 202, a pending cookie, and no way to reach the API with it.
    let pending = password_step(&app, "nina").await;
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&pending), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let as_session = pending.replace("__Host-dh_mfa", "__Host-dh_session");
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&as_session), None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // Second step with a code from the next time step (the confirming step is spent).
    let code = secret.code_at(unix_now() + 30);
    let r = mfa_step(&app, Some(&pending), &code).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["username"], "nina");
    assert_eq!(r.body["mfa_enabled"], true);
    let session = cookie_pair(&r.set_cookie.unwrap());
    assert!(session.starts_with("__Host-dh_session="), "{session}");
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&session), None).await;
    assert_eq!(r.status, StatusCode::OK);

    let reasons: Vec<String> =
        sqlx::query_scalar("SELECT reason FROM login_events ORDER BY at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(reasons, ["ok", "mfa_required", "ok"]);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn totp_codes_and_pending_logins_are_single_use(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "oscar", "active").await;
    let app = app(state(&pool, hasher));
    let browser = sign_in(&app, "oscar").await;
    let (secret, _) = enable_totp(&app, &browser).await;

    let code = secret.code_at(unix_now() + 30);
    let pending = password_step(&app, "oscar").await;
    assert_eq!(
        mfa_step(&app, Some(&pending), &code).await.status,
        StatusCode::OK
    );

    // The pending login is gone after success.
    let r = mfa_step(&app, Some(&pending), &code).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["code"], "mfa_session_expired");

    // A fresh password step cannot reuse the same code (replay).
    let pending = password_step(&app, "oscar").await;
    let r = mfa_step(&app, Some(&pending), &code).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.body["code"], "invalid_mfa_code");

    // No pending cookie, or a forged one: expired, not a code check.
    for cookie in [
        None,
        Some("__Host-dh_mfa=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
    ] {
        let r = mfa_step(&app, cookie, &code).await;
        assert_eq!(r.body["code"], "mfa_session_expired", "{cookie:?}");
    }
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn backup_codes_work_once_in_any_typed_form(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "peggy", "active").await;
    let app = app(state(&pool, hasher));
    let browser = sign_in(&app, "peggy").await;
    let (_, codes) = enable_totp(&app, &browser).await;

    let typed = codes[0].to_uppercase().replace('-', " ");
    let pending = password_step(&app, "peggy").await;
    let r = mfa_step(&app, Some(&pending), &typed).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);

    let pending = password_step(&app, "peggy").await;
    let r = mfa_step(&app, Some(&pending), &codes[0]).await;
    assert_eq!(
        r.body["code"], "invalid_mfa_code",
        "a backup code works once"
    );
    let r = mfa_step(&app, Some(&pending), &codes[1]).await;
    assert_eq!(
        r.status,
        StatusCode::OK,
        "pending login survives a wrong code"
    );

    let session = cookie_pair(&r.set_cookie.unwrap());
    let r = send(&app, "GET", "/api/v1/auth/mfa", Some(&session), None).await;
    assert_eq!(r.body["backup_codes_remaining"], 8);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn wrong_second_factors_lock_the_account_despite_correct_passwords(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "quinn", "active").await;
    let app = app(state(&pool, hasher));
    let browser = sign_in(&app, "quinn").await;
    let (secret, _) = enable_totp(&app, &browser).await;

    // An attacker who knows the password guesses codes, redoing the password
    // step each time; the correct password must not reset the counter.
    for attempt in 1..=5 {
        let pending = password_step(&app, "quinn").await;
        let r = mfa_step(&app, Some(&pending), "000000").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "attempt {attempt}");
        assert_eq!(r.body["code"], "invalid_mfa_code");
    }
    let r = login(&app, "quinn", PASSWORD, None).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.retry_after.is_some());
    let _ = secret;
}

/// Like `state`, but sudo mode lasts only `sudo_secs`.
fn sudo_state(pool: &PgPool, hasher: PasswordHasher, sudo_secs: u64) -> AuthState {
    let mut st = state(pool, hasher);
    st.sessions = SessionStore::new(
        st.redis.clone(),
        pool.clone(),
        SessionConfig {
            sudo_window: Duration::from_secs(sudo_secs),
            ..SessionConfig::default()
        },
    );
    st
}

const ENROLL: &str = "/api/v1/auth/mfa/totp/enroll";
const CONFIRM: &str = "/api/v1/auth/mfa/totp/confirm";
const DISABLE: &str = "/api/v1/auth/mfa/totp/disable";
const SUDO: &str = "/api/v1/auth/sudo";

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn enrolment_needs_csrf_and_a_valid_code_and_can_be_restarted(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "ruth", "active").await;
    let app = app(state(&pool, hasher));
    let b = sign_in(&app, "ruth").await;

    // No CSRF header.
    let r = send(&app, "POST", ENROLL, Some(&b.cookie), None).await;
    assert_eq!(r.body["code"], "csrf_failed");
    // Nothing to confirm yet.
    let r = b.post(&app, CONFIRM, json!({"code": "123456"})).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["code"], "mfa_not_pending");

    // Pending enrolment does not gate login, and a wrong code does not confirm it.
    let r = b.post_empty(&app, ENROLL).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let first_secret = r.body["secret"].as_str().unwrap().to_owned();
    let r = b.post(&app, CONFIRM, json!({"code": "12345"})).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["code"], "invalid_mfa_code");
    assert_eq!(
        login(&app, "ruth", PASSWORD, None).await.status,
        StatusCode::OK
    );

    // Enrolling again issues a different secret; then confirm it.
    let b = sign_in(&app, "ruth").await;
    let r = b.post_empty(&app, ENROLL).await;
    assert_ne!(r.body["secret"].as_str().unwrap(), first_secret);
    let secret = secret_from_base32(r.body["secret"].as_str().unwrap());
    let r = b
        .post(&app, CONFIRM, json!({"code": secret.code_at(unix_now())}))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    // Already enabled.
    let r = b.post_empty(&app, ENROLL).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.body["code"], "mfa_already_enabled");

    // Disabling (still inside the sudo window of this login) makes login one step again.
    assert_eq!(
        login(&app, "ruth", PASSWORD, None).await.status,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        b.post_empty(&app, DISABLE).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        login(&app, "ruth", PASSWORD, None).await.status,
        StatusCode::OK
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM mfa_backup_codes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn sensitive_actions_ask_for_the_password_again_after_the_sudo_window(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "sybil", "active").await;
    let app = app(sudo_state(&pool, hasher, 2));
    let b = sign_in(&app, "sybil").await;

    // Right after login the sensitive action is allowed.
    assert_eq!(b.post_empty(&app, ENROLL).await.status, StatusCode::OK);

    tokio::time::sleep(Duration::from_millis(3100)).await;
    let r = b.post_empty(&app, ENROLL).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], "reauth_required");
    // Ordinary endpoints are unaffected.
    let r = send(&app, "GET", "/api/v1/auth/me", Some(&b.cookie), None).await;
    assert_eq!(r.status, StatusCode::OK);

    // Wrong password, missing CSRF: no sudo.
    let r = b
        .post(&app, SUDO, json!({"password": "Wrong-Password-Here-9"}))
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], "password_incorrect");
    let r = send(
        &app,
        "POST",
        SUDO,
        Some(&b.cookie),
        Some(json!({"password": PASSWORD})),
    )
    .await;
    assert_eq!(r.body["code"], "csrf_failed");
    assert_eq!(
        b.post_empty(&app, ENROLL).await.body["code"],
        "reauth_required"
    );

    // Correct password: allowed again, for the configured window only.
    let r = b.post(&app, SUDO, json!({"password": PASSWORD})).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["sudo_expires_in"], 2);
    assert_eq!(b.post_empty(&app, ENROLL).await.status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(3100)).await;
    assert_eq!(
        b.post_empty(&app, DISABLE).await.body["code"],
        "reauth_required"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn with_2fa_on_sudo_needs_a_second_factor_so_a_password_cannot_disable_2fa(pool: PgPool) {
    let hasher = PasswordHasher::new(fast_params(2)).unwrap();
    create_account(&pool, &hasher, "trent", "active").await;
    let app = app(sudo_state(&pool, hasher, 2));
    let b = sign_in(&app, "trent").await;
    let (secret, codes) = enable_totp(&app, &b).await;
    tokio::time::sleep(Duration::from_millis(3100)).await;

    // A thief with the session cookie and the password still cannot switch 2FA off.
    assert_eq!(
        b.post_empty(&app, DISABLE).await.body["code"],
        "reauth_required"
    );
    let r = b.post(&app, SUDO, json!({"password": PASSWORD})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], "mfa_code_required");
    let r = b
        .post(&app, SUDO, json!({"password": PASSWORD, "code": "000000"}))
        .await;
    assert_eq!(r.body["code"], "invalid_mfa_code");
    // The code that confirmed enrolment was already used: a replay is refused too.
    let spent = secret.code_at(unix_now());
    let r = b
        .post(&app, SUDO, json!({"password": PASSWORD, "code": spent}))
        .await;
    assert_ne!(r.status, StatusCode::OK);
    assert_eq!(
        b.post_empty(&app, DISABLE).await.body["code"],
        "reauth_required"
    );
    let r = send(&app, "GET", "/api/v1/auth/mfa", Some(&b.cookie), None).await;
    assert_eq!(r.body["totp_enabled"], true);

    // With a real second factor (here a backup code) sudo is granted and 2FA can be disabled.
    let r = b
        .post(&app, SUDO, json!({"password": PASSWORD, "code": codes[0]}))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(
        b.post_empty(&app, DISABLE).await.status,
        StatusCode::NO_CONTENT
    );
    let r = send(&app, "GET", "/api/v1/auth/mfa", Some(&b.cookie), None).await;
    assert_eq!(r.body["totp_enabled"], false);
}
