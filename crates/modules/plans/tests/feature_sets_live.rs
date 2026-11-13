//! Feature sets over HTTP, against real PostgreSQL and Redis.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::{modules as db, rbac::assign_role};
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_plans::{PlansState, router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const SETS: &str = "/api/v1/admin/feature-sets";

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

fn manifest(name: &str, category: &str, core: bool, features: &[&str]) -> Manifest {
    let keys = features
        .iter()
        .map(|f| format!("\"{f}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Manifest::parse(
        name,
        &format!(
            "[module]\nname = \"{name}\"\nversion = \"1.0.0\"\ntitle = {{ en = \"{name} title\" }}\ncategory = \"{category}\"\ncore = {core}\n[features]\nkeys = [{keys}]\n"
        ),
    )
    .unwrap()
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
    let late: Arc<OnceLock<ModuleMount>> = Arc::default();
    let plans = Arc::new(Fake {
        manifest: manifest("plans", "core", true, &[]),
        routes: Box::new({
            let (pool, sessions, late) = (pool.clone(), sessions.clone(), late.clone());
            move || match late.get() {
                Some(mount) => ModuleRoutes {
                    admin: Some(router().with_state(PlansState {
                        db: pool.clone(),
                        sessions: sessions.clone(),
                        mount: mount.clone(),
                    })),
                    ..ModuleRoutes::none()
                },
                None => ModuleRoutes::none(),
            }
        }),
    });
    let plain = |m: Manifest| -> Arc<dyn Module> {
        Arc::new(Fake {
            manifest: m,
            routes: Box::new(ModuleRoutes::none),
        })
    };
    let registry = Registry::build(vec![
        plans,
        plain(manifest("domains", "web", false, &["domains", "redirects"])),
        plain(manifest("db-mysql", "db", false, &["databases"])),
    ])
    .unwrap();
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
    late.set(mount.clone()).ok();
    mount.rebuild();
    mount.apply(&db::enabled_names(pool).await.unwrap().into_iter().collect());
    Env {
        app: mount.service(),
        sessions,
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

    async fn call(
        &self,
        who: Option<&Browser>,
        method: &str,
        path: &str,
        body: Option<Value>,
        csrf: bool,
    ) -> (StatusCode, Value) {
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
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn id_of(set: &Value) -> String {
    set["id"].as_str().unwrap().to_owned()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_feature_catalogue_and_the_presets_are_listed(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;

    assert_eq!(
        e.call(None, "GET", SETS, None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = e.call(Some(&plain), "GET", SETS, None, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );

    // The catalogue: what the installed modules declare, by category.
    let (status, body) = e
        .call(Some(&root), "GET", "/api/v1/admin/features", None, false)
        .await;
    assert_eq!(status, StatusCode::OK);
    let items: Vec<(String, String, String)> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["category"].as_str().unwrap().into(),
                f["module"].as_str().unwrap().into(),
                f["key"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            (
                "db".to_owned(),
                "db-mysql".to_owned(),
                "databases".to_owned()
            ),
            ("web".to_owned(), "domains".to_owned(), "domains".to_owned()),
            (
                "web".to_owned(),
                "domains".to_owned(),
                "redirects".to_owned()
            ),
        ]
    );
    assert_eq!(body["items"][0]["module_title"]["en"], "db-mysql title");

    // The four presets, with how many plans use each.
    let (status, body) = e.call(Some(&root), "GET", SETS, None, false).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["basic", "db_only", "default", "dns_only"]);
    let default = &body["items"][2];
    assert_eq!(
        (
            default["all_features"].as_bool(),
            default["built_in"].as_bool(),
            default["plans"].as_i64()
        ),
        (Some(true), Some(true), Some(1))
    );
    // Preset keys no installed module declares yet are reported, not hidden.
    let db_only = &body["items"][1];
    assert_eq!(db_only["features"], json!(["databases"]));
    assert_eq!(
        db_only["unknown_features"],
        json!([]),
        "this fixture declares `databases`"
    );
    assert_eq!(body["items"][3]["unknown_features"], json!(["dns"]));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_feature_set_can_be_created_edited_and_deleted_with_an_audit_trail(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;

    let (status, set) = e
        .call(
            Some(&root),
            "POST",
            SETS,
            Some(json!({"name": "web_only", "description": "Sites", "features": ["redirects", "domains", "domains", "mystery"]})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{set}");
    assert_eq!(
        set["features"],
        json!(["domains", "mystery", "redirects"]),
        "sorted, no duplicates"
    );
    assert_eq!(set["unknown_features"], json!(["mystery"]));
    assert_eq!(
        (set["built_in"].clone(), set["plans"].clone()),
        (json!(false), json!(0))
    );
    let id = id_of(&set);

    let (status, same) = e
        .call(Some(&root), "GET", &format!("{SETS}/{id}"), None, false)
        .await;
    assert_eq!(
        (status, &same["name"]),
        (StatusCode::OK, &json!("web_only"))
    );

    // Edit: rename, swap the list. Then switch to "all features".
    let (status, set) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{SETS}/{id}"),
            Some(json!({"name": "sites", "features": ["domains"]})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{set}");
    assert_eq!(
        (set["name"].clone(), set["features"].clone()),
        (json!("sites"), json!(["domains"]))
    );
    let (_, set) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{SETS}/{id}"),
            Some(json!({"name": "sites", "all_features": true})),
            true,
        )
        .await;
    assert_eq!(
        (set["all_features"].clone(), set["features"].clone()),
        (json!(true), json!([]))
    );

    // A built-in set can be edited but keeps its name.
    let (_, list) = e.call(Some(&root), "GET", SETS, None, false).await;
    let dns = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "dns_only")
        .unwrap();
    let (status, edited) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{SETS}/{}", id_of(dns)),
            Some(json!({"name": "renamed", "features": ["dns", "ssl"]})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (edited["name"].clone(), edited["features"].clone()),
        (json!("dns_only"), json!(["dns", "ssl"]))
    );

    // Delete.
    let (status, _) = e
        .call(Some(&root), "DELETE", &format!("{SETS}/{id}"), None, true)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        e.call(Some(&root), "GET", &format!("{SETS}/{id}"), None, false)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Some(&root), "DELETE", &format!("{SETS}/{id}"), None, true)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    let audit: Vec<(String, Value)> =
        sqlx::query_as("SELECT action, metadata FROM audit_logs ORDER BY at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let actions: Vec<&str> = audit.iter().map(|a| a.0.as_str()).collect();
    assert_eq!(
        actions,
        [
            "admin.feature_set_created",
            "admin.feature_set_updated",
            "admin.feature_set_updated",
            "admin.feature_set_updated",
            "admin.feature_set_deleted"
        ]
    );
    assert_eq!(
        audit[1].1["before"]["features"],
        json!(["domains", "mystery", "redirects"])
    );
    assert_eq!(audit[1].1["after"]["features"], json!(["domains"]));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn bad_input_names_in_use_and_built_in_sets_are_refused(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;
    let ok = json!({"name": "fine", "features": ["dns"]});

    // Who may write.
    let (status, body) = e
        .call(Some(&plain), "POST", SETS, Some(ok.clone()), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );
    assert_eq!(
        e.call(Some(&root), "POST", SETS, Some(ok.clone()), false)
            .await
            .0,
        StatusCode::FORBIDDEN,
        "no CSRF header"
    );
    assert_eq!(
        e.call(None, "POST", SETS, Some(ok.clone()), true).await.0,
        StatusCode::UNAUTHORIZED
    );

    // Invalid fields: nothing is created.
    for bad in [
        json!({"name": "Bad Name"}),
        json!({"name": "ok_name", "features": ["Bad Key"]}),
        json!({"name": "ok_name", "all_features": true, "features": ["dns"]}),
        json!({"name": "ok_name", "description": "x".repeat(256)}),
    ] {
        let (status, body) = e
            .call(Some(&root), "POST", SETS, Some(bad.clone()), true)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
    }
    let (status, _) = e
        .call(
            Some(&root),
            "POST",
            SETS,
            Some(json!({"name": "ok_name", "force": true})),
            true,
        )
        .await;
    assert!(status.is_client_error(), "unknown fields are refused");

    // Names are unique, on create and on rename.
    let (status, body) = e
        .call(
            Some(&root),
            "POST",
            SETS,
            Some(json!({"name": "basic"})),
            true,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "feature_set_name_taken")
    );
    let (_, mine) = e.call(Some(&root), "POST", SETS, Some(ok), true).await;
    let (status, body) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{SETS}/{}", id_of(&mine)),
            Some(json!({"name": "db_only"})),
            true,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "feature_set_name_taken")
    );

    // Built-in sets cannot be deleted; sets that plans use cannot either.
    let (_, list) = e.call(Some(&root), "GET", SETS, None, false).await;
    let basic = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "basic")
        .unwrap();
    let (status, body) = e
        .call(
            Some(&root),
            "DELETE",
            &format!("{SETS}/{}", id_of(basic)),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "feature_set_built_in")
    );
    sqlx::query("INSERT INTO plans (name, feature_set_id) VALUES ('uses_mine', $1::uuid)")
        .bind(id_of(&mine))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = e
        .call(
            Some(&root),
            "DELETE",
            &format!("{SETS}/{}", id_of(&mine)),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "feature_set_in_use")
    );
    assert_eq!(
        e.call(
            Some(&root),
            "GET",
            &format!("{SETS}/{}", id_of(&mine)),
            None,
            false
        )
        .await
        .0,
        StatusCode::OK
    );

    // Not ids.
    assert_eq!(
        e.call(
            Some(&root),
            "GET",
            &format!("{SETS}/not-a-uuid"),
            None,
            false
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            Some(&root),
            "PUT",
            &format!("{SETS}/{}", Uuid::now_v7()),
            Some(json!({"name": "valid_name"})),
            true
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
