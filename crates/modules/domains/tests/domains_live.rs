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
use dhpanel_dns::mock::MockResolver;
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
    resolver: MockResolver,
}

struct Browser {
    cookie: String,
    csrf: String,
}

async fn env(pool: &PgPool) -> Env {
    env_limited(pool, 1000).await
}

/// The environment with the TLS question limit set (per minute and address).
async fn env_limited(pool: &PgPool, ask_limit: u32) -> Env {
    let url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let redis = RedisHandle::new(&SecretString::from(url)).expect("valid REDIS_URL");
    let sessions = SessionStore::new(redis.clone(), pool.clone(), SessionConfig::default());
    let settings = Arc::new(
        SettingsRegistry::build(vec![(
            "domains".to_owned(),
            serde_json::from_str(SETTINGS_SCHEMA).unwrap(),
        )])
        .unwrap(),
    );
    let resolver = MockResolver::new();
    let ask_limiter = dhpanel_cache::ratelimit::RateLimiter::new(
        redis.clone(),
        format!("test-{}", Uuid::now_v7().simple()),
        ask_limit,
        std::time::Duration::from_secs(60),
    );
    let domains = Arc::new(Fake {
        manifest: manifest("domains"),
        routes: Box::new({
            let (pool, sessions, settings, resolver, ask_limiter) = (
                pool.clone(),
                sessions.clone(),
                settings.clone(),
                resolver.clone(),
                ask_limiter.clone(),
            );
            move || {
                let state = DomainsState {
                    db: pool.clone(),
                    sessions: sessions.clone(),
                    settings: settings.clone(),
                    resolver: Arc::new(resolver.clone()),
                    ask_limiter: ask_limiter.clone(),
                    master_key: dhpanel_auth_core::secretbox::MasterKey::from_bytes(&[7u8; 32]),
                };
                ModuleRoutes {
                    admin: Some(admin_router().with_state(state.clone())),
                    client: Some(client_router().with_state(state.clone())),
                    public: Some(dhpanel_mod_domains::tls::public_router().with_state(state)),
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
        resolver,
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

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn redirects_can_be_added_changed_and_removed_and_loops_are_caught(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (_, www) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "www.example.com", "kind": "sub", "parent_domain_id": main["id"]})),
            true,
        )
        .await;
    let path = format!("{CLIENT}/{}/redirects", main["id"].as_str().unwrap());
    let www_path = format!("{CLIENT}/{}/redirects", www["id"].as_str().unwrap());
    let add = |p: String, source: &str, target: &str| {
        let (e, a) = (&e, &a);
        let body = json!({"source_path": source, "target": target});
        async move { e.call(Some(a), "POST", &p, Some(body), true).await }
    };

    // Add, with the default code, and list.
    let (status, r) = add(path.clone(), "/old", "/new").await;
    assert_eq!(status, StatusCode::CREATED, "{r}");
    assert_eq!(
        (r["code"].clone(), r["source_path"].clone()),
        (json!(301), json!("/old"))
    );
    let (status, r2) = e
        .call(Some(&a), "POST", &path, Some(json!({"source_path": "/promo", "target": "https://example.org/sale?x=1", "code": 302})), true)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{r2}");
    let (_, list) = e.call(Some(&a), "GET", &path, None, false).await;
    let sources: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["source_path"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["/old", "/promo"]);

    // Bad input says what is wrong.
    for (source, target, field) in [
        ("old", "/new", "source_path"),
        ("/a?x=1", "/new", "source_path"),
        ("/z", "javascript:alert(1)", "target"),
        ("/z", "//evil.example/x", "target"),
        ("/z", "https://user:pw@example.org/", "target"),
        ("/z", "", "target"),
    ] {
        let (status, body) = add(path.clone(), source, target).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{source} {target}"
        );
        assert_eq!(
            body["errors"][0]["field"], field,
            "{source} {target}: {body}"
        );
    }
    let (status, body) = e
        .call(
            Some(&a),
            "POST",
            &path,
            Some(json!({"source_path": "/z", "target": "/y", "code": 303})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["errors"][0]["code"], "invalid_redirect_code");
    let (status, body) = add(path.clone(), "/old", "/other").await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::CONFLICT, "redirect_exists")
    );

    // Change the target and the code; the path stays.
    let one = format!("{path}/{}", r["id"].as_str().unwrap());
    let (status, changed) = e
        .call(
            Some(&a),
            "PATCH",
            &one,
            Some(json!({"target": "https://example.net/", "code": 308})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        (changed["target"].clone(), changed["code"].clone()),
        (json!("https://example.net/"), json!(308))
    );
    let (status, _) = e.call(Some(&a), "PATCH", &one, Some(json!({})), true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Loops: back to itself, around two paths, around two domains.
    let (status, body) = add(path.clone(), "/self", "/self").await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("redirect_loop")),
        "{body}"
    );
    assert_eq!(add(path.clone(), "/a", "/b").await.0, StatusCode::CREATED);
    let (status, body) = add(path.clone(), "/b", "/a").await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("redirect_loop"))
    );
    let message = body["errors"][0]["message"].as_str().unwrap();
    assert!(
        message.contains("example.com/b -> example.com/a -> example.com/b"),
        "{message}"
    );
    // The query string does not hide a loop, and a longer way round does not either.
    assert_eq!(
        add(path.clone(), "/c", "/d?x=1").await.0,
        StatusCode::CREATED
    );
    assert_eq!(add(path.clone(), "/d", "/e").await.0, StatusCode::CREATED);
    let (status, body) = add(path.clone(), "/e", "/c").await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("redirect_loop"))
    );
    // Across names: example.com -> www.example.com -> example.com.
    assert_eq!(
        add(path.clone(), "/", "https://www.example.com/").await.0,
        StatusCode::CREATED
    );
    let (status, body) = add(www_path.clone(), "/", "https://example.com/").await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("redirect_loop")),
        "{body}"
    );
    // Changing an existing redirect cannot close a circle either.
    let (_, list) = e.call(Some(&a), "GET", &path, None, false).await;
    let id_of = |p: &str| {
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["source_path"] == p)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (status, body) = e
        .call(
            Some(&a),
            "PATCH",
            &format!("{path}/{}", id_of("/d")),
            Some(json!({"target": "/c"})),
            true,
        )
        .await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("redirect_loop"))
    );
    // A redirect leaving the panel's domains is fine.
    assert_eq!(
        add(www_path.clone(), "/", "https://elsewhere.example.org/")
            .await
            .0,
        StatusCode::CREATED
    );

    // A very long chain is refused rather than followed for ever.
    let mut last = StatusCode::CREATED;
    let mut body = Value::Null;
    for i in (0..14).rev() {
        let (status, b2) = add(path.clone(), &format!("/hop{i}"), &format!("/hop{}", i + 1)).await;
        if status != StatusCode::CREATED {
            (last, body) = (status, b2);
            break;
        }
    }
    assert_eq!(last, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body["errors"][0]["code"], "redirect_chain_too_long",
        "{body}"
    );

    // Removing.
    assert_eq!(
        e.call(Some(&a), "DELETE", &one, None, true).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Some(&a), "DELETE", &one, None, true).await.0,
        StatusCode::NOT_FOUND
    );

    // Somebody else's domain and redirects are not there; administrators see them all.
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            Some(&b),
            "POST",
            &path,
            Some(json!({"source_path": "/x", "target": "/y"})),
            true
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let admin_path = format!("{ADMIN}/{}/redirects", main["id"].as_str().unwrap());
    let (status, theirs) = e.call(Some(&root), "GET", &admin_path, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(theirs["items"].as_array().unwrap().len() >= 5);
    let (status, _) = e
        .call(
            Some(&root),
            "POST",
            &admin_path,
            Some(json!({"source_path": "/byadmin", "target": "/new"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // Redirects are a plan feature; looking is always allowed.
    let set: Uuid = sqlx::query_scalar(
        "INSERT INTO feature_sets (name, features) VALUES ('plain', '{domains}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE plans SET feature_set_id = $1 WHERE is_default")
        .bind(set)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = add(path.clone(), "/denied", "/x").await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    assert_eq!(
        e.call(Some(&a), "GET", &path, None, false).await.0,
        StatusCode::OK
    );

    // Audit.
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT action FROM audit_logs WHERE action LIKE 'redirect.%' ORDER BY action",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actions,
        ["redirect.added", "redirect.changed", "redirect.deleted"]
    );
}

async fn zone_lines(pool: &PgPool, zone_name: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT r.name || ' ' || r.rtype || ' ' || r.content || coalesce(' p' || r.priority, '')
         FROM dns_records r JOIN dns_zones z ON z.id = r.zone_id
         WHERE z.name = $1 ORDER BY r.name, r.rtype, r.content",
    )
    .bind(zone_name)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_new_domain_gets_a_zone_from_the_default_template(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice).await;
    sqlx::query("UPDATE nodes SET public_ip = '192.0.2.10'")
        .execute(&pool)
        .await
        .unwrap();

    // IPv4 only: the AAAA line is left out.
    let (status, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{main}");
    assert_eq!(
        zone_lines(&pool, "example.com").await,
        [
            "@ A 192.0.2.10",
            "@ MX example.com. p10",
            "@ TXT v=spf1 a mx ~all",
            "www CNAME example.com."
        ]
    );
    let zone: (String, String) =
        sqlx::query_as("SELECT status, provider FROM dns_zones WHERE name = 'example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        zone,
        ("pending".to_owned(), "powerdns".to_owned()),
        "not pushed to the DNS server yet"
    );

    // A subdomain puts its address into the parent's zone, and takes it out when it goes.
    let (_, blog) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(
                json!({"fqdn": "blog.example.com", "kind": "sub", "parent_domain_id": main["id"]}),
            ),
            true,
        )
        .await;
    assert!(
        zone_lines(&pool, "example.com")
            .await
            .contains(&"blog A 192.0.2.10".to_owned())
    );
    let zones: i64 = sqlx::query_scalar("SELECT count(*) FROM dns_zones")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(zones, 1, "a subdomain has no zone of its own");
    let path = format!("{CLIENT}/{}", blog["id"].as_str().unwrap());
    assert_eq!(
        e.call(Some(&a), "DELETE", &path, None, true).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(
        !zone_lines(&pool, "example.com")
            .await
            .iter()
            .any(|l| l.starts_with("blog "))
    );

    // Addons and aliases have zones of their own; both address kinds appear when the node has IPv6.
    sqlx::query("UPDATE nodes SET public_ip = '2001:db8::10'")
        .execute(&pool)
        .await
        .unwrap();
    e.call(Some(&a), "POST", CLIENT, Some(domain("second.org")), true)
        .await;
    assert_eq!(
        zone_lines(&pool, "second.org").await,
        [
            "@ AAAA 2001:db8::10",
            "@ MX second.org. p10",
            "@ TXT v=spf1 a mx ~all",
            "www CNAME second.org."
        ]
    );
    e.call(
        Some(&a),
        "POST",
        CLIENT,
        Some(json!({"fqdn": "alias.net", "kind": "alias", "parent_domain_id": main["id"]})),
        true,
    )
    .await;
    assert!(!zone_lines(&pool, "alias.net").await.is_empty());

    // No address known for the server: the zone is made, without address records.
    sqlx::query("UPDATE nodes SET public_ip = NULL")
        .execute(&pool)
        .await
        .unwrap();
    e.call(Some(&a), "POST", CLIENT, Some(domain("noaddr.com")), true)
        .await;
    let lines = zone_lines(&pool, "noaddr.com").await;
    assert!(
        lines
            .iter()
            .all(|l| !l.contains(" A ") && !l.contains(" AAAA ")),
        "{lines:?}"
    );
    assert_eq!(lines.len(), 3);

    // The zone goes with its domain.
    let id = main["id"].as_str().unwrap();
    for sub_id in sqlx::query_scalar::<_, Uuid>("SELECT id FROM domains WHERE kind <> 'primary'")
        .fetch_all(&pool)
        .await
        .unwrap()
    {
        e.call(
            Some(&a),
            "DELETE",
            &format!("{CLIENT}/{sub_id}"),
            None,
            true,
        )
        .await;
    }
    assert_eq!(
        e.call(Some(&a), "DELETE", &format!("{CLIENT}/{id}"), None, true)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM dns_zones")
        .fetch_one(&pool)
        .await
        .unwrap();
    let records: i64 = sqlx::query_scalar("SELECT count(*) FROM dns_records")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((left, records), (0, 0));
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn administrators_edit_the_zone_templates_new_zones_start_from(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice).await;
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;
    const T: &str = "/api/v1/admin/dns/templates";
    sqlx::query("UPDATE nodes SET public_ip = '192.0.2.10'")
        .execute(&pool)
        .await
        .unwrap();

    let (status, b) = e.call(Some(&plain), "GET", T, None, false).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::FORBIDDEN, "permission_denied")
    );
    assert_eq!(
        e.call(Some(&a), "GET", T, None, false).await.0,
        StatusCode::FORBIDDEN
    );
    let (status, list) = e.call(Some(&root), "GET", T, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"][0]["name"], "default");
    assert_eq!(
        (
            list["items"][0]["is_default"].clone(),
            list["items"][0]["built_in"].clone()
        ),
        (json!(true), json!(true))
    );
    assert_eq!(list["items"][0]["records"].as_array().unwrap().len(), 5);
    let default_id = list["items"][0]["id"].as_str().unwrap().to_owned();

    // Bad templates say which record is wrong; nothing is saved.
    for (records, want) in [
        (
            json!([{"name": "@", "type": "A", "content": "not-an-ip"}]),
            "invalid_ipv4",
        ),
        (
            json!([{"name": "@", "type": "A", "content": "{{oops}}"}]),
            "unknown_variable",
        ),
        (
            json!([{"name": "www", "type": "A", "content": "192.0.2.1"}, {"name": "www", "type": "CNAME", "content": "x.org."}]),
            "cname_conflict",
        ),
        (
            json!([{"name": "@", "type": "MX", "content": "m.example.com."}]),
            "priority_required",
        ),
    ] {
        let (status, body) = e
            .call(
                Some(&root),
                "POST",
                T,
                Some(json!({"name": "bad", "records": records})),
                true,
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{records}");
        assert_eq!(body["errors"][0]["code"], want, "{body}");
    }
    let (status, _) = e
        .call(
            Some(&root),
            "POST",
            T,
            Some(json!({"name": "Bad Name", "records": []})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A new template, made the default: the next zone starts from it.
    let (status, created) = e
        .call(
            Some(&root),
            "POST",
            T,
            Some(json!({
                "name": "minimal",
                "description": "Just the site",
                "is_default": true,
                "records": [
                    {"name": "@", "type": "A", "content": "{{ipv4}}", "ttl": 300},
                    {"name": "@", "type": "TXT", "content": "managed by {{domain}}"}
                ]
            })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["is_default"], true);
    let (status, b) = e
        .call(
            Some(&root),
            "POST",
            T,
            Some(json!({"name": "minimal", "records": []})),
            true,
        )
        .await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::CONFLICT, "template_name_taken")
    );
    e.call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    assert_eq!(
        zone_lines(&pool, "example.com").await,
        ["@ A 192.0.2.10", "@ TXT managed by example.com"]
    );
    let ttl: i32 = sqlx::query_scalar("SELECT ttl FROM dns_records WHERE rtype = 'A'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ttl, 300);
    let (_, defaults) = e.call(Some(&root), "GET", T, None, false).await;
    let flagged: Vec<&str> = defaults["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["is_default"] == true)
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(flagged, ["minimal"], "only one default");

    // Changing a template does not touch zones that exist.
    let minimal = format!("{T}/{}", created["id"].as_str().unwrap());
    let (status, updated) = e
        .call(Some(&root), "PUT", &minimal, Some(json!({"name": "minimal", "records": [{"name": "@", "type": "A", "content": "{{ipv4}}"}], "is_default": true})), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(zone_lines(&pool, "example.com").await.len(), 2);
    // The built-in one keeps its name and cannot be deleted; the default cannot be deleted.
    let built_in = format!("{T}/{default_id}");
    let (status, b) = e
        .call(
            Some(&root),
            "PUT",
            &built_in,
            Some(json!({"name": "renamed", "records": []})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (status, b) = e.call(Some(&root), "DELETE", &built_in, None, true).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::CONFLICT, "template_built_in")
    );
    let (status, b) = e.call(Some(&root), "DELETE", &minimal, None, true).await;
    assert_eq!(
        (status, code(&b)),
        (StatusCode::CONFLICT, "template_is_default")
    );
    // Back to the built-in default, then the other one can go.
    let (status, _) = e
        .call(Some(&root), "PUT", &built_in, Some(json!({"name": "default", "records": [{"name": "@", "type": "A", "content": "{{ipv4}}"}], "is_default": true})), true)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        e.call(Some(&root), "DELETE", &minimal, None, true).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Some(&root), "DELETE", &minimal, None, true).await.0,
        StatusCode::NOT_FOUND
    );

    let actions: Vec<String> = sqlx::query_scalar("SELECT DISTINCT action FROM audit_logs WHERE action LIKE 'admin.dns_template_%' ORDER BY action")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        actions,
        [
            "admin.dns_template_created",
            "admin.dns_template_deleted",
            "admin.dns_template_updated"
        ]
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn dns_records_are_added_changed_and_removed_with_clear_errors(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    sqlx::query("UPDATE nodes SET public_ip = '192.0.2.10'")
        .execute(&pool)
        .await
        .unwrap();
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (_, blog) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(
                json!({"fqdn": "blog.example.com", "kind": "sub", "parent_domain_id": main["id"]}),
            ),
            true,
        )
        .await;
    let path = format!("{CLIENT}/{}/dns/records", main["id"].as_str().unwrap());

    // The zone made from the template is there, and says it has not reached the server.
    let (status, list) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(
        (list["zone"]["name"].clone(), list["zone"]["status"].clone()),
        (json!("example.com"), json!("pending"))
    );
    let names: Vec<String> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            format!(
                "{} {}",
                r["name"].as_str().unwrap(),
                r["type"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        names.contains(&"www CNAME".to_owned()) && names.contains(&"@ MX".to_owned()),
        "{names:?}"
    );
    let www = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "www")
        .unwrap();
    assert_eq!(www["fqdn"], "www.example.com");

    // Add one of every type.
    sqlx::query("UPDATE dns_zones SET status = 'active'")
        .execute(&pool)
        .await
        .unwrap();
    let add = |body: Value| {
        let (e, a, path) = (&e, &a, path.clone());
        async move { e.call(Some(a), "POST", &path, Some(body), true).await }
    };
    for body in [
        json!({"name": "api", "type": "A", "content": "192.0.2.20", "ttl": 300}),
        json!({"name": "api", "type": "AAAA", "content": "2001:DB8::20"}),
        json!({"name": "mail", "type": "A", "content": "192.0.2.30"}),
        json!({"name": "@", "type": "MX", "content": "mail.example.com", "priority": 20}),
        json!({"name": "_dmarc", "type": "TXT", "content": "v=DMARC1; p=none"}),
        json!({"name": "_sip._tcp", "type": "SRV", "content": "10 5060 sip.example.com", "priority": 5}),
        json!({"name": "@", "type": "CAA", "content": "0 issue letsencrypt.org"}),
        json!({"name": "sub", "type": "NS", "content": "ns1.other.net"}),
        json!({"name": "*.wild", "type": "A", "content": "192.0.2.40"}),
    ] {
        let (status, r) = add(body.clone()).await;
        assert_eq!(status, StatusCode::CREATED, "{body}: {r}");
    }
    let (_, r) = add(json!({"name": "CDN", "type": "cname", "content": "Edge.Example.NET"})).await;
    assert_eq!(
        (r["name"].clone(), r["type"].clone(), r["content"].clone()),
        (json!("cdn"), json!("CNAME"), json!("edge.example.net.")),
        "normalised"
    );
    let (_, listed) = e.call(Some(&a), "GET", &path, None, false).await;
    let aaaa = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["type"] == "AAAA" && r["name"] == "api")
        .unwrap();
    assert_eq!(aaaa["content"], "2001:db8::20");
    assert_eq!(
        listed["zone"]["status"], "pending",
        "a change marks the zone for the next push"
    );

    // Wrong records: 422, the field and what is wrong.
    for (body, field, code_) in [
        (
            json!({"name": "x", "type": "A", "content": "256.0.0.1"}),
            "content",
            "invalid_ipv4",
        ),
        (
            json!({"name": "x", "type": "AAAA", "content": "192.0.2.1"}),
            "content",
            "invalid_ipv6",
        ),
        (
            json!({"name": "x", "type": "A", "content": "192.0.2.1", "ttl": 5}),
            "ttl",
            "ttl_out_of_range",
        ),
        (
            json!({"name": "x", "type": "A", "content": "192.0.2.1", "ttl": 9_999_999}),
            "ttl",
            "ttl_out_of_range",
        ),
        (
            json!({"name": "x", "type": "MX", "content": "mail.example.com"}),
            "priority",
            "priority_required",
        ),
        (
            json!({"name": "x", "type": "A", "content": "192.0.2.1", "priority": 5}),
            "priority",
            "priority_not_allowed",
        ),
        (
            json!({"name": "x", "type": "TXT", "content": "say \"hi\""}),
            "content",
            "invalid_txt",
        ),
        (
            json!({"name": "x", "type": "SRV", "content": "5060 host", "priority": 1}),
            "content",
            "invalid_srv",
        ),
        (
            json!({"name": "x", "type": "CAA", "content": "0 badtag x.org"}),
            "content",
            "invalid_caa",
        ),
        (
            json!({"name": "bad name", "type": "A", "content": "192.0.2.1"}),
            "name",
            "invalid_record_name",
        ),
        (
            json!({"name": "x.", "type": "A", "content": "192.0.2.1"}),
            "name",
            "invalid_record_name",
        ),
        (
            json!({"name": "@", "type": "CNAME", "content": "x.org"}),
            "name",
            "cname_at_apex",
        ),
        (
            json!({"name": "@", "type": "NS", "content": "ns.x.org"}),
            "name",
            "apex_ns_reserved",
        ),
        (
            json!({"name": "x", "type": "SOA", "content": "x"}),
            "type",
            "unsupported_record_type",
        ),
        (
            json!({"name": "x", "type": "CNAME", "content": "a b"}),
            "content",
            "invalid_record_target",
        ),
    ] {
        let (status, body_) = add(body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {body_}");
        assert_eq!(
            (
                body_["errors"][0]["field"].clone(),
                body_["errors"][0]["code"].clone()
            ),
            (json!(field), json!(code_)),
            "{body}: {body_}"
        );
        assert!(!body_["errors"][0]["message"].as_str().unwrap().is_empty());
    }
    // Clashes with what is there: 409.
    for (body, want) in [
        (
            json!({"name": "www", "type": "A", "content": "192.0.2.50"}),
            "cname_conflict",
        ),
        (
            json!({"name": "api", "type": "CNAME", "content": "x.org"}),
            "cname_conflict",
        ),
        (
            json!({"name": "cdn", "type": "CNAME", "content": "other.org"}),
            "cname_conflict",
        ),
        (
            json!({"name": "api", "type": "A", "content": "192.0.2.20"}),
            "duplicate_record",
        ),
    ] {
        let (status, body_) = add(body.clone()).await;
        assert_eq!(
            (status, code(&body_)),
            (StatusCode::CONFLICT, want),
            "{body}: {body_}"
        );
    }
    assert_eq!(
        add(json!({"name": "api", "type": "A", "content": "192.0.2.21"}))
            .await
            .0,
        StatusCode::CREATED,
        "two A records at one name are fine"
    );

    // Change: content, TTL; the same checks apply.
    let id_of = |l: &Value, n: &str, t: &str, c: &str| {
        l["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == n && r["type"] == t && r["content"] == c)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (_, listed) = e.call(Some(&a), "GET", &path, None, false).await;
    let mail = format!("{path}/{}", id_of(&listed, "mail", "A", "192.0.2.30"));
    let (status, changed) = e
        .call(
            Some(&a),
            "PATCH",
            &mail,
            Some(json!({"content": "192.0.2.31", "ttl": 120})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        (
            changed["content"].clone(),
            changed["ttl"].clone(),
            changed["name"].clone()
        ),
        (json!("192.0.2.31"), json!(120), json!("mail"))
    );
    let (status, body_) = e
        .call(
            Some(&a),
            "PATCH",
            &mail,
            Some(json!({"content": "not-an-ip"})),
            true,
        )
        .await;
    assert_eq!(
        (status, body_["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("invalid_ipv4"))
    );
    let (status, _) = e
        .call(Some(&a), "PATCH", &mail, Some(json!({})), true)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let api2 = format!("{path}/{}", id_of(&listed, "api", "A", "192.0.2.21"));
    let (status, body_) = e
        .call(
            Some(&a),
            "PATCH",
            &api2,
            Some(json!({"content": "192.0.2.20"})),
            true,
        )
        .await;
    assert_eq!(
        (status, code(&body_)),
        (StatusCode::CONFLICT, "duplicate_record")
    );
    let mx = format!("{path}/{}", id_of(&listed, "@", "MX", "mail.example.com."));
    let (status, mxr) = e
        .call(Some(&a), "PATCH", &mx, Some(json!({"priority": 5})), true)
        .await;
    assert_eq!(
        (status, mxr["priority"].clone()),
        (StatusCode::OK, json!(5))
    );

    // Remove.
    assert_eq!(
        e.call(Some(&a), "DELETE", &api2, None, true).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Some(&a), "DELETE", &api2, None, true).await.0,
        StatusCode::NOT_FOUND
    );

    // A subdomain's records are in its parent's zone; someone else's zone is not there.
    let blog_path = format!("{CLIENT}/{}/dns/records", blog["id"].as_str().unwrap());
    let (status, body_) = e.call(Some(&a), "GET", &blog_path, None, false).await;
    assert_eq!((status, code(&body_)), (StatusCode::CONFLICT, "no_zone"));
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(
            Some(&b),
            "POST",
            &path,
            Some(json!({"name": "x", "type": "A", "content": "192.0.2.1"})),
            true
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Administrators see and change any zone.
    let admin_path = format!("{ADMIN}/{}/dns/records", main["id"].as_str().unwrap());
    let (status, theirs) = e.call(Some(&root), "GET", &admin_path, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(theirs["items"].as_array().unwrap().len() > 8);
    let (status, _) = e
        .call(
            Some(&root),
            "POST",
            &admin_path,
            Some(json!({"name": "byadmin", "type": "A", "content": "192.0.2.77"})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // DNS editing is a plan feature; looking is always allowed.
    let set: Uuid = sqlx::query_scalar(
        "INSERT INTO feature_sets (name, features) VALUES ('nodns', '{domains}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE plans SET feature_set_id = $1 WHERE is_default")
        .bind(set)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body_) = add(json!({"name": "denied", "type": "A", "content": "192.0.2.1"})).await;
    assert_eq!(
        (status, code(&body_)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    assert_eq!(
        e.call(Some(&a), "GET", &path, None, false).await.0,
        StatusCode::OK
    );

    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT action FROM audit_logs WHERE action LIKE 'dns.record_%' ORDER BY action",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        actions,
        [
            "dns.record_added",
            "dns.record_changed",
            "dns.record_deleted"
        ]
    );
}

async fn set_nameservers(e: &Env, pool: &PgPool, values: Value) {
    let mut tx = pool.begin().await.unwrap();
    dhpanel_settings::set(
        &e.settings,
        &mut tx,
        "domains",
        Scope::Global,
        values.as_object().unwrap(),
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn clients_are_told_the_name_servers_and_can_check_their_domains(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (_, blog) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(
                json!({"fqdn": "blog.example.com", "kind": "sub", "parent_domain_id": main["id"]}),
            ),
            true,
        )
        .await;
    let check = format!("{CLIENT}/{}/dns/check", main["id"].as_str().unwrap());

    // Nothing set yet: the list is empty and a check says so without asking the DNS.
    let (status, list) = e
        .call(
            Some(&a),
            "GET",
            "/api/v1/client/dns/nameservers",
            None,
            false,
        )
        .await;
    assert_eq!(
        (status, list["nameservers"].clone()),
        (StatusCode::OK, json!([]))
    );
    let (status, got) = e.call(Some(&a), "POST", &check, None, true).await;
    assert_eq!(status, StatusCode::OK, "{got}");
    assert_eq!(got["status"], "not_configured");

    // The administrator sets them (any case, with or without the dot, blanks and repeats ignored).
    set_nameservers(&e, &pool, json!({"nameserver_1": "NS1.Panel.net.", "nameserver_2": "ns2.panel.net", "nameserver_3": "", "nameserver_4": "ns1.panel.net"})).await;
    let (_, list) = e
        .call(
            Some(&a),
            "GET",
            "/api/v1/client/dns/nameservers",
            None,
            false,
        )
        .await;
    assert_eq!(
        list["nameservers"],
        json!(["ns1.panel.net", "ns2.panel.net"])
    );
    let (_, list) = e
        .call(
            Some(&root),
            "GET",
            "/api/v1/admin/dns/nameservers",
            None,
            false,
        )
        .await;
    assert_eq!(
        list["nameservers"],
        json!(["ns1.panel.net", "ns2.panel.net"])
    );
    // Only valid host names can be saved.
    let mut tx = pool.begin().await.unwrap();
    let bad = dhpanel_settings::set(
        &e.settings,
        &mut tx,
        "domains",
        Scope::Global,
        json!({"nameserver_1": "not a name"}).as_object().unwrap(),
        None,
    )
    .await;
    assert!(bad.is_err());
    drop(tx);

    // A check asks the public DNS and compares; a repeat within seconds reuses the answer.
    sqlx::query("DELETE FROM dns_zones WHERE name = 'example.com' AND false")
        .execute(&pool)
        .await
        .unwrap();
    for (found, want, missing) in [
        (json!(["ns1.panel.net.", "ns2.panel.net."]), "ok", json!([])),
        (
            json!(["ns1.panel.net."]),
            "partial",
            json!(["ns2.panel.net"]),
        ),
        (
            json!(["ns.other.org."]),
            "mismatch",
            json!(["ns1.panel.net", "ns2.panel.net"]),
        ),
        (
            json!([]),
            "not_delegated",
            json!(["ns1.panel.net", "ns2.panel.net"]),
        ),
    ] {
        e.resolver.answer(
            "example.com",
            Ok(found
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect()),
        );
        sqlx::query("UPDATE dns_zones SET delegation_checked_at = now() - interval '1 minute'")
            .execute(&pool)
            .await
            .unwrap();
        let (status, got) = e.call(Some(&a), "POST", &check, None, true).await;
        assert_eq!(status, StatusCode::OK, "{got}");
        assert_eq!(
            (
                got["status"].clone(),
                got["missing"].clone(),
                got["cached"].clone()
            ),
            (json!(want), missing, json!(false)),
            "{got}"
        );
        assert_eq!(got["expected"], json!(["ns1.panel.net", "ns2.panel.net"]));
    }
    let (_, again) = e.call(Some(&a), "POST", &check, None, true).await;
    assert_eq!(
        (again["cached"].clone(), again["status"].clone()),
        (json!(true), json!("not_delegated")),
        "within 15 seconds the answer is reused"
    );

    // A resolver that cannot be asked is a 503, not a guess.
    e.resolver.answer(
        "example.com",
        Err(dhpanel_dns::DnsError::Unavailable("timeout".into())),
    );
    sqlx::query("UPDATE dns_zones SET delegation_checked_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body_) = e.call(Some(&a), "POST", &check, None, true).await;
    assert_eq!(
        (status, code(&body_)),
        (StatusCode::SERVICE_UNAVAILABLE, "dns_lookup_failed")
    );

    // A subdomain uses its parent's name servers; other accounts' domains are not there.
    let (status, body_) = e
        .call(
            Some(&a),
            "POST",
            &format!("{CLIENT}/{}/dns/check", blog["id"].as_str().unwrap()),
            None,
            true,
        )
        .await;
    assert_eq!((status, code(&body_)), (StatusCode::CONFLICT, "no_zone"));
    assert_eq!(
        e.call(Some(&b), "POST", &check, None, true).await.0,
        StatusCode::NOT_FOUND
    );
    // Administrators may check any domain; plain admins may not.
    let plain = e
        .sign_in(account(&pool, "plainadmin", &["admin"]).await)
        .await;
    e.resolver.answer(
        "example.com",
        Ok(vec!["ns1.panel.net.".into(), "ns2.panel.net.".into()]),
    );
    sqlx::query("UPDATE dns_zones SET delegation_checked_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, got) = e
        .call(
            Some(&root),
            "POST",
            &format!("{ADMIN}/{}/dns/check", main["id"].as_str().unwrap()),
            None,
            true,
        )
        .await;
    assert_eq!(
        (status, got["status"].clone()),
        (StatusCode::OK, json!("ok"))
    );
    assert_eq!(
        e.call(
            Some(&plain),
            "GET",
            "/api/v1/admin/dns/nameservers",
            None,
            false
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_certificate_may_be_issued_only_for_active_domains_of_running_accounts(pool: PgPool) {
    const ASK: &str = "/api/v1/public/tls/ask";
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let a = e.sign_in(alice).await;
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    e.call(
        Some(&a),
        "POST",
        CLIENT,
        Some(json!({"fqdn": "blog.example.com", "kind": "sub", "parent_domain_id": main["id"]})),
        true,
    )
    .await;
    e.call(
        Some(&a),
        "POST",
        CLIENT,
        Some(json!({"fqdn": "example.net", "kind": "alias", "parent_domain_id": main["id"]})),
        true,
    )
    .await;
    let ask = |name: &str| {
        let e = &e;
        let path = format!("{ASK}?domain={name}");
        async move { e.call(None, "GET", &path, None, false).await.0 }
    };

    // No session needed (the web server asks); the domains of the panel get a yes.
    for name in [
        "example.com",
        "blog.example.com",
        "example.net",
        "EXAMPLE.com",
        "example.com.",
    ] {
        assert_eq!(ask(name).await, StatusCode::OK, "{name}");
    }
    // Everything else gets a no: unknown names, parents and neighbours, other endings.
    for name in [
        "unknown.org",
        "www.example.com",
        "example.org",
        "com",
        "a.blog.example.com",
        "xample.com",
    ] {
        assert_eq!(ask(name).await, StatusCode::NOT_FOUND, "{name}");
    }
    // Not a host name at all.
    for name in [
        "",
        "a%20b",
        "http://example.com",
        "example.com/x",
        "*.example.com",
        "1.2.3.4",
    ] {
        assert_eq!(ask(name).await, StatusCode::BAD_REQUEST, "{name:?}");
    }
    assert_eq!(
        e.call(None, "GET", ASK, None, false).await.0,
        StatusCode::BAD_REQUEST,
        "no name given"
    );

    // A suspended domain gets no certificate; a suspended account does (its page is shown over HTTPS).
    sqlx::query(
        "UPDATE domains SET status = 'suspended', suspended_at = now() WHERE fqdn = 'example.net'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(ask("example.net").await, StatusCode::NOT_FOUND);
    assert_eq!(ask("example.com").await, StatusCode::OK);
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ask("example.com").await, StatusCode::OK);
    // One that is being deleted, failed or not set up yet gets none.
    for status in ["deleting", "failed", "provisioning"] {
        let sql = if status == "deleting" {
            "UPDATE tenants SET status = 'deleting', deleted_at = now(), purge_after = now() + interval '7 days' WHERE id = $1"
        } else {
            "UPDATE tenants SET status = $2, deleted_at = NULL, purge_after = NULL WHERE id = $1"
        };
        let mut q = sqlx::query(sql).bind(tenant);
        if status != "deleting" {
            q = q.bind(status);
        }
        q.execute(&pool).await.unwrap();
        assert_eq!(ask("example.com").await, StatusCode::NOT_FOUND, "{status}");
    }
    // The answer carries nothing.
    sqlx::query(
        "UPDATE tenants SET status = 'active', deleted_at = NULL, purge_after = NULL WHERE id = $1",
    )
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();
    let res = e
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{ASK}?domain=example.com"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        axum::body::to_bytes(res.into_body(), 1024)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn an_address_that_asks_too_often_is_told_to_wait(pool: PgPool) {
    let e = env_limited(&pool, 5).await;
    for i in 1..=5 {
        let (status, _) = e
            .call(
                None,
                "GET",
                "/api/v1/public/tls/ask?domain=nothing.example.com",
                None,
                false,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "question {i}");
    }
    let (status, body) = e
        .call(
            None,
            "GET",
            "/api/v1/public/tls/ask?domain=nothing.example.com",
            None,
            false,
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::TOO_MANY_REQUESTS, "too_many_requests")
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_domains_certificate_is_shown_to_its_owner_and_domain_events_are_published(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let id = main["id"].as_str().unwrap().to_owned();
    let path = format!("{CLIENT}/{id}/ssl");

    // None yet.
    let (status, none) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{none}");
    assert_eq!(none["state"], "none");
    assert_eq!(none["issuer"], Value::Null);

    // A node reports one: valid, then ending soon, then ended.
    let put = |days: i64| {
        let (pool, id) = (pool.clone(), id.clone());
        async move {
            sqlx::query(
                "INSERT INTO certificates (domain_id, issuer, not_before, not_after)
                 VALUES ($1, 'R11', now() - interval '30 days', now() + make_interval(days => $2::int))
                 ON CONFLICT (domain_id) DO UPDATE SET not_after = EXCLUDED.not_after, not_before = EXCLUDED.not_before",
            )
            .bind(id.parse::<Uuid>().unwrap())
            .bind(days as i32)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    put(60).await;
    let (_, valid) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(
        (
            valid["state"].clone(),
            valid["kind"].clone(),
            valid["issuer"].clone()
        ),
        (json!("valid"), json!("acme"), json!("R11"))
    );
    assert!(
        (59..=60).contains(&valid["days_left"].as_i64().unwrap()),
        "{valid}"
    );
    assert!(valid["not_after"].is_string());
    put(5).await;
    assert_eq!(
        e.call(Some(&a), "GET", &path, None, false).await.1["state"],
        "expiring"
    );
    put(-2).await;
    let (_, ended) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(ended["state"], "expired");
    assert!(ended["days_left"].as_i64().unwrap() < 0);

    // Someone else's domain is not there; administrators see it.
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, seen) = e
        .call(
            Some(&root),
            "GET",
            &format!("{ADMIN}/{id}/ssl"),
            None,
            false,
        )
        .await;
    assert_eq!(
        (status, seen["state"].clone()),
        (StatusCode::OK, json!("expired"))
    );
    assert_eq!(
        e.call(
            Some(&root),
            "GET",
            &format!("{ADMIN}/{}/ssl", Uuid::now_v7()),
            None,
            false
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Adding and removing a domain are announced to the other modules.
    let (_, extra) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("second.org")), true)
        .await;
    e.call(
        Some(&a),
        "DELETE",
        &format!("{CLIENT}/{}", extra["id"].as_str().unwrap()),
        None,
        true,
    )
    .await;
    let events: Vec<(String, Value)> =
        sqlx::query_as("SELECT event_type, payload FROM outbox ORDER BY created_at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(
        kinds,
        ["domain_added", "domain_added", "domain_removed"],
        "{events:?}"
    );
    assert_eq!(events[1].1["name"], "second.org");
    assert_eq!(events[2].1["domain_id"], extra["id"]);
}

/// A CA, then a leaf for `names` signed by it: `(chain pem, key pem)`.
fn make_certificate(names: &[&str], from_days: i64, to_days: i64) -> (String, String) {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Test CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_key = KeyPair::generate().unwrap();
    let ca_pem = ca_params.self_signed(&ca_key).unwrap().pem();
    let issuer = Issuer::new(ca_params, ca_key);
    let mut params =
        CertificateParams::new(names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>()).unwrap();
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now + time::Duration::days(from_days);
    params.not_after = now + time::Duration::days(to_days);
    let key = KeyPair::generate().unwrap();
    let leaf = params.signed_by(&key, &issuer).unwrap().pem();
    (format!("{leaf}{ca_pem}"), key.serialize_pem())
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn an_uploaded_certificate_is_checked_kept_sealed_and_never_given_back(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, _) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let root = e
        .sign_in(account(&pool, "rootuser", &["super_admin"]).await)
        .await;
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let id = main["id"].as_str().unwrap().to_owned();
    let path = format!("{CLIENT}/{id}/ssl/custom");
    let put = |cert: &str, key: &str| {
        let (e, a, path) = (&e, &a, path.clone());
        let body = json!({"certificate": cert, "private_key": key});
        async move { e.call(Some(a), "PUT", &path, Some(body), true).await }
    };

    // A good upload: shown as a custom certificate, with its issuer and days.
    let (chain, key) = make_certificate(&["example.com", "www.example.com"], -1, 90);
    let (status, shown) = put(&chain, &key).await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(
        (
            shown["state"].clone(),
            shown["kind"].clone(),
            shown["issuer"].clone()
        ),
        (json!("valid"), json!("custom"), json!("Test CA"))
    );
    assert!((89..=90).contains(&shown["days_left"].as_i64().unwrap()));
    // The key is stored sealed: not readable from the table, and the answer never holds it.
    let (stored, sealed): (String, Vec<u8>) =
        sqlx::query_as("SELECT chain_pem, private_key_enc FROM certificates")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stored.contains("BEGIN CERTIFICATE") && !stored.contains("PRIVATE"));
    assert!(
        !String::from_utf8_lossy(&sealed).contains("PRIVATE"),
        "sealed"
    );
    assert!(!shown.to_string().contains("PRIVATE") && !shown.to_string().contains("BEGIN"));
    let (_, got) = e
        .call(Some(&a), "GET", &format!("{CLIENT}/{id}/ssl"), None, false)
        .await;
    assert!(!got.to_string().contains("PRIVATE"));
    // It can be opened with the panel's key and the domain, and not with another domain.
    let master = dhpanel_auth_core::secretbox::MasterKey::from_bytes(&[7u8; 32]);
    let ctx = format!("certificate-key:{id}");
    let opened = master.open(ctx.as_bytes(), &sealed).unwrap();
    assert!(
        String::from_utf8(opened)
            .unwrap()
            .contains("BEGIN PRIVATE KEY")
    );
    assert!(
        master
            .open(
                format!("certificate-key:{}", Uuid::now_v7()).as_bytes(),
                &sealed
            )
            .is_err()
    );

    // The roadmap's case: a key that is not the certificate's is a 422 naming the key.
    let (_, other_key) = make_certificate(&["example.com"], -1, 90);
    let (status, body) = put(&chain, &other_key).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        (
            body["errors"][0]["field"].clone(),
            body["errors"][0]["code"].clone()
        ),
        (json!("private_key"), json!("key_mismatch"))
    );
    assert!(!body.to_string().contains("PRIVATE"));

    // Other mistakes, each with its field and code.
    let (elsewhere, elsewhere_key) = make_certificate(&["other.org"], -1, 90);
    let (old, old_key) = make_certificate(&["example.com"], -90, -1);
    let (future, future_key) = make_certificate(&["example.com"], 3, 90);
    for (cert, k, field, want) in [
        (
            elsewhere.as_str(),
            elsewhere_key.as_str(),
            "certificate",
            "name_mismatch",
        ),
        (
            old.as_str(),
            old_key.as_str(),
            "certificate",
            "certificate_expired",
        ),
        (
            future.as_str(),
            future_key.as_str(),
            "certificate",
            "certificate_not_yet_valid",
        ),
        ("hello", key.as_str(), "certificate", "invalid_certificate"),
        (
            chain.as_str(),
            "not a key",
            "private_key",
            "invalid_private_key",
        ),
        (
            chain.as_str(),
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n",
            "private_key",
            "key_encrypted",
        ),
    ] {
        let (status, body) = put(cert, k).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{want}: {body}");
        assert_eq!(
            (
                body["errors"][0]["field"].clone(),
                body["errors"][0]["code"].clone()
            ),
            (json!(field), json!(want)),
            "{body}"
        );
    }
    // Nothing changed by the refused ones.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM certificates WHERE kind = 'custom'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    // A node's report never replaces it; someone else cannot touch it.
    assert_eq!(
        e.call(
            Some(&b),
            "PUT",
            &path,
            Some(json!({"certificate": chain, "private_key": key})),
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
    let (status, seen) = e
        .call(
            Some(&root),
            "GET",
            &format!("{ADMIN}/{id}/ssl"),
            None,
            false,
        )
        .await;
    assert_eq!(
        (status, seen["kind"].clone()),
        (StatusCode::OK, json!("custom"))
    );

    // Removing gives the domain back to the web server's own certificates.
    assert_eq!(
        e.call(Some(&a), "DELETE", &path, None, true).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Some(&a), "DELETE", &path, None, true).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Some(&a), "GET", &format!("{CLIENT}/{id}/ssl"), None, false)
            .await
            .1["state"],
        "none"
    );

    // Administrators may upload for any domain; the plan must allow it for clients.
    let (status, _) = e
        .call(
            Some(&root),
            "PUT",
            &format!("{ADMIN}/{id}/ssl/custom"),
            Some(json!({"certificate": chain, "private_key": key})),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let set: Uuid = sqlx::query_scalar(
        "INSERT INTO feature_sets (name, features) VALUES ('nocustom', '{domains}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE plans SET feature_set_id = $1 WHERE is_default")
        .bind(set)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = put(&chain, &key).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );

    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT action FROM audit_logs WHERE action LIKE 'ssl.%' ORDER BY action",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, ["ssl.custom_removed", "ssl.custom_uploaded"]);
    let logged: Vec<Value> =
        sqlx::query_scalar("SELECT metadata FROM audit_logs WHERE action LIKE 'ssl.%'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        logged.iter().all(|m| !m.to_string().contains("PRIVATE")),
        "the log never holds key material"
    );
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn force_https_and_hsts_are_per_domain_and_hsts_needs_force_https(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (_, other) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("other.org")), true)
        .await;
    let path = format!("{CLIENT}/{}", main["id"].as_str().unwrap());
    assert_eq!(
        (main["force_https"].clone(), main["hsts"].clone()),
        (json!(false), json!(false)),
        "off until asked for"
    );
    let patch = |body: Value| {
        let (e, a, path) = (&e, &a, path.clone());
        async move { e.call(Some(a), "PATCH", &path, Some(body), true).await }
    };
    let version = || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT routes_version FROM tenants WHERE id = $1")
                .bind(tenant)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    // HSTS alone would lock visitors out of a site that still answers on plain HTTP.
    let (status, body) = patch(json!({"hsts": true})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        (
            body["errors"][0]["field"].clone(),
            body["errors"][0]["code"].clone()
        ),
        (json!("hsts"), json!("hsts_requires_https"))
    );
    let before = version().await;
    let (status, on) = patch(json!({"force_https": true})).await;
    assert_eq!(status, StatusCode::OK, "{on}");
    assert_eq!(
        (on["force_https"].clone(), on["hsts"].clone()),
        (json!(true), json!(false))
    );
    assert!(
        version().await > before,
        "a change reaches the web server through the route sync"
    );
    let (status, both) = patch(json!({"hsts": true})).await;
    assert_eq!(
        (status, both["hsts"].clone()),
        (StatusCode::OK, json!(true))
    );
    // Forced HTTPS cannot be turned off under HSTS; both together can.
    let (status, body) = patch(json!({"force_https": false})).await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!("hsts_requires_https")
        )
    );
    let still = version().await;
    let (status, off) = patch(json!({"force_https": false, "hsts": false})).await;
    assert_eq!(
        (status, off["force_https"].clone(), off["hsts"].clone()),
        (StatusCode::OK, json!(false), json!(false))
    );
    assert!(version().await > still);
    // Setting what is already so changes nothing; with a document root too, both are applied.
    let same = version().await;
    assert_eq!(patch(json!({"force_https": false})).await.0, StatusCode::OK);
    assert_eq!(version().await, same, "nothing changed, nothing to send");
    let (status, mixed) =
        patch(json!({"docroot": "site", "force_https": true, "hsts": true})).await;
    assert_eq!(status, StatusCode::OK, "{mixed}");
    assert_eq!(
        (
            mixed["docroot"].clone(),
            mixed["force_https"].clone(),
            mixed["hsts"].clone()
        ),
        (json!("site"), json!(true), json!(true))
    );
    assert_eq!(patch(json!({})).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        patch(json!({"force_https": "yes"})).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // One domain's settings are not another's, nor another account's.
    let (_, listed) = e
        .call(
            Some(&a),
            "GET",
            &format!("{CLIENT}/{}", other["id"].as_str().unwrap()),
            None,
            false,
        )
        .await;
    assert_eq!(
        (listed["force_https"].clone(), listed["hsts"].clone()),
        (json!(false), json!(false))
    );
    assert_eq!(
        e.call(
            Some(&b),
            "PATCH",
            &path,
            Some(json!({"force_https": false})),
            true
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action FROM audit_logs WHERE action = 'domain.https_changed'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(actions.len(), 4, "on, hsts on, both off, both on");
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn a_domains_rate_limit_and_headers_are_checked_kept_and_need_the_feature(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let path = format!("{CLIENT}/{}/policy", main["id"].as_str().unwrap());
    let put = |body: Value| {
        let (e, a, path) = (&e, &a, path.clone());
        async move { e.call(Some(a), "PUT", &path, Some(body), true).await }
    };
    let version = || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT routes_version FROM tenants WHERE id = $1")
                .bind(tenant)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    // Reading is open; an empty policy is what a new domain has.
    let (status, empty) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty, json!({"rate_limit_per_second": null, "headers": []}));
    // Changing needs the feature in the plan.
    sqlx::query(
        "UPDATE feature_sets SET all_features = false, features = ARRAY['domains']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = put(json!({"rate_limit_per_second": 10, "headers": []})).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    sqlx::query(
        "UPDATE feature_sets SET features = ARRAY['domains', 'web_policy']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let before = version().await;
    let good = json!({
        "rate_limit_per_second": 20,
        "headers": [{"name": "X-Frame-Options", "value": "SAMEORIGIN"}]
    });
    let (status, saved) = put(good.clone()).await;
    assert_eq!((status, &saved), (StatusCode::OK, &good), "{saved}");
    assert!(
        version().await > before,
        "the web server gets it with the next sync"
    );
    let (_, read) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(read, good);

    // What could break the file or the site is refused, with the field named.
    for (body, field, why) in [
        (
            json!({"rate_limit_per_second": 0, "headers": []}),
            "rate_limit_per_second",
            "invalid_rate_limit",
        ),
        (
            json!({"rate_limit_per_second": 5000, "headers": []}),
            "rate_limit_per_second",
            "invalid_rate_limit",
        ),
        (
            json!({"rate_limit_per_second": null, "headers": [{"name": "Strict-Transport-Security", "value": "x"}]}),
            "headers",
            "reserved_header",
        ),
        (
            json!({"rate_limit_per_second": null, "headers": [{"name": "X-A", "value": "a\"; deny all; \""}]}),
            "headers",
            "invalid_header_value",
        ),
        (
            json!({"rate_limit_per_second": null, "headers": [{"name": "X A", "value": "x"}]}),
            "headers",
            "invalid_header_name",
        ),
    ] {
        let (status, body) = put(body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(
            (
                body["errors"][0]["field"].clone(),
                body["errors"][0]["code"].clone()
            ),
            (json!(field), json!(why))
        );
    }
    assert_eq!(
        put(json!({"rate_limit_per_second": 5, "unknown": 1}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, read) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(read, good, "a refused change leaves the policy as it was");

    // Not another account's.
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Some(&b), "PUT", &path, Some(good.clone()), true)
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // An empty policy removes it.
    let (status, cleared) = put(json!({"rate_limit_per_second": null, "headers": []})).await;
    assert_eq!((status, cleared), (StatusCode::OK, empty));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM domain_policies")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    let actions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'domain.policy_changed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(actions, 2);
}

#[sqlx::test(migrations = "../../../migrations")]
#[ignore = "needs PostgreSQL + Redis"]
async fn an_owner_sees_its_web_server_file_and_adds_only_allowed_directives(pool: PgPool) {
    let e = env(&pool).await;
    let (alice, tenant) = client(&pool, "alice", 10_000).await;
    let (bobby, _) = client(&pool, "bobby", 10_001).await;
    let (a, b) = (e.sign_in(alice).await, e.sign_in(bobby).await);
    let (_, main) = e
        .call(Some(&a), "POST", CLIENT, Some(domain("example.com")), true)
        .await;
    let (_, alias) = e
        .call(
            Some(&a),
            "POST",
            CLIENT,
            Some(json!({"fqdn": "example.net", "kind": "alias", "parent_domain_id": main["id"]})),
            true,
        )
        .await;
    let path = format!("{CLIENT}/{}/vhost", main["id"].as_str().unwrap());
    let alias_path = format!("{CLIENT}/{}/vhost", alias["id"].as_str().unwrap());
    let put = |path: String, text: &str| {
        let (e, a) = (&e, &a);
        let body = json!({ "custom_block": text });
        async move { e.call(Some(a), "PUT", &path, Some(body), true).await }
    };

    // Without the feature the page is closed, reading too.
    sqlx::query(
        "UPDATE feature_sets SET all_features = false, features = ARRAY['domains']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "feature_disabled")
    );
    assert_eq!(
        put(path.clone(), "return 200 x;").await.0,
        StatusCode::FORBIDDEN
    );
    sqlx::query(
        "UPDATE feature_sets SET features = ARRAY['domains', 'edit_vhost']::feature_key[]
         WHERE id = (SELECT feature_set_id FROM plans WHERE is_default)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The file the site runs on, with the alias in its server_name.
    let (status, view) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    let config = view["config"].as_str().unwrap();
    assert!(
        config.contains("server_name example.com example.net;"),
        "{config}"
    );
    assert!(
        config.contains("fastcgi_pass dh-alice-php:9000;"),
        "{config}"
    );
    assert_eq!(view["custom_block"], "");
    assert_eq!(view["max_custom_bytes"], 8192);
    // An alias shows its domain's file and has none of its own to change.
    let (_, via_alias) = e.call(Some(&a), "GET", &alias_path, None, false).await;
    assert_eq!(via_alias["config"], view["config"]);
    let (status, body) = put(alias_path.clone(), "return 200 x;").await;
    assert_eq!(
        (status, body["errors"][0]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("alias_has_no_file"))
    );

    // An allowed block is saved, shown in the file, and makes the tenant out of date.
    let before: i64 = sqlx::query_scalar("SELECT routes_version FROM tenants WHERE id = $1")
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .unwrap();
    let block = "location /old { return 301 /new; }\nclient_max_body_size 64m;";
    let (status, saved) = put(path.clone(), block).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["custom_block"], block);
    assert!(
        saved["config"]
            .as_str()
            .unwrap()
            .contains("    location /old { return 301 /new; }\n    client_max_body_size 64m;\n")
    );
    let after: i64 = sqlx::query_scalar("SELECT routes_version FROM tenants WHERE id = $1")
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(after > before);

    // What is not allowed is refused, naming the directive; the block stays as it was.
    for (text, why, mention) in [
        ("alias /etc/;", "forbidden_directive", "alias"),
        (
            "location / { proxy_pass http://10.0.2.2:5432; }",
            "forbidden_directive",
            "proxy_pass",
        ),
        ("include /etc/passwd;", "forbidden_directive", "include"),
        (
            "add_header Strict-Transport-Security \"x\";",
            "reserved_header",
            "Strict-Transport-Security",
        ),
        (
            "add_header X \"a#\"; root /;",
            "forbidden_directive",
            "root",
        ),
        ("return 200 \"open", "template_syntax", ""),
    ] {
        let (status, body) = put(path.clone(), text).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}: {body}");
        assert_eq!(body["errors"][0]["field"], "custom_block");
        assert_eq!(body["errors"][0]["code"], why, "{text}");
        assert!(
            body["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(mention),
            "{body}"
        );
    }
    let (_, kept) = e.call(Some(&a), "GET", &path, None, false).await;
    assert_eq!(kept["custom_block"], block);

    // Not another account's.
    assert_eq!(
        e.call(Some(&b), "GET", &path, None, false).await.0,
        StatusCode::NOT_FOUND
    );
    // An empty text removes it; the file is the plain one again.
    let (status, cleared) = put(path.clone(), "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["custom_block"], "");
    assert!(!cleared["config"].as_str().unwrap().contains("/old"));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM domain_custom_blocks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    // The audit log notes the change and never the text.
    let logged: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT metadata FROM audit_logs WHERE action = 'domain.vhost_changed' ORDER BY at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(logged.len(), 2);
    assert!(!logged.iter().any(|m| m.to_string().contains("/old")));
}
