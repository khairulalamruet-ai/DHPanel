//! The `certificates` table (T05.16).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A domain's certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    /// The domain.
    pub domain_id: Uuid,
    /// `acme` or `custom`.
    pub kind: String,
    /// Who issued it.
    pub issuer: String,
    /// Valid from (Unix seconds).
    pub not_before: i64,
    /// Valid until (Unix seconds).
    pub not_after: i64,
    /// When a node last reported it (Unix seconds).
    pub last_seen_at: i64,
}

/// A certificate that is about to expire and has not been announced yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expiring {
    /// The domain.
    pub domain_id: Uuid,
    /// Its tenant.
    pub tenant_id: Uuid,
    /// The domain's name.
    pub fqdn: String,
    /// Valid until (Unix seconds).
    pub not_after: i64,
}

/// What a node reported for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reported<'a> {
    /// The name.
    pub host: &'a str,
    /// Who issued it.
    pub issuer: &'a str,
    /// Valid from (Unix seconds).
    pub not_before: i64,
    /// Valid until (Unix seconds).
    pub not_after: i64,
}

/// Keeps what a node reported: for each name that is a domain of a tenant on
/// that node, the latest certificate. A custom certificate is never replaced by a
/// report; a renewed one (another end date) is announced again when it nears its
/// end. Returns how many domains were updated.
///
/// # Errors
/// Database errors.
pub async fn record_reported(
    conn: &mut PgConnection,
    node_id: Uuid,
    items: &[Reported<'_>],
) -> Result<usize, sqlx::Error> {
    let mut updated = 0;
    for item in items {
        if item.not_after <= item.not_before {
            continue;
        }
        let done = sqlx::query!(
            r#"INSERT INTO certificates (domain_id, kind, issuer, not_before, not_after, source_node_id)
               SELECT d.id, 'acme', $2, to_timestamp($3::bigint), to_timestamp($4::bigint), $5
               FROM domains d JOIN tenants t ON t.id = d.tenant_id
               WHERE d.fqdn = $1 AND t.node_id = $5
               ON CONFLICT (domain_id) DO UPDATE
               SET issuer = EXCLUDED.issuer,
                   not_before = EXCLUDED.not_before,
                   not_after = EXCLUDED.not_after,
                   source_node_id = EXCLUDED.source_node_id,
                   last_seen_at = now(),
                   expiry_notified_at = CASE
                       WHEN certificates.not_after IS DISTINCT FROM EXCLUDED.not_after THEN NULL
                       ELSE certificates.expiry_notified_at END
               WHERE certificates.kind = 'acme'"#,
            item.host,
            item.issuer,
            item.not_before,
            item.not_after,
            node_id,
        )
        .execute(&mut *conn)
        .await?;
        updated += usize::try_from(done.rows_affected()).unwrap_or(0);
    }
    Ok(updated)
}

/// A domain's certificate, if it has one.
///
/// # Errors
/// Database errors.
pub async fn of_domain(pool: &PgPool, domain_id: Uuid) -> Result<Option<Certificate>, sqlx::Error> {
    sqlx::query_as!(
        Certificate,
        r#"SELECT domain_id, kind, issuer,
                  extract(epoch FROM not_before)::bigint AS "not_before!",
                  extract(epoch FROM not_after)::bigint AS "not_after!",
                  extract(epoch FROM last_seen_at)::bigint AS "last_seen_at!"
           FROM certificates WHERE domain_id = $1"#,
        domain_id,
    )
    .fetch_optional(pool)
    .await
}

/// Certificates that end within `days` days (or already have), whose owners have
/// not been told, soonest first. Domains of tenants that are being deleted or
/// failed are left out: nobody is waiting for those.
///
/// # Errors
/// Database errors.
pub async fn expiring(
    conn: &mut PgConnection,
    days: i32,
    limit: i64,
) -> Result<Vec<Expiring>, sqlx::Error> {
    sqlx::query_as!(
        Expiring,
        r#"SELECT c.domain_id, d.tenant_id, d.fqdn,
                  extract(epoch FROM c.not_after)::bigint AS "not_after!"
           FROM certificates c
           JOIN domains d ON d.id = c.domain_id
           JOIN tenants t ON t.id = d.tenant_id
           WHERE c.expiry_notified_at IS NULL
             AND c.not_after < now() + make_interval(days => $1)
             AND t.status IN ('active', 'suspended')
           ORDER BY c.not_after, c.domain_id
           LIMIT $2
           FOR UPDATE OF c SKIP LOCKED"#,
        days,
        limit,
    )
    .fetch_all(conn)
    .await
}

/// Records that the owners were told about a certificate's end.
///
/// # Errors
/// Database errors.
pub async fn mark_notified(conn: &mut PgConnection, domain_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE certificates SET expiry_notified_at = now() WHERE domain_id = $1",
        domain_id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// What an uploaded certificate is stored as. The key is already sealed.
#[derive(Debug, Clone)]
pub struct NewCustom<'a> {
    /// The domain.
    pub domain_id: Uuid,
    /// Issuer of the first certificate.
    pub issuer: &'a str,
    /// Valid from (Unix seconds).
    pub not_before: i64,
    /// Valid until (Unix seconds).
    pub not_after: i64,
    /// The certificates, PEM.
    pub chain_pem: &'a str,
    /// The private key, sealed.
    pub private_key_enc: &'a [u8],
    /// Who uploaded it.
    pub uploaded_by: Uuid,
}

/// Stores an uploaded certificate for a domain, replacing whatever it had.
///
/// # Errors
/// Database errors.
pub async fn set_custom(conn: &mut PgConnection, new: &NewCustom<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO certificates
             (domain_id, kind, issuer, not_before, not_after, chain_pem, private_key_enc, uploaded_by)
         VALUES ($1, 'custom', $2, to_timestamp($3::bigint), to_timestamp($4::bigint), $5, $6, $7)
         ON CONFLICT (domain_id) DO UPDATE
         SET kind = 'custom', issuer = EXCLUDED.issuer, not_before = EXCLUDED.not_before,
             not_after = EXCLUDED.not_after, chain_pem = EXCLUDED.chain_pem,
             private_key_enc = EXCLUDED.private_key_enc, uploaded_by = EXCLUDED.uploaded_by,
             source_node_id = NULL, last_seen_at = now(), expiry_notified_at = NULL",
        new.domain_id,
        new.issuer,
        new.not_before,
        new.not_after,
        new.chain_pem,
        new.private_key_enc,
        new.uploaded_by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes an uploaded certificate (the web server then gets its own again).
/// Returns whether there was one.
///
/// # Errors
/// Database errors.
pub async fn remove_custom(conn: &mut PgConnection, domain_id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM certificates WHERE domain_id = $1 AND kind = 'custom'",
        domain_id,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// A domain's uploaded certificate with its sealed key, for sending to its node.
#[derive(Debug, Clone)]
pub struct CustomMaterial {
    /// The certificates, PEM.
    pub chain_pem: String,
    /// The private key, sealed.
    pub private_key_enc: Vec<u8>,
}

/// The uploaded certificate of a domain, if it has one.
///
/// # Errors
/// Database errors.
pub async fn custom_material(
    pool: &PgPool,
    domain_id: Uuid,
) -> Result<Option<CustomMaterial>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT chain_pem, private_key_enc FROM certificates WHERE domain_id = $1 AND kind = 'custom'",
        domain_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| {
        Some(CustomMaterial {
            chain_pem: r.chain_pem?,
            private_key_enc: r.private_key_enc?,
        })
    }))
}
