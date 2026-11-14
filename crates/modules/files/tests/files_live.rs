//! The file manager over HTTP against real PostgreSQL and Redis, with the node
//! played by the real file operations on a temporary directory.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::modules as db;
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_files::{FilesState, SETTINGS_SCHEMA, client_router};
use dhpanel_node_protocol::{Operation, Output};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use dhpanel_settings::{Scope, SettingsRegistry};
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
    settings: Arc<SettingsRegistry>,
}

struct Browser {
    cookie: String,
    csrf: String,
}

async fn env(pool: &PgPool, wait: Duration) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let settings = Arc::new(
        SettingsRegistry::build(vec![(
            "files".to_owned(),
            serde_json::from_str(SETTINGS_SCHEMA).unwrap(),
        )])
        .unwrap(),
    );
    let manifest = Manifest::parse(
        "files",
        "[module]\nname = \"files\"\nversion = \"1.0.0\"\ntitle = { en = \"Files\" }\ncategory = \"core\"\ncore = true\n",
    )
    .unwrap();
    let module = Arc::new(Fake {
        manifest,
        routes: Box::new({
            let (pool, sessions, settings) = (pool.clone(), sessions.clone(), settings.clone());
            move || ModuleRoutes {
                client: Some(client_router().with_state(FilesState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    settings: settings.clone(),
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
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn set(&self, pool: &PgPool, values: Value) {
        let mut tx = pool.begin().await.unwrap();
        dhpanel_settings::set(
            &self.settings,
            &mut tx,
            "files",
            Scope::Global,
            values.as_object().unwrap(),
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
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

/// Plays a node: each tenant has a directory under `base`, and every `file_op` is done by
/// the real operations on it.
fn node(pool: PgPool, base: PathBuf, delay: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let rows: Vec<(Uuid, Value)> = sqlx::query_as(
                "SELECT id, payload FROM operations WHERE kind = 'file_op' AND status = 'pending'",
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
                let Operation::FileOp {
                    tenant_id, request, ..
                } = serde_json::from_value(payload).unwrap()
                else {
                    panic!("not a file op")
                };
                tokio::time::sleep(delay).await;
                let dir = base.join(tenant_id.to_string());
                std::fs::create_dir_all(&dir).unwrap();
                let root = dhpanel_safe_fs::Root::open(&dir).unwrap();
                let result = dhpanel_fileops::handle(&root, request);
                let output = serde_json::to_value(Output::File { result }).unwrap();
                dhpanel_database::operations::succeed(&pool, id, &output)
                    .await
                    .unwrap();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dh-files-api-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_file_manager_works_on_the_account_own_files_through_the_node(pool: PgPool) {
    let base = scratch("main");
    let e = env(&pool, Duration::from_secs(20)).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let node = node(pool.clone(), base.clone(), Duration::ZERO);
    let f = "/api/v1/client/files";

    // Write, read, edit with the conflict check, and the editor's size limit.
    let (status, got) = e
        .call(
            &a,
            "PUT",
            &format!("{f}/content"),
            Some(json!({"path": "index.php", "content": "<?php echo 1;"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    let (status, got) = e
        .call(&a, "GET", &format!("{f}/content?path=index.php"), None)
        .await;
    assert_eq!(
        (status, got["content"].as_str(), got["binary"].as_bool()),
        (StatusCode::OK, Some("<?php echo 1;"), Some(false))
    );
    let mtime = got["mtime"].as_i64().unwrap();
    let (status, got) = e
        .call(
            &a,
            "PUT",
            &format!("{f}/content"),
            Some(json!({"path": "index.php", "content": "x", "expected_mtime": mtime - 50})),
        )
        .await;
    assert_eq!(
        (status, code(&got)),
        (StatusCode::CONFLICT, "changed_meanwhile")
    );
    let (status, _) = e
        .call(
            &a,
            "PUT",
            &format!("{f}/content"),
            Some(json!({"path": "index.php", "content": "<?php echo 2;", "expected_mtime": mtime})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, got) = e
        .call(
            &a,
            "PUT",
            &format!("{f}/content"),
            Some(json!({"path": "index.php", "content": "n", "create_only": true})),
        )
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "exists"));
    e.set(&pool, json!({"edit_size_kb": 1})).await;
    let (status, got) = e
        .call(
            &a,
            "PUT",
            &format!("{f}/content"),
            Some(json!({"path": "big.txt", "content": "x".repeat(1025)})),
        )
        .await;
    assert_eq!(
        (status, code(&got)),
        (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
        "{got}"
    );
    std::fs::write(
        base.join(
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenants WHERE username = 'alice'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .to_string(),
        )
        .join("big.bin"),
        vec![b'a'; 4000],
    )
    .unwrap();
    let (status, got) = e
        .call(&a, "GET", &format!("{f}/content?path=big.bin"), None)
        .await;
    assert_eq!(
        (status, code(&got)),
        (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
        "{got}"
    );

    // Folders, listing and paging.
    for n in ["a", "b", "c"] {
        let (status, _) = e
            .call(
                &a,
                "POST",
                &format!("{f}/folders"),
                Some(json!({"path": n})),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, got) = e
        .call(&a, "GET", &format!("{f}?path=&limit=10"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{got}");
    let names: Vec<&str> = got["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["a", "b", "c", "big.bin", "index.php"],
        "folders first"
    );
    assert_eq!(
        (
            got["total"].clone(),
            got["offset"].clone(),
            got["limit"].clone()
        ),
        (json!(5), json!(0), json!(10))
    );
    e.set(&pool, json!({"page_size": 10})).await;
    let (_, got) = e
        .call(&a, "GET", &format!("{f}?path=&limit=500"), None)
        .await;
    assert_eq!(got["limit"], 10, "the page size setting is the most");
    let (_, got) = e
        .call(&a, "GET", &format!("{f}?path=&offset=3&limit=10"), None)
        .await;
    assert_eq!(got["items"].as_array().unwrap().len(), 2);

    // Nobody else sees them, nobody leaves the folder.
    let (_, got) = e.call(&b, "GET", &format!("{f}?path="), None).await;
    assert_eq!(got["total"], 0);
    for path in ["../bobby-files", "/etc", "a/../../x", ".trash"] {
        let (status, got) = e
            .call(
                &a,
                "GET",
                &format!("{f}?path={}", path.replace('/', "%2F")),
                None,
            )
            .await;
        assert_eq!(
            (status, code(&got)),
            (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
            "{path}: {got}"
        );
    }
    let (status, _) = e
        .call(&a, "GET", &format!("{f}/stat?path=nothing"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Move, copy (never over something), delete to the trash, restore.
    let (status, _) = e
        .call(
            &a,
            "POST",
            &format!("{f}/move"),
            Some(json!({"from": "index.php", "to": "a/index.php"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/copy"),
            Some(json!({"from": "a", "to": "b"})),
        )
        .await;
    assert_eq!((status, code(&got)), (StatusCode::CONFLICT, "exists"));
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/copy"),
            Some(json!({"from": "a", "to": "c/a-copy"})),
        )
        .await;
    assert_eq!((status, got["count"].clone()), (StatusCode::OK, json!(2)));
    let (status, _) = e.call(&a, "DELETE", &format!("{f}?path=a"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, trash) = e.call(&a, "GET", &format!("{f}/trash"), None).await;
    assert_eq!(trash["items"][0]["path"], "a");
    let id = trash["items"][0]["id"].as_str().unwrap().to_owned();
    let (status, _) = e
        .call(&a, "POST", &format!("{f}/trash/{id}/restore"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, got) = e
        .call(&a, "GET", &format!("{f}/stat?path=a%2Findex.php"), None)
        .await;
    assert_eq!(got["kind"], "file");
    let (status, got) = e.call(&a, "DELETE", &format!("{f}/trash"), None).await;
    assert_eq!((status, got["count"].clone()), (StatusCode::OK, json!(0)));

    // Permissions: setuid is refused.
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/permissions"),
            Some(json!({"path": "a/index.php", "mode": 0o4755})),
        )
        .await;
    assert_eq!(
        (status, code(&got)),
        (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
        "{got}"
    );
    assert_eq!(got["errors"][0]["code"], "invalid_mode");
    let (status, _) = e
        .call(
            &a,
            "POST",
            &format!("{f}/permissions"),
            Some(json!({"path": "a/index.php", "mode": 0o640})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Packing and unpacking; a hostile archive is refused.
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/archive"),
            Some(json!({"paths": ["a"], "dest": "a.zip", "format": "zip"})),
        )
        .await;
    assert_eq!(
        (status, got["count"].clone()),
        (StatusCode::OK, json!(2)),
        "{got}"
    );
    let (status, _) = e
        .call(
            &a,
            "POST",
            &format!("{f}/archive"),
            Some(json!({"paths": ["a"], "dest": "a2.zip", "format": "rar"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/extract"),
            Some(json!({"path": "a.zip", "dest": "unpacked"})),
        )
        .await;
    assert_eq!(
        (status, got["count"].clone()),
        (StatusCode::OK, json!(2)),
        "{got}"
    );
    let tenant_dir = base.join(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM tenants WHERE username = 'alice'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .to_string(),
    );
    std::fs::write(tenant_dir.join("evil.zip"), {
        let mut zip = zip_with_name("../../etc/passwd");
        zip.shrink_to_fit();
        zip
    })
    .unwrap();
    let (status, got) = e
        .call(
            &a,
            "POST",
            &format!("{f}/extract"),
            Some(json!({"path": "evil.zip", "dest": "evil"})),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{got}");
    assert_eq!(got["errors"][0]["code"], "bad_archive");

    // What was changed is in the audit log; reading and listing are not.
    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action FROM audit_logs WHERE action LIKE 'files.%' ORDER BY at")
            .fetch_all(&pool)
            .await
            .unwrap();
    for wanted in [
        "files.written",
        "files.folder_made",
        "files.moved",
        "files.copied",
        "files.deleted",
        "files.restored",
        "files.permissions_changed",
        "files.archived",
        "files.extracted",
    ] {
        assert!(
            actions.iter().any(|a| a == wanted),
            "{wanted} in {actions:?}"
        );
    }
    assert!(
        !actions
            .iter()
            .any(|a| a.contains("read") || a.contains("list"))
    );
    node.abort();
    let _ = std::fs::remove_dir_all(&base);
}

fn zip_with_name(name: &str) -> Vec<u8> {
    use std::io::Write as _;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"pwn").unwrap();
    zip.finish().unwrap().into_inner()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn slow_work_answers_202_and_the_answer_is_asked_for_later_by_its_owner_only(pool: PgPool) {
    let base = scratch("slow");
    let e = env(&pool, Duration::from_millis(200)).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let node = node(pool.clone(), base.clone(), Duration::from_millis(900));
    let (status, got) = e
        .call(
            &a,
            "POST",
            "/api/v1/client/files/folders",
            Some(json!({"path": "slow"})),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{got}");
    let id = got["operation_id"].as_str().unwrap().to_owned();
    let url = format!("/api/v1/client/files/operations/{id}");
    let (status, _) = e.call(&b, "GET", &url, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "someone else's operation");
    let mut last = (StatusCode::ACCEPTED, Value::Null);
    for _ in 0..40 {
        last = e.call(&a, "GET", &url, None).await;
        if last.0 != StatusCode::ACCEPTED {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        (last.0, last.1["count"].clone()),
        (StatusCode::OK, json!(1)),
        "{:?}",
        last.1
    );
    // A plan without the feature has no file manager.
    sqlx::query(
        "UPDATE feature_sets SET all_features = false, features = ARRAY['domains']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, got) = e.call(&a, "GET", "/api/v1/client/files?path=", None).await;
    assert_eq!(
        (status, code(&got)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    node.abort();
    let _ = std::fs::remove_dir_all(&base);
}
