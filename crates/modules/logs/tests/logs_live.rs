//! The log API over HTTP against real PostgreSQL and Redis (T10.04). The node is played by
//! the test. Ignored by default; run with DATABASE_URL, REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use builtpanel_cache::RedisHandle;
use builtpanel_database::modules as db;
use builtpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use builtpanel_mod_logs::{LogsState, client_router};
use builtpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";

struct Fake {
    manifest: Manifest,
    routes: Box<dyn Fn() -> ModuleRoutes + Send + Sync>,
}

impl Module for Fake {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    fn routes(&self) -> ModuleRoutes {
        (self.routes)()
    }
}

struct Env {
    app: Router,
    sessions: SessionStore,
}

struct Browser {
    cookie: String,
    csrf: String,
}

async fn env(pool: &PgPool, wait: Duration) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let manifest = Manifest::parse(
        "logs",
        "[module]\nname = \"logs\"\nversion = \"1.0.0\"\ntitle = { en = \"Logs\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions) = (pool.clone(), sessions.clone());
            move || ModuleRoutes {
                client: Some(client_router().with_state(LogsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    wait,
                })),
                ..ModuleRoutes::none()
            }
        }),
    });
    let registry = Registry::build(vec![module]).unwrap();
    let values: Vec<Value> = registry
        .modules()
        .iter()
        .map(|m| serde_json::to_value(m.manifest()).unwrap())
        .collect();
    let records: Vec<db::ModuleRecord<'_>> = registry
        .modules()
        .iter()
        .zip(&values)
        .map(|(m, json)| db::ModuleRecord {
            name: &m.manifest().module.name,
            version: &m.manifest().module.version,
            category: m.manifest().module.category.as_str(),
            core: m.manifest().module.core,
            manifest: json,
        })
        .collect();
    db::sync(pool, &records).await.unwrap();
    let mount = ModuleMount::new(registry, Router::new());
    mount.rebuild();
    mount.apply(&db::enabled_names(pool).await.unwrap().into_iter().collect());
    Env {
        app: mount.service(),
        sessions,
    }
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

    async fn get(&self, who: &Browser, path: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .header(header::COOKIE, &who.cookie)
            .header(CSRF_HEADER, &who.csrf)
            .body(Body::empty())
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

async fn client(pool: &PgPool, name: &str, uid: i32) -> (Uuid, Uuid) {
    let account: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('client', $1, $1 || '@example.com', $2) RETURNING id",
    )
    .bind(name)
    .bind(HASH)
    .fetch_one(pool)
    .await
    .unwrap();
    let node: Uuid = sqlx::query_scalar(
        "INSERT INTO nodes (name) VALUES ('node1') ON CONFLICT (name) DO UPDATE SET name = 'node1' RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let plan: Uuid = sqlx::query_scalar("SELECT id FROM plans WHERE is_default")
        .fetch_one(pool)
        .await
        .unwrap();
    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (owner_account_id, node_id, plan_id, username, uid, status)
         VALUES ($1, $2, $3, $4, $5, 'active') RETURNING id",
    )
    .bind(account)
    .bind(node)
    .bind(plan)
    .bind(name)
    .bind(uid)
    .fetch_one(pool)
    .await
    .unwrap();
    (account, tenant)
}

/// Plays the node: answers every `read_log` with two lines (and fails ones for "broken").
fn node(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let rows: Vec<(Uuid, Value)> = sqlx::query_as(
                "SELECT id, payload FROM operations WHERE kind = 'read_log' AND status = 'pending'",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            for (id, payload) in rows {
                sqlx::query(
                    "UPDATE operations SET status = 'dispatched', attempts = 1 WHERE id = $1",
                )
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
                if payload["username"] == "broken" {
                    sqlx::query("UPDATE operations SET status = 'failed', error = '{\"code\":\"not_found\",\"message\":\"x\",\"retryable\":false}', finished_at = now() WHERE id = $1")
                        .bind(id)
                        .execute(&pool)
                        .await
                        .unwrap();
                    continue;
                }
                let out = json!({"type": "log_lines", "truncated": false, "lines": [
                    {"at_ms": payload["since_ms"].as_i64().unwrap() + 1, "text": format!("{} {} {}", payload["source"], payload["tail"], payload["contains"])},
                ]});
                sqlx::query("UPDATE operations SET status = 'succeeded', output = $2, finished_at = now() WHERE id = $1")
                    .bind(id)
                    .bind(out)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn logs_are_read_through_the_node_for_the_owner_only(pool: PgPool) {
    let e = env(&pool, Duration::from_secs(5)).await;
    let (alice_account, _) = client(&pool, "alice", 10_000).await;
    let (broken_account, _) = client(&pool, "broken", 10_001).await;
    let alice = e.sign_in(alice_account).await;
    let broken = e.sign_in(broken_account).await;
    let nobody = {
        let admin: Uuid = sqlx::query_scalar(
            "INSERT INTO accounts (kind, username, email, password_hash)
             VALUES ('admin', 'rootuser', 'root@example.com', $1) RETURNING id",
        )
        .bind(HASH)
        .fetch_one(&pool)
        .await
        .unwrap();
        e.sign_in(admin).await
    };
    let played = node(pool.clone());

    let (status, got) = e
        .get(&nobody, "/api/v1/client/logs?source=web_access")
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{got}");
    let (status, got) = e
        .get(
            &alice,
            "/api/v1/client/logs?source=web_error&lines=50&search=oops",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["lines"][0]["text"], "\"web_error\" 50 \"oops\"");
    // The cursor of a live view is exclusive: the node is asked from the next millisecond.
    let (_, got) = e
        .get(&alice, "/api/v1/client/logs?source=php_error&since_ms=1000")
        .await;
    assert_eq!(got["lines"][0]["at_ms"], 1002);
    // What is not a log, or not a sensible count, is refused before the node is asked.
    for q in [
        "source=shadow",
        "source=web_access&lines=0",
        "source=web_access&lines=9999",
    ] {
        let (status, got) = e.get(&alice, &format!("/api/v1/client/logs?{q}")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{q}: {got}");
    }
    let asked: i64 = sqlx::query_scalar("SELECT count(*) FROM operations WHERE kind = 'read_log'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(asked, 2);
    // A node that cannot read it.
    let (status, got) = e
        .get(&broken, "/api/v1/client/logs?source=web_access")
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{got}");
    // A plan without the feature has none; a suspended account has nothing to read.
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE username = 'alice'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, got) = e.get(&alice, "/api/v1/client/logs?source=web_access").await;
    assert_eq!(status, StatusCode::CONFLICT, "{got}");
    sqlx::query("UPDATE feature_sets SET all_features = false, features = ARRAY['files']::feature_key[] WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)").execute(&pool).await.unwrap();
    let (status, got) = e.get(&alice, "/api/v1/client/logs?source=web_access").await;
    assert_eq!(
        (status, got["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("feature_disabled"))
    );
    played.abort();
}
