//! What a domain's web server does besides serving files: a rate limit and extra
//! response headers (T06.05).

use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, types::Uuid};

/// One response header, as kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredHeader {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
}

/// A domain's policy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DomainPolicy {
    /// Requests per second one visitor may make, if limited.
    pub rate_limit_per_second: Option<u16>,
    /// Headers to add.
    pub headers: Vec<StoredHeader>,
}

fn decode(rate: Option<i16>, headers: serde_json::Value) -> DomainPolicy {
    DomainPolicy {
        rate_limit_per_second: rate.and_then(|r| u16::try_from(r).ok()),
        headers: serde_json::from_value(headers).unwrap_or_default(),
    }
}

/// A domain's policy (the empty one when nothing was set).
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, domain_id: Uuid) -> Result<DomainPolicy, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT rate_limit_per_second, headers FROM domain_policies WHERE domain_id = $1",
        domain_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or_else(DomainPolicy::default, |r| {
        decode(r.rate_limit_per_second, r.headers)
    }))
}

/// Sets a domain's policy; an empty one removes the row.
///
/// # Errors
/// Database errors.
pub async fn set(
    conn: &mut PgConnection,
    domain_id: Uuid,
    policy: &DomainPolicy,
) -> Result<(), sqlx::Error> {
    if policy.rate_limit_per_second.is_none() && policy.headers.is_empty() {
        sqlx::query!(
            "DELETE FROM domain_policies WHERE domain_id = $1",
            domain_id
        )
        .execute(conn)
        .await?;
        return Ok(());
    }
    let headers = serde_json::to_value(&policy.headers).unwrap_or_default();
    sqlx::query!(
        "INSERT INTO domain_policies (domain_id, rate_limit_per_second, headers)
         VALUES ($1, $2, $3)
         ON CONFLICT (domain_id) DO UPDATE
         SET rate_limit_per_second = EXCLUDED.rate_limit_per_second, headers = EXCLUDED.headers",
        domain_id,
        policy
            .rate_limit_per_second
            .map(i16::try_from)
            .transpose()
            .unwrap_or(None),
        headers,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The policies of a tenant's domains, by domain.
///
/// # Errors
/// Database errors.
pub async fn of_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<(Uuid, DomainPolicy)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT p.domain_id, p.rate_limit_per_second, p.headers
         FROM domain_policies p JOIN domains d ON d.id = p.domain_id
         WHERE d.tenant_id = $1",
        tenant_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.domain_id, decode(r.rate_limit_per_second, r.headers)))
        .collect())
}
