//! Backup destinations over HTTP, against real PostgreSQL and Redis.
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
use dhpanel_mod_backups::{BackupsState, admin_router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use dhpanel_settings::SettingsRegistry;
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
}

struct Browser {
    cookie: String,
    csrf: String,
}

async fn env(pool: &PgPool) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let settings = Arc::new(SettingsRegistry::build(vec![]).unwrap());
    let manifest = Manifest::parse(
        "backups",
        "[module]\nname = \"backups\"\nversion = \"1.0.0\"\ntitle = { en = \"Backups\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions) = (pool.clone(), sessions.clone());
            move || ModuleRoutes {
                admin: Some(admin_router().with_state(BackupsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    settings: settings.clone(),
                    master_key: MasterKey::from_bytes(&KEY),
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
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

async fn admin(pool: &PgPool, name: &str, roles: &[&str]) -> Uuid {
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
        dhpanel_database::rbac::assign_role(pool, id, role, None)
            .await
            .unwrap();
    }
    id
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn destinations_keep_their_secrets_sealed_and_never_show_them(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(admin(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let nobody = e.sign_in(admin(&pool, "plain", &[]).await).await;
    let path = "/api/v1/admin/backup-destinations";
    let (status, _) = e.call(&nobody, "GET", path, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A folder on the node: nothing secret; the first one is the default.
    let (status, local) = e.call(&root, "POST", path, Some(json!({"name": "Disk", "kind": "local", "settings": {"path": "/var/backups/dhpanel"}}))).await;
    assert_eq!(status, StatusCode::CREATED, "{local}");
    assert_eq!(
        (
            local["is_default"].clone(),
            local["has_credentials"].clone()
        ),
        (json!(true), json!(false))
    );
    // S3: the keys are sealed at once.
    let secret = "super-secret-key-1234567890";
    let (status, s3) = e
        .call(&root, "POST", path, Some(json!({
            "name": "Offsite",
            "kind": "s3",
            "settings": {"endpoint": "https://s3.example.com", "bucket": "dh-backups", "prefix": "/panel/", "ignored": "x"},
            "credentials": {"access_key": "AKIAEXAMPLE", "secret_key": secret}
        })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{s3}");
    assert_eq!(
        (s3["has_credentials"].clone(), s3["is_default"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        s3["settings"],
        json!({"endpoint": "https://s3.example.com", "bucket": "dh-backups", "region": "us-east-1", "prefix": "panel", "path_style": true})
    );
    let id: Uuid = s3["id"].as_str().unwrap().parse().unwrap();
    // Nowhere in the database or the answers is the secret in plain.
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT sealed_credentials FROM backup_destinations WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!stored.windows(secret.len()).any(|w| w == secret.as_bytes()));
    let everything: String =
        sqlx::query_scalar("SELECT d::text FROM backup_destinations d WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!everything.contains(secret) && !everything.contains("AKIAEXAMPLE"));
    let opened = MasterKey::from_bytes(&KEY)
        .open(&dhpanel_database::backups::credentials_context(id), &stored)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&opened).unwrap()["secret_key"],
        secret
    );
    let (_, list) = e.call(&root, "GET", path, None).await;
    assert!(!list.to_string().contains(secret) && !list.to_string().contains("AKIA"));
    // Sealed for that destination only.
    assert!(
        MasterKey::from_bytes(&KEY)
            .open(
                &dhpanel_database::backups::credentials_context(Uuid::new_v4()),
                &stored
            )
            .is_err()
    );

    // What cannot be.
    for (body, why) in [
        (
            json!({"name": "x", "kind": "ftp", "settings": {}}),
            "invalid_kind",
        ),
        (
            json!({"name": "x", "kind": "local", "settings": {"path": "relative"}}),
            "invalid_path",
        ),
        (
            json!({"name": "x", "kind": "local", "settings": {"path": "/a/../b"}}),
            "invalid_path",
        ),
        (
            json!({"name": "x", "kind": "s3", "settings": {"endpoint": "http://evil.example.com", "bucket": "abc"}, "credentials": {"access_key": "a", "secret_key": "b"}}),
            "invalid_endpoint",
        ),
        (
            json!({"name": "x", "kind": "s3", "settings": {"endpoint": "https://s3.example.com", "bucket": "AB"}, "credentials": {"access_key": "a", "secret_key": "b"}}),
            "invalid_bucket",
        ),
        (
            json!({"name": "x", "kind": "s3", "settings": {"endpoint": "https://s3.example.com", "bucket": "abc"}}),
            "required",
        ),
        (
            json!({"name": "x", "kind": "sftp", "settings": {"host": "h", "user": "u"}, "credentials": {"password": "p", "private_key": "-----BEGIN PRIVATE KEY-----"}}),
            "invalid_credentials",
        ),
        (
            json!({"name": "x", "kind": "sftp", "settings": {"host": "h", "user": "u", "host_key_sha256": "zz"}, "credentials": {"password": "p"}}),
            "invalid_fingerprint",
        ),
        (
            json!({"name": "", "kind": "local", "settings": {"path": "/x"}}),
            "invalid_name",
        ),
    ] {
        let (status, got) = e.call(&root, "POST", path, Some(body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{why}: {got}");
        assert!(got.to_string().contains(why), "{why}: {got}");
    }
    let (status, got) = e
        .call(
            &root,
            "POST",
            path,
            Some(json!({"name": "Disk", "kind": "local", "settings": {"path": "/x"}})),
        )
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "name_taken"));

    // Changing: new keys replace the sealed ones; the default is moved, not switched off.
    let (status, got) = e.call(&root, "PUT", &format!("{path}/{id}"), Some(json!({"credentials": {"access_key": "NEWKEY", "secret_key": "new-secret-9876543210"}, "enabled": true}))).await;
    assert_eq!(status, StatusCode::OK, "{got}");
    let stored2: Vec<u8> =
        sqlx::query_scalar("SELECT sealed_credentials FROM backup_destinations WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(stored, stored2);
    let local_id = local["id"].as_str().unwrap();
    let (status, got) = e
        .call(
            &root,
            "PUT",
            &format!("{path}/{local_id}"),
            Some(json!({"enabled": false})),
        )
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "is_default"));
    let (status, got) = e
        .call(&root, "DELETE", &format!("{path}/{local_id}"), None)
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "is_default"));
    let (status, _) = e
        .call(&root, "POST", &format!("{path}/{id}/default"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    // A destination backups point at stays.
    let tenant_free: Uuid = sqlx::query_scalar(
        "INSERT INTO backups (kind, trigger, destination_id, status) VALUES ('system', 'manual', $1, 'ready') RETURNING id",
    )
    .bind(local_id.parse::<Uuid>().unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    let (status, got) = e
        .call(&root, "DELETE", &format!("{path}/{local_id}"), None)
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "in_use"));
    sqlx::query("DELETE FROM backups WHERE id = $1")
        .bind(tenant_free)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = e
        .call(&root, "DELETE", &format!("{path}/{local_id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE action LIKE 'backup_destination.%'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        actions.iter().any(|a| a == "backup_destination.added")
            && actions.iter().any(|a| a == "backup_destination.removed")
    );
    assert!(!actions.is_empty());
}
