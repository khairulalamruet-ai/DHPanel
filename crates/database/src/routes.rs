//! What the web server needs to know about a tenant (T05.14b): its domains and
//! their redirects, and which tenants have changed since their routes were last
//! sent.

use sqlx::{PgPool, types::Uuid};

/// A tenant whose routes are not up to date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// Tenant id.
    pub id: Uuid,
    /// System name.
    pub username: String,
    /// Its node.
    pub node_id: Uuid,
    /// `provisioning`, `active`, `suspended`, `deleting` or `failed`.
    pub status: String,
    /// The version these routes are for (give it back to [`mark_synced`]).
    pub version: i64,
}

/// A domain, as far as routing is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedDomain {
    /// Id.
    pub id: Uuid,
    /// Name, lower-case ASCII.
    pub fqdn: String,
    /// `primary`, `addon`, `sub` or `alias`.
    pub kind: String,
    /// The domain an alias or subdomain belongs to.
    pub parent_id: Option<Uuid>,
    /// `active` or `suspended`.
    pub status: String,
    /// Visitors on plain HTTP are sent to HTTPS.
    pub force_https: bool,
    /// Answers over HTTPS carry the HSTS header.
    pub hsts: bool,
    /// Where the files are, below the tenant's web directory ("" for the directory itself).
    pub docroot: String,
    /// The PHP version the domain asks for instead of the account's.
    pub php_version: Option<String>,
}

/// A redirect of a domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedRedirect {
    /// The domain.
    pub domain_id: Uuid,
    /// Exact path.
    pub path: String,
    /// Where to.
    pub target: String,
    /// 301, 302, 307 or 308.
    pub code: i16,
}

/// Tenants whose routes have not been sent since something changed, oldest
/// change first.
///
/// # Errors
/// Database errors.
pub async fn pending(pool: &PgPool, limit: i64) -> Result<Vec<Pending>, sqlx::Error> {
    sqlx::query_as!(
        Pending,
        r#"SELECT id, username, node_id, status, routes_version AS version
           FROM tenants
           WHERE routes_version <> routes_synced_version
           ORDER BY updated_at, id
           LIMIT $1"#,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// A tenant's domains, by name.
///
/// # Errors
/// Database errors.
pub async fn domains(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<RoutedDomain>, sqlx::Error> {
    sqlx::query_as!(
        RoutedDomain,
        r#"SELECT id, fqdn, kind, parent_domain_id AS parent_id, status, force_https, hsts, docroot, php_version
           FROM domains WHERE tenant_id = $1 ORDER BY fqdn"#,
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// The redirects of a tenant's domains, by domain and path.
///
/// # Errors
/// Database errors.
pub async fn redirects(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<RoutedRedirect>, sqlx::Error> {
    sqlx::query_as!(
        RoutedRedirect,
        r#"SELECT r.domain_id, r.source_path AS path, r.target, r.code
           FROM redirects r JOIN domains d ON d.id = r.domain_id
           WHERE d.tenant_id = $1 ORDER BY d.fqdn, r.source_path"#,
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// Records that the routes of `version` have been queued. A tenant that changed
/// again since stays pending. Returns whether anything was written.
///
/// # Errors
/// Database errors.
pub async fn mark_synced(pool: &PgPool, id: Uuid, version: i64) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE tenants SET routes_synced_version = $2 WHERE id = $1 AND routes_synced_version <> $2",
        id,
        version,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}
