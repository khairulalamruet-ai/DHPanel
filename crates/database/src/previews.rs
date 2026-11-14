//! Temporary preview addresses of sites (T06.14).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A preview link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Id.
    pub id: Uuid,
    /// The domain it shows.
    pub domain_id: Uuid,
    /// The address, `<random>.<preview domain>`.
    pub host: String,
    /// Works until (Unix seconds).
    pub expires_at: i64,
}

/// The links of a domain that have not expired, newest first.
///
/// # Errors
/// Database errors.
pub async fn active(pool: &PgPool, domain_id: Uuid) -> Result<Vec<Preview>, sqlx::Error> {
    sqlx::query_as!(
        Preview,
        r#"SELECT id, domain_id, host, extract(epoch FROM expires_at)::bigint AS "expires_at!"
           FROM preview_links WHERE domain_id = $1 AND expires_at > now()
           ORDER BY created_at DESC"#,
        domain_id,
    )
    .fetch_all(pool)
    .await
}

/// How many links of a tenant have not expired.
///
/// # Errors
/// Database errors.
pub async fn count_active(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM preview_links WHERE tenant_id = $1 AND expires_at > now()"#,
        tenant_id,
    )
    .fetch_one(conn)
    .await
}

/// Adds a link that works for `hours`. Returns it, or `None` when the address
/// is taken (try another random one).
///
/// # Errors
/// Database errors.
pub async fn create(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    domain_id: Uuid,
    host: &str,
    hours: i32,
    by: Option<Uuid>,
) -> Result<Option<Preview>, sqlx::Error> {
    sqlx::query_as!(
        Preview,
        r#"INSERT INTO preview_links (tenant_id, domain_id, host, expires_at, created_by)
           VALUES ($1, $2, $3, now() + make_interval(hours => $4), $5)
           ON CONFLICT (host) DO NOTHING
           RETURNING id, domain_id, host, extract(epoch FROM expires_at)::bigint AS "expires_at!""#,
        tenant_id,
        domain_id,
        host,
        hours,
        by,
    )
    .fetch_optional(conn)
    .await
}

/// Removes a link of a domain. Returns whether there was one.
///
/// # Errors
/// Database errors.
pub async fn delete(
    conn: &mut PgConnection,
    domain_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM preview_links WHERE id = $1 AND domain_id = $2",
        id,
        domain_id,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Removes the links that have expired. Returns how many.
///
/// # Errors
/// Database errors.
pub async fn remove_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!("DELETE FROM preview_links WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}
