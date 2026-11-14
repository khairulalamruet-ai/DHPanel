//! The `redirects` table (T05.04).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A redirect row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// Id.
    pub id: Uuid,
    /// The domain it belongs to.
    pub domain_id: Uuid,
    /// The path it applies to (exact match), starting with `/`.
    pub source_path: String,
    /// Where it sends the visitor: an http(s) address or a path on the same site.
    pub target: String,
    /// 301, 302, 307 or 308.
    pub code: i16,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

/// Why adding a redirect failed.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// The domain already redirects this path.
    #[error("this path already has a redirect")]
    Duplicate,
    /// The domain does not exist.
    #[error("unknown domain")]
    UnknownDomain,
    /// Any other database error (a check violation means the input skipped validation).
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The redirects of a domain, by path.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, domain_id: Uuid) -> Result<Vec<Redirect>, sqlx::Error> {
    sqlx::query_as!(
        Redirect,
        r#"SELECT id, domain_id, source_path, target, code,
                  extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM redirects WHERE domain_id = $1 ORDER BY source_path"#,
        domain_id,
    )
    .fetch_all(pool)
    .await
}

/// One redirect of a domain.
///
/// # Errors
/// Database errors.
pub async fn get(
    pool: &PgPool,
    domain_id: Uuid,
    id: Uuid,
) -> Result<Option<Redirect>, sqlx::Error> {
    sqlx::query_as!(
        Redirect,
        r#"SELECT id, domain_id, source_path, target, code,
                  extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM redirects WHERE domain_id = $1 AND id = $2"#,
        domain_id,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// How many redirects a domain has.
///
/// # Errors
/// Database errors.
pub async fn count(conn: &mut PgConnection, domain_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM redirects WHERE domain_id = $1"#,
        domain_id,
    )
    .fetch_one(conn)
    .await
}

/// Adds a redirect.
///
/// # Errors
/// [`AddError`].
pub async fn add(
    conn: &mut PgConnection,
    domain_id: Uuid,
    source_path: &str,
    target: &str,
    code: i16,
) -> Result<Redirect, AddError> {
    sqlx::query_as!(
        Redirect,
        r#"INSERT INTO redirects (domain_id, source_path, target, code)
           VALUES ($1, $2, $3, $4)
           RETURNING id, domain_id, source_path, target, code,
                     extract(epoch FROM created_at)::bigint AS "created_at!""#,
        domain_id,
        source_path,
        target,
        code,
    )
    .fetch_one(conn)
    .await
    .map_err(
        |err| match err.as_database_error().and_then(|e| e.constraint()) {
            Some("redirects_domain_source_key") => AddError::Duplicate,
            Some("redirects_domain_id_fkey") => AddError::UnknownDomain,
            _ => AddError::Database(err),
        },
    )
}

/// Changes a redirect's target and/or code. Returns whether it exists.
///
/// # Errors
/// Database errors.
pub async fn update(
    conn: &mut PgConnection,
    domain_id: Uuid,
    id: Uuid,
    target: Option<&str>,
    code: Option<i16>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE redirects SET target = COALESCE($3, target), code = COALESCE($4, code)
         WHERE domain_id = $1 AND id = $2",
        domain_id,
        id,
        target,
        code,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Removes a redirect. Returns whether it existed.
///
/// # Errors
/// Database errors.
pub async fn delete(
    conn: &mut PgConnection,
    domain_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM redirects WHERE domain_id = $1 AND id = $2",
        domain_id,
        id,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The target of the redirect that applies to `path` on the domain named
/// `fqdn`, anywhere on the panel (that is where a chain of redirects leads).
///
/// # Errors
/// Database errors.
pub async fn target_for(
    conn: &mut PgConnection,
    fqdn: &str,
    path: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT r.target FROM redirects r JOIN domains d ON d.id = r.domain_id
         WHERE d.fqdn = $1 AND r.source_path = $2",
        fqdn,
        path,
    )
    .fetch_optional(conn)
    .await
}
