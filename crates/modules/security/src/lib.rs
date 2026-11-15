//! The web application firewall's policies (T11.02).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET, PUT, DELETE | `/api/v1/admin/waf/policy` | `admin.security.read`, `.manage` |
//! | GET, PUT, DELETE | `/api/v1/admin/waf/tenants/{id}` | the same |
//! | GET, PUT, DELETE | `/api/v1/admin/waf/domains/{id}` | the same |
//! | GET, POST | `/api/v1/admin/security/user-agents` | `admin.security.read`, `.manage` |
//! | DELETE | `/api/v1/admin/security/user-agents/{id}` | `admin.security.manage` |
//! | GET | `/api/v1/client/waf/domains` | a client |
//! | PUT, DELETE | `/api/v1/client/waf/domains/{id}` | the domain's owner |
//!
//! A policy is a mode (`off`, `detect`, `block`) and a list of rule ids that do not apply (the
//! way out of a false positive). The most specific policy that exists decides the mode (domain,
//! then account, then global; `detect` when there is none); the rules switched off are those of
//! all three together. A client sets the policy of their own domains, in `detect` or `block`
//! only (`off` is for administrators). Every change makes the account's routes be sent to its
//! node again (the global one, every account's), where the firewall is configured from them.

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::get,
};
use dhpanel_audit::{AuditEntry, Outcome};
use dhpanel_database::{
    accounts::DbAccountKind,
    domains,
    sqlx::PgPool,
    tenants, users,
    waf::{self, Mode, Policy, Scope},
};
use dhpanel_http_kit::{AppError, FieldError, ProblemDetails};
use dhpanel_rbac::{
    Principal, Require,
    permissions::{SecurityManage, SecurityRead},
};
use dhpanel_session::SessionStore;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/security/module.toml");

/// What the module's handlers share.
#[derive(Clone)]
pub struct SecurityState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
}

impl FromRef<SecurityState> for PgPool {
    fn from_ref(s: &SecurityState) -> Self {
        s.db.clone()
    }
}

impl FromRef<SecurityState> for SessionStore {
    fn from_ref(s: &SecurityState) -> Self {
        s.sessions.clone()
    }
}

/// Routes for administrators.
pub fn admin_router() -> Router<SecurityState> {
    Router::new()
        .route(
            "/api/v1/admin/waf/policy",
            get(admin_get_global)
                .put(admin_put_global)
                .delete(admin_delete_global),
        )
        .route(
            "/api/v1/admin/waf/tenants/{id}",
            get(admin_get_tenant)
                .put(admin_put_tenant)
                .delete(admin_delete_tenant),
        )
        .route(
            "/api/v1/admin/security/user-agents",
            get(list_user_agents).post(add_user_agent),
        )
        .route(
            "/api/v1/admin/security/user-agents/{id}",
            axum::routing::delete(remove_user_agent),
        )
        .route(
            "/api/v1/admin/waf/domains/{id}",
            get(admin_get_domain)
                .put(admin_put_domain)
                .delete(admin_delete_domain),
        )
}

/// Routes for hosting accounts.
pub fn client_router() -> Router<SecurityState> {
    Router::new()
        .route("/api/v1/client/waf/domains", get(client_list))
        .route(
            "/api/v1/client/waf/domains/{id}",
            axum::routing::put(client_put).delete(client_delete),
        )
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

/// A policy as written.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PolicyRequest {
    /// `off`, `detect` or `block`.
    pub mode: String,
    /// Rule ids that do not apply (such as `942100`), at most 200.
    #[serde(default)]
    pub disabled_rules: Vec<i32>,
}

/// A policy as stored.
#[derive(Debug, Serialize, ToSchema)]
pub struct PolicyView {
    /// `off`, `detect` or `block`.
    pub mode: String,
    /// Rule ids that do not apply.
    pub disabled_rules: Vec<i32>,
}

/// What applies in the end.
#[derive(Debug, Serialize, ToSchema)]
pub struct EffectiveView {
    /// `off`, `detect` or `block`.
    pub mode: String,
    /// Which policy decided the mode: `domain`, `tenant`, `global` or `default`.
    pub mode_from: String,
    /// The rules switched off, all policies together.
    pub disabled_rules: Vec<i32>,
}

/// A policy and what applies.
#[derive(Debug, Serialize, ToSchema)]
pub struct PolicyState {
    /// The policy set exactly here (`null`: none).
    pub policy: Option<PolicyView>,
    /// What really applies (for a domain; for the others the policy or the default).
    pub effective: EffectiveView,
}

fn view(p: &Policy) -> PolicyView {
    PolicyView {
        mode: p.mode.as_str().to_owned(),
        disabled_rules: p.disabled_rules.clone(),
    }
}

fn effective_view(e: waf::Effective) -> EffectiveView {
    EffectiveView {
        mode: e.mode.as_str().to_owned(),
        mode_from: e.mode_from.to_owned(),
        disabled_rules: e.disabled_rules,
    }
}

fn check(req: &PolicyRequest, client: bool) -> Result<(Mode, Vec<i32>), AppError> {
    let bad = |name: &'static str, code: &'static str, message: &str| {
        AppError::validation(vec![FieldError::new(name, code, message)])
    };
    let mode =
        Mode::parse(&req.mode).ok_or_else(|| bad("mode", "invalid", "off, detect or block."))?;
    if client && mode == Mode::Off {
        return Err(bad(
            "mode",
            "not_allowed",
            "Only an administrator can switch it off.",
        ));
    }
    let limit = if client { 50 } else { 200 };
    if req.disabled_rules.len() > limit {
        return Err(bad(
            "disabled_rules",
            "too_many",
            &format!("At most {limit} rules."),
        ));
    }
    if req
        .disabled_rules
        .iter()
        .any(|r| !(1..=999_999).contains(r))
    {
        return Err(bad(
            "disabled_rules",
            "invalid",
            "A rule id is a number from 1 to 999999.",
        ));
    }
    Ok((mode, req.disabled_rules.clone()))
}

/// The routes of these accounts are to be sent to their nodes again.
async fn resend(pool: &PgPool, tenant: Option<Uuid>) -> Result<(), AppError> {
    match tenant {
        Some(t) => {
            dhpanel_database::sqlx::query(
                "UPDATE tenants SET routes_synced_version = 0 WHERE id = $1",
            )
            .bind(t)
            .execute(pool)
            .await
        }
        None => {
            dhpanel_database::sqlx::query("UPDATE tenants SET routes_synced_version = 0")
                .execute(pool)
                .await
        }
    }
    .map_err(internal)?;
    Ok(())
}

async fn record(
    state: &SecurityState,
    action: &'static str,
    who: &Principal,
    tenant: Option<Uuid>,
    target: Uuid,
    detail: serde_json::Value,
) -> Result<(), AppError> {
    let mut entry = AuditEntry::new(action, Outcome::Success)
        .actor(who.account_id())
        .target("waf_policy", target)
        .metadata(detail);
    if let Some(t) = tenant {
        entry = entry.tenant(t);
    }
    dhpanel_audit::insert(&state.db, &entry)
        .await
        .map_err(internal)
}

async fn state_of(
    state: &SecurityState,
    scope: Scope,
    domain: Option<(Uuid, Uuid)>,
) -> Result<PolicyState, AppError> {
    let policy = waf::get(&state.db, scope).await.map_err(internal)?;
    let effective = match domain {
        Some((tenant_id, domain_id)) => waf::effective(&state.db, tenant_id, domain_id)
            .await
            .map_err(internal)?,
        None => {
            let global = waf::get(&state.db, Scope::Global).await.map_err(internal)?;
            let tenant = match scope {
                Scope::Tenant(t) => waf::get(&state.db, Scope::Tenant(t))
                    .await
                    .map_err(internal)?,
                _ => None,
            };
            waf::combine(global.as_ref(), tenant.as_ref(), None)
        }
    };
    Ok(PolicyState {
        policy: policy.as_ref().map(view),
        effective: effective_view(effective),
    })
}

/// Which account or domain a request is about, checked to exist.
enum Target {
    Global,
    Tenant(Uuid),
    Domain(Uuid, Uuid),
}

impl Target {
    fn scope(&self) -> Scope {
        match *self {
            Self::Global => Scope::Global,
            Self::Tenant(t) => Scope::Tenant(t),
            Self::Domain(tenant_id, domain_id) => Scope::Domain {
                tenant_id,
                domain_id,
            },
        }
    }

    fn tenant(&self) -> Option<Uuid> {
        match *self {
            Self::Global => None,
            Self::Tenant(t) | Self::Domain(t, _) => Some(t),
        }
    }

    fn id(&self) -> Uuid {
        match *self {
            Self::Global => Uuid::nil(),
            Self::Tenant(t) => t,
            Self::Domain(_, d) => d,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Tenant(_) => "tenant",
            Self::Domain(..) => "domain",
        }
    }
}

async fn tenant_target(state: &SecurityState, id: Uuid) -> Result<Target, AppError> {
    tenants::get(&state.db, id)
        .await
        .map_err(internal)?
        .map(|t| Target::Tenant(t.id))
        .ok_or_else(|| AppError::not_found("tenant"))
}

async fn domain_target(state: &SecurityState, id: Uuid) -> Result<Target, AppError> {
    domains::get(&state.db, id)
        .await
        .map_err(internal)?
        .map(|d| Target::Domain(d.tenant_id, d.id))
        .ok_or_else(|| AppError::not_found("domain"))
}

async fn show(state: &SecurityState, target: &Target) -> Result<Json<PolicyState>, AppError> {
    let domain = match *target {
        Target::Domain(t, d) => Some((t, d)),
        _ => None,
    };
    Ok(Json(state_of(state, target.scope(), domain).await?))
}

async fn put_policy(
    state: &SecurityState,
    who: &Principal,
    target: &Target,
    req: &PolicyRequest,
    client: bool,
) -> Result<Json<PolicyState>, AppError> {
    let (mode, rules) = check(req, client)?;
    waf::set(
        &state.db,
        target.scope(),
        mode,
        &rules,
        Some(who.account_id()),
    )
    .await
    .map_err(internal)?;
    resend(&state.db, target.tenant()).await?;
    record(
        state,
        "waf_policy.set",
        who,
        target.tenant(),
        target.id(),
        serde_json::json!({ "scope": target.label(), "mode": mode.as_str(), "disabled_rules": rules }),
    )
    .await?;
    show(state, target).await
}

async fn delete_policy(
    state: &SecurityState,
    who: &Principal,
    target: &Target,
) -> Result<StatusCode, AppError> {
    if !waf::clear(&state.db, target.scope())
        .await
        .map_err(internal)?
    {
        return Err(AppError::not_found("waf_policy"));
    }
    resend(&state.db, target.tenant()).await?;
    record(
        state,
        "waf_policy.removed",
        who,
        target.tenant(),
        target.id(),
        serde_json::json!({ "scope": target.label() }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The panel-wide policy.
#[utoipa::path(get, path = "/api/v1/admin/waf/policy", tag = "security", operation_id = "adminGetWafPolicy",
    security(("session" = [])),
    responses((status = 200, description = "The policy and what applies", body = PolicyState),
              (status = 403, description = "`permission_denied` (needs admin.security.read)", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_get_global(
    State(state): State<SecurityState>,
    _who: Require<SecurityRead>,
) -> Result<Json<PolicyState>, AppError> {
    show(&state, &Target::Global).await
}

/// Set the panel-wide policy.
#[utoipa::path(put, path = "/api/v1/admin/waf/policy", tag = "security", operation_id = "adminPutWafPolicy",
    security(("session" = [])), params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = PolicyRequest,
    responses((status = 200, description = "Set", body = PolicyState),
              (status = 422, description = "A mode or rule that is not usable", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_put_global(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Json(req): Json<PolicyRequest>,
) -> Result<Json<PolicyState>, AppError> {
    put_policy(&state, &who.0, &Target::Global, &req, false).await
}

/// Remove the panel-wide policy (the default, `detect`, applies).
#[utoipa::path(delete, path = "/api/v1/admin/waf/policy", tag = "security", operation_id = "adminDeleteWafPolicy",
    security(("session" = [])), params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 204, description = "Removed"),
              (status = 404, description = "There is none", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_delete_global(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
) -> Result<StatusCode, AppError> {
    delete_policy(&state, &who.0, &Target::Global).await
}

/// One account's policy.
#[utoipa::path(get, path = "/api/v1/admin/waf/tenants/{id}", tag = "security", operation_id = "adminGetTenantWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Account id")),
    responses((status = 200, description = "The policy and what applies", body = PolicyState),
              (status = 404, description = "No such account", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_get_tenant(
    State(state): State<SecurityState>,
    _who: Require<SecurityRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<PolicyState>, AppError> {
    let target = tenant_target(&state, id).await?;
    show(&state, &target).await
}

/// Set one account's policy.
#[utoipa::path(put, path = "/api/v1/admin/waf/tenants/{id}", tag = "security", operation_id = "adminPutTenantWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Account id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = PolicyRequest,
    responses((status = 200, description = "Set", body = PolicyState),
              (status = 404, description = "No such account", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 422, description = "A mode or rule that is not usable", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_put_tenant(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Path(id): Path<Uuid>,
    Json(req): Json<PolicyRequest>,
) -> Result<Json<PolicyState>, AppError> {
    let target = tenant_target(&state, id).await?;
    put_policy(&state, &who.0, &target, &req, false).await
}

/// Remove one account's policy.
#[utoipa::path(delete, path = "/api/v1/admin/waf/tenants/{id}", tag = "security", operation_id = "adminDeleteTenantWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Account id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 204, description = "Removed"),
              (status = 404, description = "No such account or no policy", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_delete_tenant(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let target = tenant_target(&state, id).await?;
    delete_policy(&state, &who.0, &target).await
}

/// One domain's policy.
#[utoipa::path(get, path = "/api/v1/admin/waf/domains/{id}", tag = "security", operation_id = "adminGetDomainWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Domain id")),
    responses((status = 200, description = "The policy and what applies", body = PolicyState),
              (status = 404, description = "No such domain", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_get_domain(
    State(state): State<SecurityState>,
    _who: Require<SecurityRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<PolicyState>, AppError> {
    let target = domain_target(&state, id).await?;
    show(&state, &target).await
}

/// Set one domain's policy.
#[utoipa::path(put, path = "/api/v1/admin/waf/domains/{id}", tag = "security", operation_id = "adminPutDomainWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Domain id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = PolicyRequest,
    responses((status = 200, description = "Set", body = PolicyState),
              (status = 404, description = "No such domain", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 422, description = "A mode or rule that is not usable", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_put_domain(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Path(id): Path<Uuid>,
    Json(req): Json<PolicyRequest>,
) -> Result<Json<PolicyState>, AppError> {
    let target = domain_target(&state, id).await?;
    put_policy(&state, &who.0, &target, &req, false).await
}

/// Remove one domain's policy.
#[utoipa::path(delete, path = "/api/v1/admin/waf/domains/{id}", tag = "security", operation_id = "adminDeleteDomainWafPolicy",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Domain id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 204, description = "Removed"),
              (status = 404, description = "No such domain or no policy", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn admin_delete_domain(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let target = domain_target(&state, id).await?;
    delete_policy(&state, &who.0, &target).await
}

async fn client_tenant(
    state: &SecurityState,
    who: &Principal,
) -> Result<tenants::Tenant, AppError> {
    if who.kind != DbAccountKind::Client {
        return Err(AppError::forbidden(
            "client_only",
            "This is for hosting accounts.",
        ));
    }
    let user = users::get(&state.db, who.account_id())
        .await
        .map_err(internal)?;
    let owned = user.and_then(|u| u.tenant).ok_or_else(|| {
        AppError::conflict("no_hosting", "This account has no hosting account yet.")
    })?;
    tenants::get(&state.db, owned.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("user"))
}

/// One of the account's own domains, or `404`.
async fn own_domain(
    state: &SecurityState,
    tenant: &tenants::Tenant,
    id: Uuid,
) -> Result<Target, AppError> {
    domains::get(&state.db, id)
        .await
        .map_err(internal)?
        .filter(|d| d.tenant_id == tenant.id)
        .map(|d| Target::Domain(tenant.id, d.id))
        .ok_or_else(|| AppError::not_found("domain"))
}

/// A domain with its firewall.
#[derive(Debug, Serialize, ToSchema)]
pub struct DomainWaf {
    /// The domain's id.
    pub domain_id: Uuid,
    /// Its name.
    pub fqdn: String,
    /// The domain's own policy (`null`: none).
    pub policy: Option<PolicyView>,
    /// What really applies.
    pub effective: EffectiveView,
}

/// The domains of the account.
#[derive(Debug, Serialize, ToSchema)]
pub struct DomainWafList {
    /// By name.
    pub items: Vec<DomainWaf>,
}

/// The account's domains with their firewall policy and what applies to each.
#[utoipa::path(get, path = "/api/v1/client/waf/domains", tag = "security", operation_id = "clientListWaf",
    security(("session" = [])),
    responses((status = 200, description = "The domains", body = DomainWafList),
              (status = 403, description = "`client_only`", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn client_list(
    State(state): State<SecurityState>,
    who: Principal,
) -> Result<Json<DomainWafList>, AppError> {
    let tenant = client_tenant(&state, &who).await?;
    let rows = dhpanel_database::sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, fqdn FROM domains WHERE tenant_id = $1 ORDER BY fqdn",
    )
    .bind(tenant.id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let mut items = Vec::new();
    for (domain_id, fqdn) in rows {
        let policy = waf::get(
            &state.db,
            Scope::Domain {
                tenant_id: tenant.id,
                domain_id,
            },
        )
        .await
        .map_err(internal)?;
        let effective = waf::effective(&state.db, tenant.id, domain_id)
            .await
            .map_err(internal)?;
        items.push(DomainWaf {
            domain_id,
            fqdn,
            policy: policy.as_ref().map(view),
            effective: effective_view(effective),
        });
    }
    Ok(Json(DomainWafList { items }))
}

/// Set the policy of one of the account's domains (`detect` or `block`).
#[utoipa::path(put, path = "/api/v1/client/waf/domains/{id}", tag = "security", operation_id = "clientPutWaf",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Domain id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = PolicyRequest,
    responses((status = 200, description = "Set", body = PolicyState),
              (status = 404, description = "Not one of the account's domains", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 422, description = "`off`, a mode or a rule that is not usable", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn client_put(
    State(state): State<SecurityState>,
    who: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<PolicyRequest>,
) -> Result<Json<PolicyState>, AppError> {
    let tenant = client_tenant(&state, &who).await?;
    let target = own_domain(&state, &tenant, id).await?;
    put_policy(&state, &who, &target, &req, true).await
}

/// Remove the policy of one of the account's domains (the account's or the panel's applies).
#[utoipa::path(delete, path = "/api/v1/client/waf/domains/{id}", tag = "security", operation_id = "clientDeleteWaf",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Domain id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 204, description = "Removed"),
              (status = 404, description = "Not one of the account's domains, or no policy", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn client_delete(
    State(state): State<SecurityState>,
    who: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let tenant = client_tenant(&state, &who).await?;
    let target = own_domain(&state, &tenant, id).await?;
    delete_policy(&state, &who, &target).await
}

/// A User-Agent text that is refused.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserAgentView {
    /// Id.
    pub id: Uuid,
    /// The text a User-Agent contains (any case).
    pub pattern: String,
    /// Why it is listed.
    pub note: Option<String>,
    /// When it was added (Unix seconds).
    pub created_at: i64,
}

/// The list.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserAgentList {
    /// Oldest first.
    pub items: Vec<UserAgentView>,
}

/// A text to refuse.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddUserAgent {
    /// The text (1-100 characters, no control characters): a request whose User-Agent
    /// contains it, in any case, is answered `403`.
    #[schema(example = "badbot")]
    pub pattern: String,
    /// Why (at most 200 characters).
    pub note: Option<String>,
}

/// The User-Agent texts the web servers refuse.
#[utoipa::path(get, path = "/api/v1/admin/security/user-agents", tag = "security", operation_id = "adminListUserAgents",
    security(("session" = [])),
    responses((status = 200, description = "The list", body = UserAgentList),
              (status = 403, description = "`permission_denied` (needs admin.security.read)", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn list_user_agents(
    State(state): State<SecurityState>,
    _who: Require<SecurityRead>,
) -> Result<Json<UserAgentList>, AppError> {
    let items = dhpanel_database::user_agents::list(&state.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|b| UserAgentView {
            id: b.id,
            pattern: b.pattern,
            note: b.note,
            created_at: b.created_at,
        })
        .collect();
    Ok(Json(UserAgentList { items }))
}

/// Refuse a User-Agent text on every node (they are told within seconds).
#[utoipa::path(post, path = "/api/v1/admin/security/user-agents", tag = "security", operation_id = "adminAddUserAgent",
    security(("session" = [])), params(("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    request_body = AddUserAgent,
    responses((status = 201, description = "Listed", body = UserAgentView),
              (status = 409, description = "`already_listed` or `list_full`", body = ProblemDetails, content_type = "application/problem+json"),
              (status = 422, description = "A text that is empty, too long or has a control character", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn add_user_agent(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Json(req): Json<AddUserAgent>,
) -> Result<(StatusCode, Json<UserAgentView>), AppError> {
    let pattern = req.pattern.trim().to_owned();
    let bad = |name: &'static str, message: &str| {
        AppError::validation(vec![FieldError::new(name, "invalid", message)])
    };
    if pattern.is_empty() || pattern.chars().count() > 100 || pattern.chars().any(char::is_control)
    {
        return Err(bad(
            "pattern",
            "1 to 100 characters, no control characters.",
        ));
    }
    let note = req.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > 200) {
        return Err(bad("note", "At most 200 characters."));
    }
    let id = dhpanel_database::user_agents::add(&state.db, &pattern, note, who.0.account_id())
        .await
        .map_err(|e| match e {
            dhpanel_database::user_agents::AddError::Exists => {
                AppError::conflict("already_listed", "This text is listed already.")
            }
            dhpanel_database::user_agents::AddError::Full => {
                AppError::conflict("list_full", "The list holds at most 500 texts.")
            }
            dhpanel_database::user_agents::AddError::Database(db) => internal(db),
        })?;
    record(
        &state,
        "user_agent.blocked",
        &who.0,
        None,
        id,
        serde_json::json!({ "pattern": pattern }),
    )
    .await?;
    let made = dhpanel_database::user_agents::list(&state.db)
        .await
        .map_err(internal)?
        .into_iter()
        .find(|b| b.id == id)
        .ok_or_else(|| AppError::not_found("user_agent"))?;
    Ok((
        StatusCode::CREATED,
        Json(UserAgentView {
            id: made.id,
            pattern: made.pattern,
            note: made.note,
            created_at: made.created_at,
        }),
    ))
}

/// Stop refusing a User-Agent text.
#[utoipa::path(delete, path = "/api/v1/admin/security/user-agents/{id}", tag = "security", operation_id = "adminRemoveUserAgent",
    security(("session" = [])), params(("id" = Uuid, Path, description = "Id"), ("X-CSRF-Token" = String, Header, description = "The session's csrf_token")),
    responses((status = 204, description = "Removed"),
              (status = 404, description = "Not listed", body = ProblemDetails, content_type = "application/problem+json")))]
pub async fn remove_user_agent(
    State(state): State<SecurityState>,
    who: Require<SecurityManage>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    if !dhpanel_database::user_agents::remove(&state.db, id)
        .await
        .map_err(internal)?
    {
        return Err(AppError::not_found("user_agent"));
    }
    record(
        &state,
        "user_agent.unblocked",
        &who.0,
        None,
        id,
        serde_json::json!({}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The module's OpenAPI document.
#[derive(OpenApi)]
#[openapi(
    paths(
        admin_get_global, admin_put_global, admin_delete_global, admin_get_tenant, admin_put_tenant,
        admin_delete_tenant, admin_get_domain, admin_put_domain, admin_delete_domain, client_list,
        client_put, client_delete, list_user_agents, add_user_agent, remove_user_agent
    ),
    components(schemas(UserAgentView, UserAgentList, AddUserAgent, PolicyRequest, PolicyView, EffectiveView, PolicyState, DomainWaf, DomainWafList)),
    tags((name = "security", description = "The web application firewall"))
)]
pub struct SecurityApi;
