//! Databases over HTTP, against real PostgreSQL and Redis.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_cache::RedisHandle;
use dhpanel_database::modules as db;
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_databases::{DatabasesState, SETTINGS_SCHEMA, client_router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use dhpanel_settings::{Scope, SettingsRegistry};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const KEY: [u8; 32] = [9u8; 32];

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
    settings: Arc<SettingsRegistry>,
}

struct Browser {
    cookie: String,
    csrf: String,
}

async fn env(pool: &PgPool) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let settings = Arc::new(
        SettingsRegistry::build(vec![(
            "databases".to_owned(),
            serde_json::from_str(SETTINGS_SCHEMA).unwrap(),
        )])
        .unwrap(),
    );
    let manifest = Manifest::parse(
        "databases",
        "[module]\nname = \"databases\"\nversion = \"1.0.0\"\ntitle = { en = \"Databases\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions, settings) = (pool.clone(), sessions.clone(), settings.clone());
            move || {
                let state = DatabasesState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    settings: settings.clone(),
                    master_key: MasterKey::from_bytes(&KEY),
                };
                ModuleRoutes {
                    client: Some(client_router().with_state(state)),
                    ..ModuleRoutes::none()
                }
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
        settings,
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
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
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

/// Plays the node answering the newest operation of a kind about `name`.
async fn node_answers(pool: &PgPool, kind: &str, name: &str, ok: bool) {
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM operations WHERE kind = $1 AND payload ->> 'name' = $2 AND status = 'pending'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(kind)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE operations SET status = 'dispatched', attempts = 1 WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    if ok {
        assert!(
            dhpanel_database::operations::succeed(pool, id, &json!({}))
                .await
                .unwrap()
        );
    } else {
        dhpanel_database::operations::fail(
            pool,
            id,
            &json!({"code": "internal", "message": "no", "retryable": false}),
            None,
        )
        .await
        .unwrap();
    }
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn databases_users_and_grants_are_made_on_the_node_in_the_background(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let plan: Uuid = sqlx::query_scalar("SELECT plan_id FROM tenants WHERE id = $1")
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE plans SET limits = '{\"max_databases\": 2, \"max_db_users\": 2}' WHERE id = $1",
    )
    .bind(plan)
    .execute(&pool)
    .await
    .unwrap();

    // Reading is open; the prefix and the limit are told.
    let (status, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        (list["prefix"].clone(), list["limit"].clone()),
        (json!("alice_"), json!(2))
    );
    // Making needs the feature.
    sqlx::query(
        "UPDATE feature_sets SET all_features = false, features = ARRAY['domains']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "shop"})),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    sqlx::query(
        "UPDATE feature_sets SET features = ARRAY['domains', 'databases', 'db_remote']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // A database: pending, queued for the node, ready when the node says so.
    let (status, shop) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "shop"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{shop}");
    assert_eq!(
        (shop["name"].clone(), shop["status"].clone()),
        (json!("alice_shop"), json!("pending"))
    );
    let queued: Value =
        sqlx::query_scalar("SELECT payload FROM operations WHERE kind = 'create_database'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued["name"], "alice_shop");
    node_answers(&pool, "create_database", "alice_shop", true).await;
    let (_, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    assert_eq!(list["items"][0]["status"], "ready");
    // Typed with the prefix already, the same name is taken.
    let (status, body) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "alice_shop"})),
        )
        .await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "name_taken"));
    // Names that cannot be, are refused with the reason.
    for (name, why) in [
        ("sh op", "invalid_db_name"),
        ("shop;drop", "invalid_db_name"),
        ("mysql", "reserved_db_name"),
        ("", "invalid_db_name"),
        (&"x".repeat(80), "db_name_too_long"),
    ] {
        let (status, body) = e
            .call(
                &a,
                "POST",
                "/api/v1/client/databases",
                Some(json!({"name": name})),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}: {body}");
        assert_eq!(body["errors"][0]["code"], why);
    }
    // The plan's limit.
    let (status, _) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "blog"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    node_answers(&pool, "create_database", "alice_blog", true).await;
    let (status, body) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "third"})),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "limit_reached")
    );
    // Another account sees none of it, and may use the same short name.
    let (_, theirs) = e.call(&b, "GET", "/api/v1/client/databases", None).await;
    assert_eq!(theirs["items"], json!([]));
    let (status, mine_too) = e
        .call(
            &b,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "shop"})),
        )
        .await;
    assert_eq!(
        (status, mine_too["name"].clone()),
        (StatusCode::CREATED, json!("bobby_shop"))
    );

    // A user: weak passwords are refused; without one a strong one is made and shown once.
    for bad in ["short", "alllowercaseonly", "Has\nnewline12A"] {
        let (status, body) = e
            .call(
                &a,
                "POST",
                "/api/v1/client/db-users",
                Some(json!({"name": "web", "password": bad})),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["errors"][0]["field"], "password");
    }
    let (status, made) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/db-users",
            Some(json!({"name": "web"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{made}");
    let password = made["password"].as_str().unwrap().to_owned();
    assert!(dhpanel_domain::databases::check_password(&password).is_ok());
    assert_eq!(made["user"]["name"], "alice_web");
    // The ledger has no password; the sealed one opens to what was shown.
    let ledger: Value =
        sqlx::query_scalar("SELECT payload FROM operations WHERE kind = 'upsert_db_user'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ledger["name"], "alice_web");
    assert!(ledger.get("password").is_none(), "{ledger}");
    let all_ledger: String =
        sqlx::query_scalar("SELECT string_agg(payload::text, ' ') FROM operations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!all_ledger.contains(&password));
    let (user_id, sealed) = dhpanel_database::databases::sealed_password_of(&pool, "alice_web")
        .await
        .unwrap()
        .unwrap();
    let opened = MasterKey::from_bytes(&KEY)
        .open(
            &dhpanel_database::databases::password_context(user_id),
            &sealed,
        )
        .unwrap();
    assert_eq!(opened, password.as_bytes());
    // Sealed to this user only.
    assert!(
        MasterKey::from_bytes(&KEY)
            .open(
                &dhpanel_database::databases::password_context(Uuid::now_v7()),
                &sealed
            )
            .is_err()
    );
    let audit: String =
        sqlx::query_scalar("SELECT string_agg(metadata::text, ' ') FROM audit_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !audit.contains(&password),
        "the audit log never holds a password"
    );
    // Not ready yet: no grant, no password change.
    let uid = made["user"]["id"].as_str().unwrap().to_owned();
    let shop_id = shop["id"].as_str().unwrap().to_owned();
    let (status, body) = e
        .call(
            &a,
            "PUT",
            &format!("/api/v1/client/databases/{shop_id}/grants/{uid}"),
            Some(json!({"access": "all"})),
        )
        .await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "not_ready"));
    // The node has it: ready, and the sealed password is gone.
    node_answers(&pool, "upsert_db_user", "alice_web", true).await;
    assert!(
        dhpanel_database::databases::sealed_password_of(&pool, "alice_web")
            .await
            .unwrap()
            .is_none()
    );
    let (_, users) = e.call(&a, "GET", "/api/v1/client/db-users", None).await;
    assert_eq!(
        (users["items"][0]["status"].clone(), users["limit"].clone()),
        (json!("ready"), json!(2))
    );

    // Grants: an unknown level is refused; a good one is queued and shown on both sides.
    let grant = |db: &str, user: &str| format!("/api/v1/client/databases/{db}/grants/{user}");
    let (status, body) = e
        .call(
            &a,
            "PUT",
            &grant(&shop_id, &uid),
            Some(json!({"access": "root"})),
        )
        .await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("unknown_access"))
    );
    let (status, view) = e
        .call(
            &a,
            "PUT",
            &grant(&shop_id, &uid),
            Some(json!({"access": "read_write"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["users"][0]["access"], "read_write");
    let queued: Value =
        sqlx::query_scalar("SELECT payload FROM operations WHERE kind = 'grant_db'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        queued,
        json!({"type": "grant_db", "database": "alice_shop", "user": "alice_web", "access": "read_write"})
    );
    let (_, users) = e.call(&a, "GET", "/api/v1/client/db-users", None).await;
    assert_eq!(users["items"][0]["databases"][0]["database"], "alice_shop");
    // Another account's user or database is not found.
    let (_, bobs_db) = e.call(&b, "GET", "/api/v1/client/databases", None).await;
    let bob_db = bobs_db["items"][0]["id"].as_str().unwrap().to_owned();
    let (status, _) = e
        .call(
            &a,
            "PUT",
            &grant(&bob_db, &uid),
            Some(json!({"access": "all"})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = e
        .call(
            &b,
            "PUT",
            &grant(&bob_db, &uid),
            Some(json!({"access": "all"})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        e.call(
            &b,
            "DELETE",
            &format!("/api/v1/client/db-users/{uid}"),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // A new password goes the same way.
    let (status, changed) = e
        .call(
            &a,
            "PUT",
            &format!("/api/v1/client/db-users/{uid}/password"),
            Some(json!({"password": "Another-Passw0rd"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert!(
        changed["password"].is_null(),
        "a password the person gave is not shown back"
    );
    assert!(
        dhpanel_database::databases::sealed_password_of(&pool, "alice_web")
            .await
            .unwrap()
            .is_some()
    );

    // Remote access: the feature, the shape of the addresses, then saved and queued.
    let remote = format!("/api/v1/client/db-users/{uid}/remote-hosts");
    for bad in ["0.0.0.0/0", "%", "203.0.113.7", "10.0.0.0/7"] {
        let (status, body) = e
            .call(&a, "PUT", &remote, Some(json!({"cidrs": [bad]})))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert_eq!(body["errors"][0]["code"], "invalid_cidr");
    }
    let many: Vec<String> = (0..21).map(|i| format!("203.0.113.{i}/32")).collect();
    assert_eq!(
        e.call(&a, "PUT", &remote, Some(json!({"cidrs": many})))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, saved) = e
        .call(
            &a,
            "PUT",
            &remote,
            Some(json!({"cidrs": ["203.0.113.7/32", "198.51.100.0/24", "203.0.113.7/32"]})),
        )
        .await;
    assert_eq!(
        (status, saved["cidrs"].clone()),
        (StatusCode::OK, json!(["203.0.113.7/32", "198.51.100.0/24"]))
    );
    let (_, got) = e.call(&a, "GET", &remote, None).await;
    assert_eq!(got["cidrs"].as_array().unwrap().len(), 2);
    let queued: Value =
        sqlx::query_scalar("SELECT payload FROM operations WHERE kind = 'set_db_remote_hosts'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued["username"], "alice");
    assert_eq!(queued["name"], "alice_web");
    // The node is told which addresses its packet filter lets through: all of them.
    let access: Value = sqlx::query_scalar(
        "SELECT payload FROM operations WHERE kind = 'set_db_remote_access' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        access["cidrs"],
        json!(["198.51.100.0/24", "203.0.113.7/32"])
    );

    // Removing a grant, a user and a database.
    assert_eq!(
        e.call(&a, "DELETE", &grant(&shop_id, &uid), None).await.0,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        e.call(&a, "DELETE", &grant(&shop_id, &uid), None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            &a,
            "DELETE",
            &format!("/api/v1/client/db-users/{uid}"),
            None
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    node_answers(&pool, "drop_db_user", "alice_web", true).await;
    let (_, users) = e.call(&a, "GET", "/api/v1/client/db-users", None).await;
    assert_eq!(users["items"], json!([]));
    assert_eq!(
        e.call(
            &a,
            "DELETE",
            &format!("/api/v1/client/databases/{shop_id}"),
            None
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let (_, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["status"] == "deleting")
    );
    node_answers(&pool, "drop_database", "alice_shop", true).await;
    let (_, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    assert_eq!(
        list["items"].as_array().unwrap().len(),
        1,
        "only alice_blog is left"
    );

    // A database the node could not make is shown as failed (and can be removed).
    let (status, _) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "again"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    node_answers(&pool, "create_database", "alice_again", false).await;
    let (_, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    let failed = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "alice_again")
        .unwrap();
    assert_eq!(failed["status"], "failed");

    // A suspended account changes nothing.
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "late"})),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "tenant_not_running")
    );
    let _ = (&e.settings, Scope::Global);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_account_prefix_can_be_switched_off_and_names_stay_unique(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    sqlx::query(
        "UPDATE feature_sets SET all_features = false, features = ARRAY['databases']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    dhpanel_settings::set(
        &e.settings,
        &mut tx,
        "databases",
        Scope::Global,
        json!({"require_prefix": false}).as_object().unwrap(),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (_, list) = e.call(&a, "GET", "/api/v1/client/databases", None).await;
    assert_eq!(list["prefix"], "");
    let (status, made) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "shared_name"})),
        )
        .await;
    assert_eq!(
        (status, made["name"].clone()),
        (StatusCode::CREATED, json!("shared_name"))
    );
    // Unique across the panel; the server's own names are still closed.
    let (status, body) = e
        .call(
            &b,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "shared_name"})),
        )
        .await;
    assert_eq!((status, code(&body)), (StatusCode::CONFLICT, "name_taken"));
    let (status, _) = e
        .call(
            &b,
            "POST",
            "/api/v1/client/databases",
            Some(json!({"name": "mysql"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
