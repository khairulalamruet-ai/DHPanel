//! The apps module over HTTP against real PostgreSQL and Redis (T12.01). Ignored by default;
//! run with DATABASE_URL, REDIS_URL and `--include-ignored`.
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
use dhpanel_mod_apps::{AppsState, admin_router, client_router};
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
        "apps",
        "[module]\nname = \"apps\"\nversion = \"1.0.0\"\ntitle = { en = \"Apps\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions) = (pool.clone(), sessions.clone());
            move || ModuleRoutes {
                admin: Some(admin_router().with_state(AppsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    master_key: dhpanel_auth_core::secretbox::MasterKey::from_bytes(&[42; 32]),
                })),
                client: Some(client_router().with_state(AppsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    master_key: dhpanel_auth_core::secretbox::MasterKey::from_bytes(&[42; 32]),
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
    // One primary domain per account; the others are add-ons.
    let kind = if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM domains WHERE tenant_id = $1)",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await
    .unwrap()
    {
        "addon"
    } else {
        "primary"
    };
    sqlx::query_scalar(
        "INSERT INTO domains (tenant_id, fqdn, kind) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(fqdn)
    .bind(kind)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn admin(pool: &PgPool) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO accounts (kind, username, email, password_hash)
         VALUES ('admin', 'rootuser', 'root@example.com', $1) RETURNING id",
    )
    .bind(HASH)
    .fetch_one(pool)
    .await
    .unwrap();
    dhpanel_database::rbac::assign_role(pool, id, "super_admin", None)
        .await
        .unwrap();
    id
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn what_runs_where_is_tracked_one_application_per_place(pool: PgPool) {
    use dhpanel_database::apps::{self, CreateError, NewApp};
    let e = env(&pool).await;
    let (alice_account, alice) = client(&pool, "alice", 10_000).await;
    let (_, bob) = client(&pool, "bob", 10_001).await;
    let a = e.sign_in(alice_account).await;
    let site = domain(&pool, alice, "alice.example").await;
    let other = domain(&pool, alice, "shop.example").await;
    let bobs = domain(&pool, bob, "bob.example").await;
    let settings = json!({});
    let new = |domain_id, path| NewApp {
        tenant_id: alice,
        domain_id,
        path,
        kind: "wordpress",
        name: "My blog",
        version: Some("6.8"),
        database_id: None,
        settings: &settings,
    };
    // A site and a blog under it; the same place twice is refused; a domain that is not the
    // account's is refused.
    let root = apps::create(&pool, &new(Some(site), "/")).await.unwrap();
    let blog = apps::create(&pool, &new(Some(site), "/blog"))
        .await
        .unwrap();
    apps::create(&pool, &new(Some(other), "/")).await.unwrap();
    assert!(matches!(
        apps::create(&pool, &new(Some(site), "/blog")).await,
        Err(CreateError::Taken)
    ));
    assert!(matches!(
        apps::create(&pool, &new(Some(bobs), "/")).await,
        Err(CreateError::NotOwned)
    ));
    // A failed install does not hold its place.
    apps::set_status(&pool, blog, "failed", None, Some("no space"))
        .await
        .unwrap();
    let again = apps::create(&pool, &new(Some(site), "/blog"))
        .await
        .unwrap();
    assert_ne!(again, blog);
    apps::set_status(&pool, again, "active", Some("6.8.1"), None)
        .await
        .unwrap();
    // The client sees their own applications, all or one domain's.
    let (status, list) = e.call(&a, "GET", "/api/v1/client/apps", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["items"].as_array().unwrap().len(), 4, "{list}");
    let (_, on_site) = e
        .call(
            &a,
            "GET",
            &format!("/api/v1/client/apps?domain_id={site}"),
            None,
        )
        .await;
    let paths: Vec<&str> = on_site["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["/", "/blog", "/blog"]);
    assert_eq!(on_site["items"][0]["fqdn"], "alice.example");
    let (status, one) = e
        .call(&a, "GET", &format!("/api/v1/client/apps/{again}"), None)
        .await;
    assert_eq!(
        (status, one["status"].as_str(), one["version"].as_str()),
        (StatusCode::OK, Some("active"), Some("6.8.1"))
    );
    let (status, failed) = e
        .call(&a, "GET", &format!("/api/v1/client/apps/{blog}"), None)
        .await;
    assert_eq!(
        (status, failed["error"].as_str()),
        (StatusCode::OK, Some("no space"))
    );
    // Nobody else's.
    let (bob_account, _) = (
        sqlx::query_scalar::<_, Uuid>("SELECT owner_account_id FROM tenants WHERE id = $1")
            .bind(bob)
            .fetch_one(&pool)
            .await
            .unwrap(),
        (),
    );
    let b = e.sign_in(bob_account).await;
    assert_eq!(
        e.call(&b, "GET", &format!("/api/v1/client/apps/{root}"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, theirs) = e.call(&b, "GET", "/api/v1/client/apps", None).await;
    assert_eq!(theirs["items"].as_array().unwrap().len(), 0);
    // An administrator reads any account's applications; a client may not.
    let root_admin = e.sign_in(admin(&pool).await).await;
    let path = format!("/api/v1/admin/tenants/{alice}/apps");
    assert_eq!(
        e.call(&a, "GET", &path, None).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, all) = e.call(&root_admin, "GET", &path, None).await;
    assert_eq!(
        (status, all["items"].as_array().unwrap().len()),
        (StatusCode::OK, 4)
    );
    assert_eq!(
        e.call(
            &root_admin,
            "GET",
            &format!("/api/v1/admin/tenants/{}/apps", Uuid::new_v4()),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Removing the domain leaves the application, now without one.
    sqlx::query("DELETE FROM domains WHERE id = $1")
        .bind(other)
        .execute(&pool)
        .await
        .unwrap();
    let (_, list) = e.call(&a, "GET", "/api/v1/client/apps", None).await;
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["domain_id"].is_null())
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn wordpress_is_asked_for_with_a_database_of_its_own_and_sealed_passwords(pool: PgPool) {
    let e = env(&pool).await;
    let (alice_account, alice) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice_account).await;
    let site = domain(&pool, alice, "alice.example").await;
    let (_, bob) = client(&pool, "bob", 10_001).await;
    let bobs = domain(&pool, bob, "bob.example").await;
    let path = "/api/v1/client/apps/wordpress";
    let good = json!({"domain_id": site, "path": "/blog", "title": "Alice's blog",
        "admin_user": "boss", "admin_email": "boss@example.com",
        "admin_password": "a-long-enough-pass-1", "plugins": ["classic-editor"]});
    let (status, made) = e.call(&a, "POST", path, Some(good.clone())).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{made}");
    assert_eq!(
        (
            made["status"].as_str(),
            made["kind"].as_str(),
            made["path"].as_str()
        ),
        (Some("installing"), Some("wordpress"), Some("/blog"))
    );
    // A database and a user of its own, and the two operations that make them.
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM databases WHERE tenant_id = $1")
        .bind(alice)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(names, ["alice_wp1"]);
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM operations WHERE tenant_id = $1 ORDER BY kind")
            .bind(alice)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(kinds, ["create_database", "upsert_db_user"]);
    // Neither password is in the ledger, and the sealed ones are not the text.
    let ledger: String =
        sqlx::query_scalar("SELECT string_agg(payload::text, ' ') FROM operations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!ledger.contains("a-long-enough-pass-1"));
    let sealed: Vec<Vec<u8>> = sqlx::query_scalar("SELECT sealed FROM app_secrets ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(sealed.len(), 2);
    assert!(
        sealed
            .iter()
            .all(|s| !String::from_utf8_lossy(s).contains("a-long-enough-pass-1"))
    );
    // The tracked place is taken; a second site gets the next database name.
    let (status, again) = e.call(&a, "POST", path, Some(good.clone())).await;
    assert_eq!(
        (status, again["code"].as_str()),
        (StatusCode::CONFLICT, Some("place_taken")),
        "{again}"
    );
    let mut second = good.clone();
    second["path"] = json!("/shop");
    let (status, _) = e.call(&a, "POST", path, Some(second)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM databases WHERE tenant_id = $1 ORDER BY name")
            .bind(alice)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(names, ["alice_wp1", "alice_wp2"]);
    // Not valid, and not somebody else's domain.
    for bad in [
        json!({"domain_id": site, "path": "/a b", "title": "t", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1"}),
        json!({"domain_id": site, "path": "/../x", "title": "t", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1"}),
        json!({"domain_id": site, "title": "", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1"}),
        json!({"domain_id": site, "title": "t", "admin_user": "x", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1"}),
        json!({"domain_id": site, "title": "t", "admin_user": "boss", "admin_email": "nope", "admin_password": "a-long-enough-pass-1"}),
        json!({"domain_id": site, "title": "t", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "short"}),
        json!({"domain_id": site, "title": "t", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1", "plugins": ["Bad Slug"]}),
        json!({"domain_id": bobs, "title": "t", "admin_user": "boss", "admin_email": "b@e.com", "admin_password": "a-long-enough-pass-1"}),
    ] {
        let (status, body) = e.call(&a, "POST", path, Some(bad.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} -> {body}");
    }
}
