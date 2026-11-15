//! The services API over HTTP against real PostgreSQL and Redis (T10.12). The node is played by
//! the test. Ignored by default; run with DATABASE_URL, REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::modules as db;
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_services::{ServicesState, client_router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
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
        "services",
        "[module]\nname = \"logs\"\nversion = \"1.0.0\"\ntitle = { en = \"Services\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions) = (pool.clone(), sessions.clone());
            move || ModuleRoutes {
                client: Some(client_router().with_state(ServicesState {
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

    async fn call(&self, who: &Browser, method: &str, path: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
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

/// Plays the node: lists two containers, restarts what is its own and refuses the rest.
fn node(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let rows: Vec<(Uuid, Value)> = sqlx::query_as(
                "SELECT id, payload FROM operations WHERE kind IN ('list_containers', 'restart_container') AND status = 'pending'",
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
                let kind: String = sqlx::query_scalar("SELECT kind FROM operations WHERE id = $1")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                let (status, out, err) = if kind == "list_containers" {
                    (
                        "succeeded",
                        Some(json!({"type": "containers", "items": [
                            {"name": "dh-alice-php", "role": "php", "image": "php:8.4", "state": "running", "status": "Up 2 hours", "created": 1_700_000_000},
                            {"name": "dh-alice-web", "role": "web", "image": "nginx", "state": "exited", "status": "Exited (0) 1 hour ago", "created": 1_700_000_100}
                        ]})),
                        None,
                    )
                } else if payload["name"] == "dh-alice-web" {
                    (
                        "succeeded",
                        Some(
                            json!({"type": "container_restarted", "name": "dh-alice-web", "state": "Up 1 second"}),
                        ),
                        None,
                    )
                } else {
                    (
                        "failed",
                        None,
                        Some(
                            json!({"code": "invalid_arguments", "message": "this account has no such container", "retryable": false}),
                        ),
                    )
                };
                sqlx::query("UPDATE operations SET status = $2, output = $3, error = $4, finished_at = now() WHERE id = $1")
                    .bind(id)
                    .bind(status)
                    .bind(out)
                    .bind(err)
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
async fn an_account_lists_and_restarts_its_services_and_nobody_elses(pool: PgPool) {
    let e = env(&pool, Duration::from_secs(5)).await;
    let (alice_account, _) = client(&pool, "alice", 10_000).await;
    let alice = e.sign_in(alice_account).await;
    let admin_id: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('admin', 'rootuser', 'root@example.com', $1) RETURNING id",
    )
    .bind(HASH)
    .fetch_one(&pool)
    .await
    .unwrap();
    let root = e.sign_in(admin_id).await;
    let played = node(pool.clone());

    assert_eq!(
        e.call(&root, "GET", "/api/v1/client/services").await.0,
        StatusCode::FORBIDDEN
    );
    let (status, list) = e.call(&alice, "GET", "/api/v1/client/services").await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["items"][0]["name"], "dh-alice-php");
    assert_eq!(list["items"][1]["state"], "exited");
    // The node was asked about this account, by its id.
    let asked: Value =
        sqlx::query_scalar("SELECT payload FROM operations WHERE kind = 'list_containers'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM tenants WHERE username = 'alice'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(asked["tenant_id"], tenant.to_string());

    let (status, done) = e
        .call(
            &alice,
            "POST",
            "/api/v1/client/services/dh-alice-web/restart",
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state"], "Up 1 second");
    // What the node refuses is a 404, whatever the reason.
    for name in ["dh-bob-web", "dh-db"] {
        let (status, _) = e
            .call(
                &alice,
                "POST",
                &format!("/api/v1/client/services/{name}/restart"),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}");
    }
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE action = 'service.restarted'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audited, 1);
    // A suspended account cannot restart.
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE username = 'alice'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = e
        .call(
            &alice,
            "POST",
            "/api/v1/client/services/dh-alice-web/restart",
        )
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::CONFLICT, Some("tenant_not_running"))
    );
    played.abort();
}
