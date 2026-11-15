//! The firewall policy API over HTTP against real PostgreSQL and Redis (T11.02). The node is played by
//! the test. Ignored by default; run with DATABASE_URL, REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::modules as db;
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_security::{SecurityState, admin_router, client_router};
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

async fn env(pool: &PgPool) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let manifest = Manifest::parse(
        "security",
        "[module]\nname = \"logs\"\nversion = \"1.0.0\"\ntitle = { en = \"Security\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions) = (pool.clone(), sessions.clone());
            move || ModuleRoutes {
                admin: Some(admin_router().with_state(SecurityState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                })),
                client: Some(client_router().with_state(SecurityState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
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

    async fn call(
        &self,
        who: &Browser,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &who.cookie)
            .header(CSRF_HEADER, &who.csrf);
        let req = match body {
            Some(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                req.body(Body::from(v.to_string())).unwrap()
            }
            None => req.body(Body::empty()).unwrap(),
        };
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

async fn domain(pool: &PgPool, tenant: Uuid, fqdn: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO domains (tenant_id, fqdn, kind) VALUES ($1, $2, 'primary') RETURNING id",
    )
    .bind(tenant)
    .bind(fqdn)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_most_specific_policy_decides_and_clients_are_kept_to_their_own_domains(pool: PgPool) {
    let e = env(&pool).await;
    let (alice_account, alice) = client(&pool, "alice", 10_000).await;
    let (bob_account, bob) = client(&pool, "bob", 10_001).await;
    let (a_dom, b_dom) = (
        domain(&pool, alice, "alice.example.com").await,
        domain(&pool, bob, "bob.example.com").await,
    );
    let a = e.sign_in(alice_account).await;
    let b = e.sign_in(bob_account).await;
    let admin_id: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('admin', 'rootuser', 'root@example.com', $1) RETURNING id",
    )
    .bind(HASH)
    .fetch_one(&pool)
    .await
    .unwrap();
    dhpanel_database::rbac::assign_role(&pool, admin_id, "super_admin", None)
        .await
        .unwrap();
    let root = e.sign_in(admin_id).await;
    let plain_id: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('admin', 'plain', 'plain@example.com', $1) RETURNING id",
    )
    .bind(HASH)
    .fetch_one(&pool)
    .await
    .unwrap();
    let plain = e.sign_in(plain_id).await;
    let synced = |v: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE tenants SET routes_synced_version = $1")
                .bind(v)
                .execute(&pool)
                .await
                .unwrap();
        }
    };
    let version_of = |t: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT routes_synced_version FROM tenants WHERE id = $1")
                .bind(t)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let global = "/api/v1/admin/waf/policy";

    // Nothing set: the default, detect.
    let (status, st) = e.call(&root, "GET", global, None).await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!(
        (
            st["effective"]["mode"].as_str(),
            st["effective"]["mode_from"].as_str()
        ),
        (Some("detect"), Some("default"))
    );
    // Who may do what.
    assert_eq!(
        e.call(&plain, "GET", global, None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.call(&a, "GET", global, None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.call(&root, "GET", "/api/v1/client/waf/domains", None)
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Global: block, one rule off. Every account's routes are sent again.
    synced(7).await;
    let (status, st) = e
        .call(
            &root,
            "PUT",
            global,
            Some(json!({"mode": "block", "disabled_rules": [942100]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!(st["policy"]["mode"], "block");
    assert_eq!((version_of(alice).await, version_of(bob).await), (0, 0));
    let (_, list) = e.call(&a, "GET", "/api/v1/client/waf/domains", None).await;
    assert_eq!(
        list["items"].as_array().unwrap().len(),
        1,
        "only her own domain: {list}"
    );
    assert_eq!(
        (
            list["items"][0]["effective"]["mode"].as_str(),
            list["items"][0]["effective"]["mode_from"].as_str()
        ),
        (Some("block"), Some("global"))
    );
    assert!(list["items"][0]["policy"].is_null());

    // The account's policy overrides the panel's; only her routes are sent again.
    synced(7).await;
    let (status, _) = e
        .call(
            &root,
            "PUT",
            &format!("/api/v1/admin/waf/tenants/{alice}"),
            Some(json!({"mode": "detect", "disabled_rules": [941100]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((version_of(alice).await, version_of(bob).await), (0, 7));
    let (_, d) = e
        .call(
            &root,
            "GET",
            &format!("/api/v1/admin/waf/domains/{a_dom}"),
            None,
        )
        .await;
    assert_eq!(
        (
            d["effective"]["mode"].as_str(),
            d["effective"]["mode_from"].as_str()
        ),
        (Some("detect"), Some("tenant"))
    );
    assert_eq!(d["effective"]["disabled_rules"], json!([941100, 942100]));
    // Bob keeps the panel's.
    let (_, d) = e
        .call(
            &root,
            "GET",
            &format!("/api/v1/admin/waf/domains/{b_dom}"),
            None,
        )
        .await;
    assert_eq!(d["effective"]["mode_from"], "global");

    // The domain's policy overrides both: an administrator may switch it off, a client may not.
    let own = format!("/api/v1/client/waf/domains/{a_dom}");
    let (status, got) = e.call(&a, "PUT", &own, Some(json!({"mode": "off"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{got}");
    let (status, st) = e
        .call(
            &a,
            "PUT",
            &own,
            Some(json!({"mode": "block", "disabled_rules": [920350]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{st}");
    assert_eq!(
        (
            st["effective"]["mode"].as_str(),
            st["effective"]["mode_from"].as_str()
        ),
        (Some("block"), Some("domain"))
    );
    assert_eq!(
        st["effective"]["disabled_rules"],
        json!([920350, 941100, 942100])
    );
    let (status, st) = e
        .call(
            &root,
            "PUT",
            &format!("/api/v1/admin/waf/domains/{a_dom}"),
            Some(json!({"mode": "off"})),
        )
        .await;
    assert_eq!(
        (status, st["effective"]["mode"].as_str()),
        (StatusCode::OK, Some("off")),
        "{st}"
    );
    // Bad input.
    for bad in [
        json!({"mode": "maybe"}),
        json!({"mode": "block", "disabled_rules": [0]}),
        json!({"mode": "block", "disabled_rules": [1000000]}),
        json!({"mode": "block", "extra": 1}),
    ] {
        let (status, got) = e.call(&a, "PUT", &own, Some(bad.clone())).await;
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "{bad}: {status} {got}"
        );
    }
    let many: Vec<i32> = (1..=51).collect();
    assert_eq!(
        e.call(
            &a,
            "PUT",
            &own,
            Some(json!({"mode": "block", "disabled_rules": many}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Another account's domain, and one that does not exist: not found.
    for id in [b_dom, Uuid::new_v4()] {
        let path = format!("/api/v1/client/waf/domains/{id}");
        assert_eq!(
            e.call(&a, "PUT", &path, Some(json!({"mode": "block"})))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            e.call(&a, "DELETE", &path, None).await.0,
            StatusCode::NOT_FOUND
        );
    }
    // Taking the domain's policy away brings the account's back; taking all away, the default.
    assert_eq!(
        e.call(&a, "DELETE", &own, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(&a, "DELETE", &own, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            &root,
            "DELETE",
            &format!("/api/v1/admin/waf/tenants/{alice}"),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(&root, "DELETE", global, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, d) = e
        .call(
            &root,
            "GET",
            &format!("/api/v1/admin/waf/domains/{a_dom}"),
            None,
        )
        .await;
    assert_eq!(
        (
            d["effective"]["mode"].as_str(),
            d["effective"]["mode_from"].as_str()
        ),
        (Some("detect"), Some("default"))
    );
    // Everything that changed is in the audit log.
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action IN ('waf_policy.set', 'waf_policy.removed')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audited, 7);
    let _ = b;
}
