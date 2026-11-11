//! Login / logout / me against real PostgreSQL and Redis.
//! Ignored by default: `eval "$(deploy/dev/devenv.sh test-env)" && cargo test -p dhpanel-mod-auth -- --include-ignored`
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{net::SocketAddr, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use dhpanel_auth_core::password::{Argon2Params, PasswordHasher};
use dhpanel_cache::RedisHandle;
use dhpanel_mod_auth::{AuthState, router};
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
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    AuthState {
        db: pool.clone(),
        sessions: SessionStore::new(redis, pool.clone(), SessionConfig::default()),
        hasher,
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
    body: Value,
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Resp {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::USER_AGENT, "test-agent/1.0");
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

    let r = send(&app, "GET", "/api/v1/auth/me", Some(&cookie), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["id"], id.to_string());

    let r = send(&app, "POST", "/api/v1/auth/logout", Some(&cookie), None).await;
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
