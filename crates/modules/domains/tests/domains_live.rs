//! Domains over HTTP, against real PostgreSQL and Redis.
//! Ignored by default; run with DATABASE_URL (superuser), REDIS_URL and `--include-ignored`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use dhpanel_cache::RedisHandle;
use dhpanel_database::{modules as db, rbac::assign_role};
use dhpanel_kernel::{Manifest, Module, ModuleMount, ModuleRoutes, Registry};
use dhpanel_mod_domains::{DomainsState, SETTINGS_SCHEMA, admin_router, client_router};
use dhpanel_session::{CSRF_HEADER, NewSession, SessionConfig, SessionStore, set_cookie};
use dhpanel_settings::{Scope, SettingsRegistry};
use secrecy::SecretString;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt as _;
use uuid::Uuid;

const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
const ADMIN: &str = "/api/v1/admin/domains";
const CLIENT: &str = "/api/v1/client/domains";

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

fn manifest(name: &str) -> Manifest {
    Manifest::parse(
        name,
        &format!(
            "[module]\nname = \"{name}\"\nversion = \"1.0.0\"\ntitle = {{ en = \"{name} title\" }}\ncategory = \"core\"\ncore = true\n"
        ),
    )
    .unwrap()
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
            "domains".to_owned(),
            serde_json::from_str(SETTINGS_SCHEMA).unwrap(),
        )])
        .unwrap(),
    );
    let domains = Arc::new(Fake {
        manifest: manifest("domains"),
        routes: Box::new({
            let (pool, sessions, settings) = (pool.clone(), sessions.clone(), settings.clone());
            move || {
                let state = DomainsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    settings: settings.clone(),
                };
                ModuleRoutes {
                    admin: Some(admin_router().with_state(state.clone())),
                    client: Some(client_router().with_state(state)),
                    ..ModuleRoutes::none()
                }
            }
        }),
    });
    let registry = Registry::build(vec![domains]).unwrap();
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

/// A client account with a hosting account on the default plan.
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

fn domain(fqdn: &str) -> Value {
    json!({ "fqdn": fqdn })
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_client_adds_lists_changes_and_removes_its_own_domains(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let browser = e.sign_in(alice).await;
    assert_eq!(
        e.call(None, "GET", CLIENT, None, false).await.0,
        StatusCode::UNAUTHORIZED
    );

    // The first domain is the main one, the next ones are addons; names are normalised.
    let (status, main) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(domain("Example.COM")),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{main}");
    assert_eq!(
        (main["fqdn"].clone(), main["kind"].clone()),
        (json!("example.com"), json!("primary"))
    );
    assert_eq!(main["status"], "active");
    let (_, second) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "bücher.de", "docroot": "sites/books"})),
            true,
        )
        .await;
    assert_eq!(
        (
            second["kind"].clone(),
            second["fqdn"].clone(),
            second["unicode"].clone()
        ),
        (
            json!("addon"),
            json!("xn--bcher-kva.de"),
            json!("bücher.de")
        )
    );
    assert_eq!(second["docroot"], "sites/books");
    let main_id = main["id"].as_str().unwrap();
    let (status, blog) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "blog.example.com", "kind": "sub", "parent_domain_id": main_id, "docroot": "blog"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{blog}");
    let (_, alias) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "example.net", "kind": "alias", "parent_domain_id": main_id})),
            true,
        )
        .await;
    assert_eq!(alias["kind"], "alias");

    // List, filter, search, page.
    let (_, all) = e.call(Some(&browser), "GET", CLIENT, None, false).await;
    assert_eq!(all["total"], 4);
    let names: Vec<&str> = all["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["fqdn"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "blog.example.com",
            "example.com",
            "example.net",
            "xn--bcher-kva.de"
        ]
    );
    let (_, subs) = e
        .call(
            Some(&browser),
            "GET",
            &format!("{CLIENT}?kind=sub"),
            None,
            false,
        )
        .await;
    assert_eq!(subs["total"], 1);
    let (_, found) = e
        .call(
            Some(&browser),
            "GET",
            &format!("{CLIENT}?q=EXAMPLE"),
            None,
            false,
        )
        .await;
    assert_eq!(found["total"], 3);
    let (_, page) = e
        .call(
            Some(&browser),
            "GET",
            &format!("{CLIENT}?limit=2&offset=3"),
            None,
            false,
        )
        .await;
    assert_eq!(
        (
            page["items"].as_array().unwrap().len(),
            page["total"].clone()
        ),
        (1, json!(4))
    );
    for bad in ["limit=0", "limit=999", "kind=nope", "status=x", "offset=-1"] {
        let (status, _) = e
            .call(
                Some(&browser),
                "GET",
                &format!("{CLIENT}?{bad}"),
                None,
                false,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // The document root: plain relative paths only.
    let blog_path = format!("{CLIENT}/{}", blog["id"].as_str().unwrap());
    let (status, changed) = e
        .call(
            Some(&browser),
            "PATCH",
            &blog_path,
            Some(json!({"docroot": "apps/blog"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["docroot"], "apps/blog");
    for bad in ["../etc", "/abs", "a//b"] {
        let (status, _) = e
            .call(
                Some(&browser),
                "PATCH",
                &blog_path,
                Some(json!({"docroot": bad})),
                true,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // Bad names say what is wrong; nothing is added.
    for (name, want) in [
        ("com", "domain_not_registrable"),
        ("localhost.test", "domain_unknown_suffix"),
        ("1.2.3.4", "domain_is_ip_address"),
        ("exa mple.com", "invalid_domain"),
    ] {
        let (status, b) = e
            .call(Some(&browser), "POST", CLIENT, Some(domain(name)), true)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
        assert_eq!(b["errors"][0]["code"], want, "{name}: {b}");
    }
    let (status, b) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(domain("example.com")),
            true,
        )
        .await;
    assert_eq!((status, code(&b)), (StatusCode::CONFLICT, "domain_taken"));
    let (status, b) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "second-main.org", "kind": "primary"})),
            true,
        )
        .await;
    assert_eq!((status, code(&b)), (StatusCode::CONFLICT, "primary_exists"));
    let (status, _) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "x.example.com", "kind": "sub"})),
            true,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a subdomain names its parent"
    );
    let (status, _) = e
        .call(
            Some(&browser),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "elsewhere.org", "kind": "sub", "parent_domain_id": main_id})),
            true,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "and lies below it"
    );

    // Removing: the main domain stays while others exist; a subdomain goes alone.
    let (status, b) = e
        .call(
            Some(&browser),
            "DELETE",
            &format!("{CLIENT}/{main_id}"),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::CONFLICT, "primary_has_domains")
    );
    assert_eq!(
        e.call(Some(&browser), "DELETE", &blog_path, None, true)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Some(&browser), "GET", &blog_path, None, false)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // The log has it, naming the account and the domain.
    let actions: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, target_id FROM audit_logs WHERE action LIKE 'domain.%' ORDER BY at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(actions.iter().filter(|a| a.0 == "domain.added").count() == 4);
    assert!(actions.iter().any(|a| a.0 == "domain.docroot_changed"));
    assert!(
        actions
            .iter()
            .any(|a| a.0 == "domain.deleted" && a.1.as_deref() == blog["id"].as_str())
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn one_account_cannot_see_or_touch_or_build_below_anothers_domains(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let main_id = main["id"].as_str().unwrap().to_owned();

    // The DoD: someone else's subdomain is a 409.
    for (name, kind) in [
        ("shop.example.com", "addon"),
        ("shop.example.com", "primary"),
        ("a.b.example.com", "addon"),
    ] {
        let (status, body) = e
            .call(
                Some(&b),
                "POST",
                CLIENT,
                Some(json!({"fqdn": name, "kind": kind})),
                true,
            )
            .await;
        assert_eq!(
            (status, code(&body)),
            (StatusCode::CONFLICT, "domain_owned_by_other"),
            "{name}"
        );
    }
    // Using her domain as a parent looks like using a domain that is not there.
    let (status, body) = e
        .call(
            Some(&b),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "www.example.com", "kind": "sub", "parent_domain_id": main_id})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["errors"][0]["code"], "parent_not_found");
    // Her domain is invisible to him.
    let path = format!("{CLIENT}/{main_id}");
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            Some(&b),
            "PATCH",
            &path,
            Some(json!({"docroot": "x"})),
            true
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Some(&b), "DELETE", &path, None, true).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, list) = e.call(Some(&b), "GET", CLIENT, None, false).await;
    assert_eq!(list["total"], 0);
    // His own look-alike is fine.
    let (status, _) = e
        .call(
            Some(&b),
            "POST",
            CLIENT,
            Some(domain("myexample.com")),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // With sharing permitted by the administrator, he may host a subdomain of hers.
    let mut tx = pool.begin().await.unwrap();
    dhpanel_settings::set(
        &e.settings,
        &mut tx,
        "domains",
        Scope::Global,
        json!({"permit_subdomain_sharing": true})
            .as_object()
            .unwrap(),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (status, _) = e
        .call(
            Some(&b),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "shop.example.com", "kind": "addon"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn the_plan_limits_the_number_of_domains_and_subdomains_and_must_include_the_feature(
    pool: PgPool,
) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice).await;
    let plan: Uuid = sqlx::query_scalar("SELECT plan_id FROM tenants WHERE id = $1")
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Two names and one subdomain.
    sqlx::query(
        "UPDATE plans SET limits = '{\"max_domains\": 2, \"max_subdomains\": 1}' WHERE id = $1",
    )
    .bind(plan)
    .execute(&pool)
    .await
    .unwrap();
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (status, _) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("two.com")), true)
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("three.com")), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "limit_reached"),
        "{body}"
    );
    let sub = |name: &str| json!({"fqdn": name, "kind": "sub", "parent_domain_id": main["id"]});
    let (status, _) = e
        .call(Some(&a), "POST", CLIENT, Some(sub("a.example.com")), true)
        .await;
    assert_eq!(status, StatusCode::CREATED, "subdomains count apart");
    let (status, body) = e
        .call(Some(&a), "POST", CLIENT, Some(sub("b.example.com")), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "limit_reached")
    );
    // Aliases count with the domains.
    let (status, _) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "alias.org", "kind": "alias", "parent_domain_id": main["id"]})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Unlimited, and a plan that does not list the limit gets the module default (10).
    sqlx::query("UPDATE plans SET limits = '{\"max_domains\": -1}' WHERE id = $1")
        .bind(plan)
        .execute(&pool)
        .await
        .unwrap();
    for i in 0..15 {
        let (status, body) = e
            .call(
                Some(&a),
                "POST",
                CLIENT,
                Some(domain(&format!("site{i}.org"))),
                true,
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{i}: {body}");
    }
    sqlx::query("UPDATE plans SET limits = '{}' WHERE id = $1")
        .bind(plan)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("over.org")), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "limit_reached"),
        "17 names are over the default of 10"
    );

    // A plan without the feature cannot add domains at all.
    let set: Uuid = sqlx::query_scalar(
        "INSERT INTO feature_sets (name, features) VALUES ('nodomains', '{}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE plans SET feature_set_id = $2, limits = '{\"max_domains\": -1}' WHERE id = $1",
    )
    .bind(plan)
    .bind(set)
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("nope.org")), true)
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn blocked_names_are_refused_and_administrators_manage_any_domain(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;

    // Blocked by the administrator: the name and everything below it.
    let mut tx = pool.begin().await.unwrap();
    dhpanel_settings::set(
        &e.settings,
        &mut tx,
        "domains",
        Scope::Global,
        json!({"blocked_domains": "Blocked.org,\n gov"})
            .as_object()
            .unwrap(),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for name in ["blocked.org", "www.blocked.org", "agency.gov"] {
        let (status, body) = e
            .call(Some(&a), "POST", CLIENT, Some(domain(name)), true)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
        assert_eq!(body["errors"][0]["code"], "domain_blocked", "{name}");
    }
    assert_eq!(
        e.call(
            Some(&a),
            "POST",
            CLIENT,
            Some(domain("notblocked.org")),
            true
        )
        .await
        .0,
        StatusCode::CREATED
    );

    // Administrators: permissions, adding for a user, listing everyone's, suspending.
    let (status, b) = e.call(Some(&plain), "GET", ADMIN, None, false).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );
    let (status, b) = e.call(Some(&root), "GET", CLIENT, None, false).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::FORBIDDEN, "client_only"),
        "administrators use the admin routes"
    );
    let (status, b) = e.call(Some(&a), "GET", ADMIN, None, false).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::FORBIDDEN, "permission_denied"),
        "a client is not an administrator"
    );
    let (status, added) = e
        .call(
            Some(&root),
            "POST",
            ADMIN,
            Some(json!({"user_id": alice, "fqdn": "byadmin.org", "docroot": "web"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{added}");
    assert_eq!(added["tenant_username"], "alice");
    let (status, _) = e
        .call(
            Some(&root),
            "POST",
            ADMIN,
            Some(json!({"user_id": Uuid::now_v7(), "fqdn": "ghost.org"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, bobby_domains) = {
        let (bobby, _) = client(&pool, "bobby", 10_001).await;
        let b = e.sign_in(bobby).await;
        e.call(Some(&b), "POST", CLIENT, Some(domain("bobbys.org")), true)
            .await;
        e.call(
            Some(&root),
            "GET",
            &format!("{ADMIN}?user_id={bobby}"),
            None,
            false,
        )
        .await
    };
    assert_eq!(bobby_domains["total"], 1);
    let (_, all) = e.call(Some(&root), "GET", ADMIN, None, false).await;
    assert_eq!(all["total"], 3);

    let id = added["id"].as_str().unwrap();
    let (status, s) = e
        .call(
            Some(&root),
            "POST",
            &format!("{ADMIN}/{id}/suspend"),
            None,
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["status"], "suspended");
    assert!(s["suspended_at"].is_string());
    let (_, suspended) = e
        .call(
            Some(&root),
            "GET",
            &format!("{ADMIN}?status=suspended"),
            None,
            false,
        )
        .await;
    assert_eq!(suspended["total"], 1);
    // The client sees it but has no way to undo it.
    let (_, seen) = e
        .call(Some(&a), "GET", &format!("{CLIENT}/{id}"), None, false)
        .await;
    assert_eq!(seen["status"], "suspended");
    let (status, _) = e
        .call(
            Some(&a),
            "POST",
            &format!("{CLIENT}/{id}/unsuspend"),
            None,
            true,
        )
        .await;
    assert!(matches!(
        status,
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
    ));
    let (_, s) = e
        .call(
            Some(&root),
            "POST",
            &format!("{ADMIN}/{id}/unsuspend"),
            None,
            true,
        )
        .await;
    assert_eq!(
        (s["status"].clone(), s["suspended_at"].clone()),
        (json!("active"), Value::Null)
    );
    assert_eq!(
        e.call(Some(&root), "DELETE", &format!("{ADMIN}/{id}"), None, true)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let _ = tenant;
}
