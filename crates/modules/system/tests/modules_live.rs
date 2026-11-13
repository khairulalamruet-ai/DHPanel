//! Switching modules over HTTP, against real PostgreSQL and Redis, with the
//! routes really appearing and disappearing.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, OnceLock};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::{modules as db, rbac::assign_role};
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_system::{SystemState, router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const BASE: &str = "/api/v1/admin/modules";

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

fn manifest(name: &str, core: bool, deps: &[&str]) -> Manifest {
    let deps = deps
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let category = if core { "core" } else { "web" };
    Manifest::parse(
        name,
        &format!(
            "[module]\nname = \"{name}\"\nversion = \"1.0.0\"\ntitle = {{ en = \"{name} title\" }}\ncategory = \"{category}\"\ncore = {core}\ndepends_on = [{deps}]\n"
        ),
    )
    .unwrap()
}

/// A module with one route: `/api/v1/client/<name>` answers its own name.
fn feature(name: &'static str, deps: &[&str]) -> Arc<dyn Module> {
    Arc::new(Fake {
        manifest: manifest(name, false, deps),
        routes: Box::new(move || ModuleRoutes {
            client: Some(Router::new().route(
                &format!("/api/v1/client/{name}"),
                get(move || async move { name }),
            )),
            ..ModuleRoutes::none()
        }),
    })
}

struct Env {
    app: Router,
    sessions: SessionStore,
    pool: PgPool,
    mount: ModuleMount,
}

struct Browser {
    cookie: String,
    csrf: String,
}

/// auth (core), system (core, the real routes), dns, domains -> dns, ssl -> domains.
async fn env(pool: &PgPool) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis, pool.clone(), SessionConfig::default());
    let late: Arc<OnceLock<ModuleMount>> = Arc::default();
    let system = Arc::new(Fake {
        manifest: manifest("system", true, &[]),
        routes: Box::new({
            let (pool, sessions, late) = (pool.clone(), sessions.clone(), late.clone());
            move || match late.get() {
                Some(mount) => ModuleRoutes {
                    admin: Some(router().with_state(SystemState {
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
    let registry = Registry::build(vec![
        system,
        feature("dns", &[]),
        feature("domains", &["dns"]),
        feature("ssl", &["domains"]),
    ])
    .unwrap();
    // What control-api does at startup: write the build's modules to the table.
    let values: Vec<(String, Value)> = registry
        .modules()
        .iter()
        .map(|m| {
            (
                m.manifest().module.name.clone(),
                serde_json::to_value(m.manifest()).unwrap(),
            )
        })
        .collect();
    let records: Vec<db::ModuleRecord<'_>> = registry
        .modules()
        .iter()
        .zip(&values)
        .map(|(m, (_, json))| db::ModuleRecord {
            name: &m.manifest().module.name,
            version: &m.manifest().module.version,
            category: m.manifest().module.category.as_str(),
            core: m.manifest().module.core,
            manifest: json,
        })
        .collect();
    db::sync(pool, &records).await.unwrap();

    let base = Router::new().fallback(|| async { StatusCode::NOT_FOUND });
    let mount = ModuleMount::new(registry, base);
    late.set(mount.clone()).ok();
    mount.rebuild();
    mount.apply(&db::enabled_names(pool).await.unwrap().into_iter().collect());
    Env {
        app: mount.service(),
        sessions,
        pool: pool.clone(),
        mount,
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

    async fn switch(
        &self,
        who: &Browser,
        name: &str,
        enabled: bool,
        cascade: bool,
    ) -> (StatusCode, Value) {
        self.call(
            Some(who),
            "PUT",
            &format!("{BASE}/{name}"),
            Some(json!({"enabled": enabled, "cascade": cascade})),
            true,
        )
        .await
    }

    /// Status of a module's own route, through the same running application.
    async fn route(&self, name: &str) -> StatusCode {
        self.call(None, "GET", &format!("/api/v1/client/{name}"), None, false)
            .await
            .0
    }
}

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_list_shows_what_exists_what_is_on_and_what_depends_on_what(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;

    assert_eq!(
        e.call(None, "GET", BASE, None, false).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = e.call(Some(&plain), "GET", BASE, None, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );

    let (status, body) = e.call(Some(&root), "GET", BASE, None, false).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    let names: Vec<&str> = items.iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["dns", "domains", "ssl", "system"]);
    let dns = &items[0];
    assert_eq!(dns["title"]["en"], "dns title");
    assert_eq!(
        (
            dns["enabled"].as_bool(),
            dns["active"].as_bool(),
            dns["core"].as_bool()
        ),
        (Some(true), Some(true), Some(false))
    );
    assert_eq!(dns["dependents"], json!(["domains", "ssl"]));
    assert_eq!(items[1]["depends_on"], json!(["dns"]));
    assert_eq!(items[3]["core"], true);
    assert_eq!(dns["present"], true);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_switched_off_module_answers_404_at_once_and_comes_back_when_switched_on(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    assert_eq!(e.route("ssl").await, StatusCode::OK);

    let (status, body) = e.switch(&root, "ssl", false, false).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["changed"], json!(["ssl"]));
    assert_eq!(body["active"], json!(["dns", "domains", "system"]));
    // The same running application, no restart.
    assert_eq!(e.route("ssl").await, StatusCode::NOT_FOUND);
    assert_eq!(e.route("domains").await, StatusCode::OK);

    // Asking again changes nothing and is not an error.
    let (status, body) = e.switch(&root, "ssl", false, false).await;
    assert_eq!((status, &body["changed"]), (StatusCode::OK, &json!([])));

    let (status, body) = e.switch(&root, "ssl", true, false).await;
    assert_eq!(
        (status, &body["changed"]),
        (StatusCode::OK, &json!(["ssl"]))
    );
    assert_eq!(e.route("ssl").await, StatusCode::OK);

    let audit: Vec<(String, String, Value)> =
        sqlx::query_as("SELECT action, target_id, metadata FROM audit_logs ORDER BY at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(audit.len(), 2, "one entry per real change: {audit:?}");
    assert_eq!(
        (audit[0].0.as_str(), audit[0].1.as_str()),
        ("admin.module_disabled", "ssl")
    );
    assert_eq!(
        (audit[1].0.as_str(), audit[1].1.as_str()),
        ("admin.module_enabled", "ssl")
    );
    assert_eq!(audit[0].2, json!({"requested": "ssl", "cascade": false}));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn dependencies_are_respected_unless_cascade_is_asked_for(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;

    // dns is needed by domains and ssl: refused, named, nothing changed.
    let (status, body) = e.switch(&root, "dns", false, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "module_has_dependents")
    );
    assert!(
        body["detail"].as_str().unwrap().contains("domains, ssl"),
        "{body}"
    );
    assert_eq!(e.route("dns").await, StatusCode::OK);
    assert_eq!(db::enabled_names(&pool).await.unwrap().len(), 4);

    // With cascade all three go, each audited.
    let (status, body) = e.switch(&root, "dns", false, true).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["changed"], json!(["dns", "domains", "ssl"]));
    assert_eq!(body["active"], json!(["system"]));
    for name in ["dns", "domains", "ssl"] {
        assert_eq!(e.route(name).await, StatusCode::NOT_FOUND, "{name}");
    }
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'admin.module_disabled'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audited, 3);

    // ssl needs domains, which needs dns: refused, then switched on together.
    let (status, body) = e.switch(&root, "ssl", true, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "module_needs_dependencies")
    );
    assert!(
        body["detail"].as_str().unwrap().contains("dns, domains"),
        "{body}"
    );
    assert_eq!(e.route("ssl").await, StatusCode::NOT_FOUND);
    let (status, body) = e.switch(&root, "ssl", true, true).await;
    assert_eq!(
        (status, &body["changed"]),
        (StatusCode::OK, &json!(["dns", "domains", "ssl"]))
    );
    assert_eq!(e.route("ssl").await, StatusCode::OK);
    assert_eq!(e.mount.mounted().len(), 4);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn core_modules_strangers_and_bad_requests_are_refused(pool: PgPool) {
    let e = env(&pool).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;

    let (status, body) = e.switch(&root, "system", false, true).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "module_is_core")
    );
    let (status, _) = e.switch(&root, "ghost", false, false).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = e.switch(&plain, "ssl", false, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );
    let (status, _) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{BASE}/ssl"),
            Some(json!({"enabled": false})),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no CSRF header");
    let (status, _) = e
        .call(
            None,
            "PUT",
            &format!("{BASE}/ssl"),
            Some(json!({"enabled": false})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{BASE}/ssl"),
            Some(json!({"enabled": false, "force": true})),
            true,
        )
        .await;
    assert!(
        status.is_client_error(),
        "unknown fields are refused: {status}"
    );

    // Nothing of the above changed anything.
    assert_eq!(e.route("ssl").await, StatusCode::OK);
    assert_eq!(db::enabled_names(&e.pool).await.unwrap().len(), 4);
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(audited, 0);
}
