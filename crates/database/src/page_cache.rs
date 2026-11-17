//! The `domain_page_cache` table (T12.11): a domain's page cache.
//!
//! Plain queries, so this table needs no offline cache.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A domain's page cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCache {
    /// Whether it is on.
    pub enabled: bool,
    /// Minutes a page stays fresh.
    pub ttl_minutes: i16,
    /// Goes up with every purge.
    pub generation: i32,
}

type Row = (bool, i16, i32);

fn row((enabled, ttl_minutes, generation): Row) -> PageCache {
    PageCache {
        enabled,
        ttl_minutes,
        generation,
    }
}

/// A domain's page cache, if it has ever had one.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, domain_id: Uuid) -> Result<Option<PageCache>, sqlx::Error> {
    Ok(sqlx::query_as::<_, Row>(
        "SELECT enabled, ttl_minutes, generation FROM domain_page_cache WHERE domain_id = $1",
    )
    .bind(domain_id)
    .fetch_optional(pool)
    .await?
    .map(row))
}

/// When the cache was last purged (seconds since 1970).
///
/// # Errors
/// Database errors.
pub async fn purged_at(pool: &PgPool, domain_id: Uuid) -> Result<Option<i64>, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, Option<i64>>(
        "SELECT EXTRACT(EPOCH FROM purged_at)::bigint FROM domain_page_cache WHERE domain_id = $1",
    )
    .bind(domain_id)
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// Turns the cache on or off and sets its time; the generation is kept.
///
/// # Errors
/// Database errors.
pub async fn set(
    conn: &mut PgConnection,
    domain_id: Uuid,
    enabled: bool,
    ttl_minutes: i16,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO domain_page_cache (domain_id, enabled, ttl_minutes) VALUES ($1, $2, $3)
         ON CONFLICT (domain_id) DO UPDATE
         SET enabled = EXCLUDED.enabled, ttl_minutes = EXCLUDED.ttl_minutes",
    )
    .bind(domain_id)
    .bind(enabled)
    .bind(ttl_minutes)
    .execute(conn)
    .await?;
    Ok(())
}

/// Purges: the generation goes up. `false` when the domain has no cache row.
///
/// # Errors
/// Database errors.
pub async fn purge(conn: &mut PgConnection, domain_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE domain_page_cache SET generation = generation + 1, purged_at = now()
         WHERE domain_id = $1",
    )
    .bind(domain_id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The caches of a tenant's domains that are on, by domain.
///
/// # Errors
/// Database errors.
pub async fn of_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<(Uuid, PageCache)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid, bool, i16, i32)>(
        "SELECT c.domain_id, c.enabled, c.ttl_minutes, c.generation
         FROM domain_page_cache c JOIN domains d ON d.id = c.domain_id
         WHERE d.tenant_id = $1 AND c.enabled",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, e, t, g)| {
            (
                id,
                PageCache {
                    enabled: e,
                    ttl_minutes: t,
                    generation: g,
                },
            )
        })
        .collect())
}
