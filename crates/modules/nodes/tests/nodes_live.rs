//! Node enrollment tokens over HTTP, against real PostgreSQL and Redis.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::connect_info::MockConnectInfo,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::{node_enrollments::consume_token, rbac::assign_role};
use dhpanel_mod_nodes::{NodesState, router, token};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const BASE: &str = "/api/v1/admin/nodes";
const LIST: &str = "/api/v1/admin/nodes/enrollments";

struct Env {
    app: Router,
    sessions: SessionStore,
    pool: PgPool,
}

struct Browser {
    cookie: String,
    csrf: String,
}

fn env(pool: &PgPool, public_url: Option<&str>, sudo_secs: u64) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let redis_for_limiter = redis.clone();
    let sessions = SessionStore::new(
        redis,
        pool.clone(),
        SessionConfig {
            sudo_window: Duration::from_secs(sudo_secs),
            ..SessionConfig::default()
        },
    );
    let app = router()
        .with_state(NodesState {
            db: pool.clone(),
            sessions: sessions.clone(),
            public_url: public_url.map(Arc::from),
            stream_url: None,
            token_ttl: Duration::from_secs(900),
            ca: None,
            limiter: dhpanel_mod_nodes::enrollment_limiter(redis_for_limiter),
            audit: dhpanel_audit::AuditWriter::new(pool.clone()),
        })
        .layer(MockConnectInfo(std::net::SocketAddr::from((
            [198, 51, 100, 7],
            4000,
        ))));
    Env {
        app,
        sessions,
        pool: pool.clone(),
    }
}

async fn account(pool: &PgPool, name: &str, roles: &[&str]) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('admin', $1, $1 || '@example.com', $2) RETURNING id",
    )
    .bind(name)
    .bind(HASH)
    .fetch_one(pool)
    .await
    .unwrap();
    for role in roles {
        assign_role(pool, id, role, None).await.unwrap();
    }
    id
}

impl Env {
    async fn sign_in(&self, account_id: Uuid) -> Browser {
        let (t, data) = self
            .sessions
            .create(NewSession {
                account_id,
                ip: None,
                user_agent: None,
            })
            .await
            .unwrap();
        Browser {
            cookie: set_cookie(&t).split(';').next().unwrap().to_owned(),
            csrf: data.csrf_token.expose().to_owned(),
        }
    }

    /// Returns status, JSON body and the Cache-Control header.
    async fn call(
        &self,
        who: Option<&Browser>,
        method: &str,
        path: &str,
        body: Option<Value>,
        csrf: bool,
    ) -> (StatusCode, Value, Option<String>) {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(b) = who {
            req = req.header(header::COOKIE, &b.cookie);
            if csrf {
                req = req.header(CSRF_HEADER, &b.csrf);
            }
        }
        let req = match body {
            Some(v) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let cache = res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap().to_owned());
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            cache,
        )
    }

    async fn send(
        &self,
        who: &Browser,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, b, _) = self.call(Some(who), method, path, body, true).await;
        (s, b)
    }

    async fn create(&self, who: &Browser, name: &str) -> (StatusCode, Value) {
        self.send(who, "POST", BASE, Some(json!({"name": name})))
            .await
    }
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

async fn audit(pool: &PgPool) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT action, metadata FROM audit_logs ORDER BY at, id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn only_holders_of_the_manage_permission_can_add_nodes(pool: PgPool) {
    let e = env(&pool, None, 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let plain = account(&pool, "plainadmin", &["admin"]).await;
    let helper = account(&pool, "helper", &["support"]).await;
    let nobody = account(&pool, "norole", &[]).await;

    let (status, body, _) = e
        .call(None, "POST", BASE, Some(json!({"name": "node-1"})), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::UNAUTHORIZED, "unauthenticated")
    );
    for who in [nobody, helper, plain] {
        let b = e.sign_in(who).await;
        let (status, body) = e.create(&b, "node-1").await;
        assert_eq!(
            (status, code(&body)),
            (StatusCode::FORBIDDEN, "permission_denied"),
            "{who}"
        );
    }
    let r = e.sign_in(root).await;
    let (status, body, _) = e
        .call(
            Some(&r),
            "POST",
            BASE,
            Some(json!({"name": "node-1"})),
            false,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "csrf_failed")
    );
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM node_enrollment_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tokens, 0, "nothing was created by the refused requests");

    let (status, body) = e.create(&r, "node-1").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn adding_a_node_needs_a_recent_confirmation_of_identity(pool: PgPool) {
    let e = env(&pool, None, 1);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let r = e.sign_in(root).await;
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let (status, body) = e.create(&r, "node-1").await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "reauth_required")
    );
    e.sessions
        .mark_reauthenticated(r.cookie.split_once('=').unwrap().1)
        .await
        .unwrap();
    assert_eq!(e.create(&r, "node-1").await.0, StatusCode::CREATED);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_token_is_shown_once_and_only_its_hash_is_kept(pool: PgPool) {
    let e = env(&pool, Some("https://panel.example.com:8443"), 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let r = e.sign_in(root).await;

    let (status, body, cache) = e
        .call(
            Some(&r),
            "POST",
            BASE,
            Some(json!({"name": "  eu-west-1 "})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(cache.as_deref(), Some("no-store"));
    let secret = body["token"].as_str().unwrap().to_owned();
    assert!(
        secret.starts_with("dhn_") && secret.len() == token::TOKEN_LEN,
        "{secret}"
    );
    assert_eq!(body["name"], "eu-west-1", "the name is trimmed");
    assert_eq!(body["expires_in"], 900);
    assert_eq!(
        body["enroll_command"],
        format!("dhpanel-agent enroll --server https://panel.example.com:8443 --token {secret}")
    );

    // The database holds the hash of the token, and the token text appears nowhere.
    let (stored_hash, expires_in): (Vec<u8>, f64) = sqlx::query_as(
        "SELECT token_sha256, extract(epoch FROM expires_at - created_at)::float8 FROM node_enrollment_tokens",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_hash, token::hash(&secret).unwrap().to_vec());
    assert!((expires_in - 900.0).abs() < 1.0);
    let everywhere: String = sqlx::query_scalar(
        "SELECT COALESCE(string_agg(t::text, ' '), '') FROM (
             SELECT row_to_json(n)::text AS t FROM node_enrollment_tokens n
             UNION ALL SELECT row_to_json(a)::text FROM audit_logs a) x",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !everywhere.contains(&secret),
        "token text found in the database"
    );

    // Listing never reveals it.
    let (status, list) = e.send(&r, "GET", LIST, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!list.to_string().contains(&secret));
    assert_eq!(list["items"][0]["status"], "pending");
    assert_eq!(list["items"][0]["name"], "eu-west-1");

    // Without a configured public address there is no command, only the token.
    let e2 = env(&pool, None, 600);
    let (_, body) = e2.create(&e2.sign_in(root).await, "eu-west-2").await;
    assert_eq!(body["enroll_command"], Value::Null);
    assert!(body["token"].is_string());
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_created_token_enrols_once_then_never_again(pool: PgPool) {
    let e = env(&pool, None, 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let r = e.sign_in(root).await;
    let (_, body) = e.create(&r, "node-1").await;
    let hash = token::hash(body["token"].as_str().unwrap()).unwrap();

    let mut conn = e.pool.acquire().await.unwrap();
    let first = consume_token(&mut conn, &hash).await.unwrap();
    assert_eq!(first.map(|c| c.name), Some("node-1".to_owned()));
    assert!(
        consume_token(&mut conn, &hash).await.unwrap().is_none(),
        "second use fails"
    );
    let (_, list) = e.send(&r, "GET", LIST, None).await;
    assert_eq!(list["items"][0]["status"], "consumed");
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn names_are_validated_and_not_handed_out_twice(pool: PgPool) {
    let e = env(&pool, None, 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let r = e.sign_in(root).await;

    for bad in [
        "",
        "a",
        "Node",
        "1node",
        "no_de",
        "-x",
        "with space",
        &format!("a{}", "b".repeat(63)),
    ] {
        let (status, body) = e.create(&r, bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
        assert_eq!(body["errors"][0]["field"], "name");
    }
    let (status, _) = e
        .send(
            &r,
            "POST",
            BASE,
            Some(json!({"name": "ok-name", "extra": 1})),
        )
        .await;
    assert!(status.is_client_error(), "unknown fields are rejected");

    assert_eq!(e.create(&r, "node-1").await.0, StatusCode::CREATED);
    let (status, body) = e.create(&r, "node-1").await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "node_name_taken"),
        "pending token holds the name"
    );
    sqlx::query("INSERT INTO nodes (name) VALUES ('node-2')")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = e.create(&r, "node-2").await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "node_name_taken"),
        "an enrolled node holds the name"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_pending_token_can_be_cancelled_and_is_then_useless(pool: PgPool) {
    let e = env(&pool, None, 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let plain = account(&pool, "plainadmin", &["admin"]).await;
    let r = e.sign_in(root).await;
    let (_, body) = e.create(&r, "node-1").await;
    let id = body["id"].as_str().unwrap().to_owned();
    let hash = token::hash(body["token"].as_str().unwrap()).unwrap();
    let url = format!("{LIST}/{id}");

    // Read-only administrators see tokens but cannot cancel them.
    let p = e.sign_in(plain).await;
    assert_eq!(e.send(&p, "GET", LIST, None).await.0, StatusCode::OK);
    let (status, body) = e.send(&p, "DELETE", &url, None).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );
    let (_, body, _) = e.call(Some(&r), "DELETE", &url, None, false).await;
    assert_eq!(code(&body), "csrf_failed");

    assert_eq!(
        e.send(&r, "DELETE", &url, None).await.0,
        StatusCode::NO_CONTENT
    );
    let mut conn = e.pool.acquire().await.unwrap();
    assert!(
        consume_token(&mut conn, &hash).await.unwrap().is_none(),
        "a cancelled token cannot be used"
    );
    let (_, list) = e.send(&r, "GET", LIST, None).await;
    assert_eq!(list["items"][0]["status"], "revoked");
    // Cancelling twice, an unknown id and a malformed id.
    let (status, body) = e.send(&r, "DELETE", &url, None).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "enrollment_not_pending")
    );
    let (status, _) = e
        .send(&r, "DELETE", &format!("{LIST}/{}", Uuid::now_v7()), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = e.send(&r, "DELETE", &format!("{LIST}/nope"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The name is free again.
    assert_eq!(e.create(&r, "node-1").await.0, StatusCode::CREATED);

    // A token that was used cannot be cancelled either.
    let (_, body) = e.create(&r, "node-2").await;
    consume_token(
        &mut conn,
        &token::hash(body["token"].as_str().unwrap()).unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    let (status, body2) = e
        .send(
            &r,
            "DELETE",
            &format!("{LIST}/{}", body["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(
        (status, code(&body2)),
        (StatusCode::CONFLICT, "enrollment_not_pending")
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn creating_and_cancelling_are_audited_without_the_token(pool: PgPool) {
    let e = env(&pool, Some("https://panel.example.com"), 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let r = e.sign_in(root).await;
    let (_, body) = e.create(&r, "node-1").await;
    let id = body["id"].as_str().unwrap().to_owned();
    e.send(&r, "DELETE", &format!("{LIST}/{id}"), None).await;
    // Refused attempts leave no trace of success.
    e.create(&r, "bad name").await;

    let rows = audit(&pool).await;
    let actions: Vec<&str> = rows.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        [
            "admin.node_enrollment_created",
            "admin.node_enrollment_revoked"
        ]
    );
    assert_eq!(rows[0].1, json!({"name": "node-1", "expires_in": 900}));
    let (actor, target): (Option<Uuid>, String) = sqlx::query_as(
        "SELECT actor_id, target_type || ':' || target_id FROM audit_logs WHERE action = 'admin.node_enrollment_created'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (actor, target),
        (Some(root), format!("node_enrollment:{id}"))
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_node_list_shows_live_status_to_readers_only(pool: PgPool) {
    let e = env(&pool, None, 600);
    let root = account(&pool, "rootuser", &["super_admin"]).await;
    let nobody = account(&pool, "norole", &[]).await;
    for (name, status, silent_secs) in [
        ("fresh-online", "online", 0_i32),
        ("stale-online", "online", 300),
        ("quiet", "offline", 86_400),
    ] {
        sqlx::query(
            "INSERT INTO nodes (name, status, last_seen_at, hostname, agent_version, public_ip)
             VALUES ($1, $2, now() - make_interval(secs => $3), 'box', '0.1.0', '203.0.113.9')",
        )
        .bind(name)
        .bind(status)
        .bind(silent_secs)
        .execute(&pool)
        .await
        .unwrap();
    }

    let (status, _, _) = e.call(None, "GET", BASE, None, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let n = e.sign_in(nobody).await;
    let (status, body, _) = e.call(Some(&n), "GET", BASE, None, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );

    let r = e.sign_in(root).await;
    let (status, body, _) = e.call(Some(&r), "GET", BASE, None, false).await;
    assert_eq!(status, StatusCode::OK);
    let by_name = |name: &str| {
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .clone()
    };
    assert_eq!(by_name("fresh-online")["status"], "online");
    assert_eq!(
        by_name("stale-online")["status"],
        "offline",
        "no heartbeat for minutes is not online, whatever the stored value says"
    );
    assert_eq!(by_name("quiet")["status"], "offline");
    let fresh = by_name("fresh-online");
    assert_eq!(fresh["public_ip"], "203.0.113.9");
    assert_eq!(fresh["agent_version"], "0.1.0");
    assert!(fresh["last_seen_at"].as_str().unwrap().ends_with('Z'));
    // Nothing secret about a node is listed.
    let text = body.to_string();
    assert!(!text.contains("capabilities") && !text.contains("fingerprint"));
}
