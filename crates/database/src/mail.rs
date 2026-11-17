//! Email (T13.03): mail domains, mailboxes, forwarders and automatic replies, with the version
//! counters the mail tick uses to tell what the node's mail server still has to be told. Plain
//! queries, so these tables need no offline cache.

use serde_json::Value;
use sqlx::{FromRow, PgPool, types::Uuid};

/// A domain that has mail.
#[derive(Debug, Clone, FromRow)]
pub struct MailDomain {
    /// Id.
    pub id: Uuid,
    /// The hosting account (none once it is deleted: the domain is being removed).
    pub tenant_id: Option<Uuid>,
    /// The domain (of the `domains` table; none once it is deleted).
    pub domain_id: Option<Uuid>,
    /// Its name.
    pub fqdn: String,
    /// The node whose mail server holds it.
    pub node_id: Uuid,
    /// Raised by every change of its settings.
    pub version: i64,
    /// The version the node has.
    pub applied_version: i64,
    /// Raised by every change of its forwarders.
    pub fwd_version: i64,
    /// The forwarders' version the node has.
    pub fwd_applied: i64,
    /// `pending`, `active`, `error` or `removing`.
    pub state: String,
    /// Why the node refused the last change.
    pub last_error: Option<String>,
    /// Most messages an hour.
    pub per_hour: Option<i32>,
    /// Outgoing mail is held (an abuse stop).
    pub held: bool,
    /// Why it is held.
    pub held_reason: Option<String>,
    /// What the mail server said the zone needs.
    pub records: Value,
    /// Whether those records are in the zone.
    pub dns_applied: bool,
    /// Seconds since the row last changed.
    pub age_secs: i64,
}

const DOMAIN: &str = "id, tenant_id, domain_id, fqdn, node_id, version, applied_version, \
    fwd_version, fwd_applied, state, last_error, per_hour, held, held_reason, records, \
    dns_applied, extract(epoch FROM now() - updated_at)::bigint AS age_secs";

/// A mailbox.
#[derive(Debug, Clone, FromRow)]
pub struct Mailbox {
    /// Id.
    pub id: Uuid,
    /// The hosting account.
    pub tenant_id: Uuid,
    /// Its mail domain.
    pub mail_domain_id: Uuid,
    /// The part before the `@`.
    pub local_part: String,
    /// The domain's name.
    pub fqdn: String,
    /// Quota in megabytes.
    pub quota_mb: i32,
    /// Bytes stored.
    pub used_bytes: i64,
    /// `pending`, `active`, `error` or `removing`.
    pub state: String,
    /// Why the node refused the last change.
    pub last_error: Option<String>,
    /// Raised by every change.
    pub version: i64,
    /// The version the node has.
    pub applied_version: i64,
    /// Subject of the automatic reply (none: there is no reply).
    pub ar_subject: Option<String>,
    /// Text of the automatic reply.
    pub ar_body: Option<String>,
    /// The reply starts then (Unix seconds).
    pub ar_from: Option<i64>,
    /// The reply ends then (Unix seconds).
    pub ar_until: Option<i64>,
    /// Raised by every change of the reply.
    pub ar_version: i64,
    /// The reply's version the node has.
    pub ar_applied: i64,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Seconds since the row last changed.
    pub age_secs: i64,
}

const MAILBOX: &str = "m.id, m.tenant_id, m.mail_domain_id, m.local_part, d.fqdn, m.quota_mb, \
    m.used_bytes, m.state, m.last_error, m.version, m.applied_version, m.ar_subject, m.ar_body, \
    extract(epoch FROM m.ar_from)::bigint AS ar_from, \
    extract(epoch FROM m.ar_until)::bigint AS ar_until, m.ar_version, m.ar_applied, \
    extract(epoch FROM m.created_at)::bigint AS created_at, \
    extract(epoch FROM now() - m.updated_at)::bigint AS age_secs";

/// An address that sends mail on.
#[derive(Debug, Clone, FromRow)]
pub struct Forwarder {
    /// Id.
    pub id: Uuid,
    /// Its mail domain.
    pub mail_domain_id: Uuid,
    /// The name, or `*` for the catch-all.
    pub local_part: String,
    /// The domain's name.
    pub fqdn: String,
    /// Where it goes.
    pub destination: String,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

const FORWARDER: &str = "f.id, f.mail_domain_id, f.local_part, d.fqdn, f.destination, \
    extract(epoch FROM f.created_at)::bigint AS created_at";

/// Why a row could not be made.
#[derive(Debug, thiserror::Error)]
pub enum MakeError {
    /// It exists.
    #[error("it exists already")]
    Taken,
    /// The database.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn unique(err: sqlx::Error) -> MakeError {
    match err.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("23505") => MakeError::Taken,
        _ => MakeError::Database(err),
    }
}

/// The mail domain of a domain, made when there is none.
///
/// # Errors
/// Database errors.
pub async fn ensure_domain(
    pool: &PgPool,
    tenant_id: Uuid,
    domain_id: Uuid,
    fqdn: &str,
    node_id: Uuid,
    per_hour: Option<i32>,
) -> Result<MailDomain, sqlx::Error> {
    sqlx::query(
        "INSERT INTO mail_domains (tenant_id, domain_id, fqdn, node_id, per_hour)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (domain_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(domain_id)
    .bind(fqdn)
    .bind(node_id)
    .bind(per_hour)
    .execute(pool)
    .await?;
    sqlx::query_as::<_, MailDomain>(sqlx::AssertSqlSafe(format!(
        "SELECT {DOMAIN} FROM mail_domains WHERE domain_id = $1"
    )))
    .bind(domain_id)
    .fetch_one(pool)
    .await
}

/// The mail domain with this name of a tenant.
///
/// # Errors
/// Database errors.
pub async fn domain_of(
    pool: &PgPool,
    tenant_id: Uuid,
    fqdn: &str,
) -> Result<Option<MailDomain>, sqlx::Error> {
    sqlx::query_as::<_, MailDomain>(sqlx::AssertSqlSafe(format!(
        "SELECT {DOMAIN} FROM mail_domains WHERE tenant_id = $1 AND fqdn = $2"
    )))
    .bind(tenant_id)
    .bind(fqdn)
    .fetch_optional(pool)
    .await
}

/// One mail domain.
///
/// # Errors
/// Database errors.
pub async fn domain(pool: &PgPool, id: Uuid) -> Result<Option<MailDomain>, sqlx::Error> {
    sqlx::query_as::<_, MailDomain>(sqlx::AssertSqlSafe(format!(
        "SELECT {DOMAIN} FROM mail_domains WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// A tenant's mail domains.
///
/// # Errors
/// Database errors.
pub async fn domains_of_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<MailDomain>, sqlx::Error> {
    sqlx::query_as::<_, MailDomain>(sqlx::AssertSqlSafe(format!(
        "SELECT {DOMAIN} FROM mail_domains WHERE tenant_id = $1 ORDER BY fqdn"
    )))
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

/// Every mail domain (for the tick and the admin's pages).
///
/// # Errors
/// Database errors.
pub async fn all_domains(pool: &PgPool) -> Result<Vec<MailDomain>, sqlx::Error> {
    sqlx::query_as::<_, MailDomain>(sqlx::AssertSqlSafe(format!(
        "SELECT {DOMAIN} FROM mail_domains ORDER BY fqdn"
    )))
    .fetch_all(pool)
    .await
}

/// Says the domain's settings changed.
///
/// # Errors
/// Database errors.
pub async fn bump_domain(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE mail_domains SET version = version + 1,
                state = CASE WHEN state = 'removing' THEN state ELSE 'pending' END
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Says the domain's forwarders changed.
///
/// # Errors
/// Database errors.
pub async fn bump_forwarders(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mail_domains SET fwd_version = fwd_version + 1 WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets the plan's outgoing limit; a change raises the version.
///
/// # Errors
/// Database errors.
pub async fn set_per_hour(
    pool: &PgPool,
    id: Uuid,
    per_hour: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE mail_domains SET per_hour = $2, version = version + 1
         WHERE id = $1 AND per_hour IS DISTINCT FROM $2",
    )
    .bind(id)
    .bind(per_hour)
    .execute(pool)
    .await?;
    Ok(())
}

/// Holds or releases a domain's outgoing mail.
///
/// # Errors
/// Database errors.
pub async fn set_held(
    pool: &PgPool,
    id: Uuid,
    held: bool,
    reason: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE mail_domains
            SET held = $2, held_reason = $3,
                held_at = CASE WHEN $2 THEN now() ELSE NULL END,
                version = version + 1
          WHERE id = $1 AND held IS DISTINCT FROM $2",
    )
    .bind(id)
    .bind(held)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The node has the domain's settings of this version, with these DNS records.
///
/// # Errors
/// Database errors.
pub async fn domain_applied(
    pool: &PgPool,
    id: Uuid,
    version: i64,
    records: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE mail_domains
            SET applied_version = GREATEST(applied_version, $2), last_error = NULL,
                state = CASE WHEN version <= $2 AND state <> 'removing' THEN 'active' ELSE state END,
                -- An answer without records leaves the ones known.
                dns_applied = CASE WHEN jsonb_array_length($3) > 0 AND records IS DISTINCT FROM $3
                                   THEN false ELSE dns_applied END,
                records = CASE WHEN jsonb_array_length($3) > 0 THEN $3 ELSE records END
          WHERE id = $1",
    )
    .bind(id)
    .bind(version)
    .bind(records)
    .execute(pool)
    .await?;
    Ok(())
}

/// The domain's forwarders of this version are on the node.
///
/// # Errors
/// Database errors.
pub async fn forwarders_applied(pool: &PgPool, id: Uuid, version: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mail_domains SET fwd_applied = GREATEST(fwd_applied, $2) WHERE id = $1")
        .bind(id)
        .bind(version)
        .execute(pool)
        .await?;
    Ok(())
}

/// The domain's DNS records are in its zone.
///
/// # Errors
/// Database errors.
pub async fn dns_done(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mail_domains SET dns_applied = true WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The node refused something.
///
/// # Errors
/// Database errors.
pub async fn domain_failed(pool: &PgPool, id: Uuid, why: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mail_domains SET state = 'error', last_error = $2 WHERE id = $1")
        .bind(id)
        .bind(why)
        .execute(pool)
        .await?;
    Ok(())
}

/// Starts the removal of a mail domain (its mailboxes go with it on the node).
///
/// # Errors
/// Database errors.
pub async fn domain_removing(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mail_domains SET state = 'removing', version = version + 1 WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes a mail domain's rows (the node has let go of it).
///
/// # Errors
/// Database errors.
pub async fn delete_domain(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM mail_domains WHERE id = $1 AND state = 'removing'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Lets a domain that was in error try again.
///
/// # Errors
/// Database errors.
pub async fn retry_stale(pool: &PgPool, older_than_secs: i32) -> Result<u64, sqlx::Error> {
    let a = sqlx::query(
        "UPDATE mail_domains SET version = version + 1, fwd_version = fwd_version + 1,
                state = 'pending'
          WHERE state = 'error' AND updated_at < now() - make_interval(secs => $1::int)",
    )
    .bind(older_than_secs)
    .execute(pool)
    .await?;
    let b = sqlx::query(
        "UPDATE mailboxes SET version = version + 1, ar_version = ar_version + 1,
                state = 'pending'
          WHERE state = 'error' AND updated_at < now() - make_interval(secs => $1::int)",
    )
    .bind(older_than_secs)
    .execute(pool)
    .await?;
    Ok(a.rows_affected() + b.rows_affected())
}

// ------------------------------------------------------------------------------ mailboxes

/// Makes a mailbox.
///
/// # Errors
/// [`MakeError::Taken`] when the address exists.
pub async fn create_mailbox(
    pool: &PgPool,
    id: Uuid,
    tenant_id: Uuid,
    mail_domain_id: Uuid,
    local: &str,
    quota_mb: i32,
    sealed_password: &[u8],
) -> Result<(), MakeError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO mailboxes (id, tenant_id, mail_domain_id, local_part, quota_mb)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(mail_domain_id)
    .bind(local)
    .bind(quota_mb)
    .execute(&mut *tx)
    .await
    .map_err(unique)?;
    sqlx::query("INSERT INTO mailbox_passwords (mailbox_id, sealed) VALUES ($1, $2)")
        .bind(id)
        .bind(sealed_password)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// One mailbox of a tenant.
///
/// # Errors
/// Database errors.
pub async fn mailbox(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<Mailbox>, sqlx::Error> {
    sqlx::query_as::<_, Mailbox>(sqlx::AssertSqlSafe(format!(
        "SELECT {MAILBOX} FROM mailboxes m JOIN mail_domains d ON d.id = m.mail_domain_id
         WHERE m.tenant_id = $1 AND m.id = $2"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// A tenant's mailboxes (those being removed are left out), of one domain or all.
///
/// # Errors
/// Database errors.
pub async fn mailboxes_of_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    fqdn: Option<&str>,
) -> Result<Vec<Mailbox>, sqlx::Error> {
    sqlx::query_as::<_, Mailbox>(sqlx::AssertSqlSafe(format!(
        "SELECT {MAILBOX} FROM mailboxes m JOIN mail_domains d ON d.id = m.mail_domain_id
         WHERE m.tenant_id = $1 AND m.state <> 'removing' AND ($2::text IS NULL OR d.fqdn = $2)
         ORDER BY d.fqdn, m.local_part"
    )))
    .bind(tenant_id)
    .bind(fqdn)
    .fetch_all(pool)
    .await
}

/// The mailboxes of one mail domain, being removed or not.
///
/// # Errors
/// Database errors.
pub async fn mailboxes_of_domain(
    pool: &PgPool,
    mail_domain_id: Uuid,
) -> Result<Vec<Mailbox>, sqlx::Error> {
    sqlx::query_as::<_, Mailbox>(sqlx::AssertSqlSafe(format!(
        "SELECT {MAILBOX} FROM mailboxes m JOIN mail_domains d ON d.id = m.mail_domain_id
         WHERE m.mail_domain_id = $1 ORDER BY m.local_part"
    )))
    .bind(mail_domain_id)
    .fetch_all(pool)
    .await
}

/// How many mailboxes a tenant has.
///
/// # Errors
/// Database errors.
pub async fn count_mailboxes(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM mailboxes WHERE tenant_id = $1 AND state <> 'removing'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

/// Whether a local part is a mailbox of the domain.
///
/// # Errors
/// Database errors.
pub async fn mailbox_exists(
    pool: &PgPool,
    mail_domain_id: Uuid,
    local: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM mailboxes
                         WHERE mail_domain_id = $1 AND local_part = $2 AND state <> 'removing')",
    )
    .bind(mail_domain_id)
    .bind(local)
    .fetch_one(pool)
    .await
}

/// Changes a mailbox's password (sealed).
///
/// # Errors
/// Database errors.
pub async fn set_password(pool: &PgPool, id: Uuid, sealed: &[u8]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO mailbox_passwords (mailbox_id, sealed) VALUES ($1, $2)
         ON CONFLICT (mailbox_id) DO UPDATE SET sealed = EXCLUDED.sealed, created_at = now()",
    )
    .bind(id)
    .bind(sealed)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE mailboxes SET version = version + 1, state = 'pending' WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// The sealed password waiting for the mailbox `local@fqdn`, with its id (what it was sealed for).
///
/// # Errors
/// Database errors.
pub async fn sealed_password_of(
    pool: &PgPool,
    fqdn: &str,
    local: &str,
) -> Result<Option<(Uuid, Vec<u8>)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT p.mailbox_id, p.sealed FROM mailbox_passwords p
         JOIN mailboxes m ON m.id = p.mailbox_id
         JOIN mail_domains d ON d.id = m.mail_domain_id
         WHERE d.fqdn = $1 AND m.local_part = $2",
    )
    .bind(fqdn)
    .bind(local)
    .fetch_optional(pool)
    .await
}

/// What a sealed mailbox password is sealed to: that mailbox and nothing else.
#[must_use]
pub fn password_context(mailbox_id: Uuid) -> Vec<u8> {
    format!("mailbox-password:{mailbox_id}").into_bytes()
}

/// The node has the mailbox as of this version; its password is not needed any more.
///
/// # Errors
/// Database errors.
pub async fn mailbox_applied(pool: &PgPool, id: Uuid, version: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE mailboxes
            SET applied_version = GREATEST(applied_version, $2), last_error = NULL,
                state = CASE WHEN version <= $2 AND state <> 'removing' THEN 'active' ELSE state END
          WHERE id = $1",
    )
    .bind(id)
    .bind(version)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM mailbox_passwords p USING mailboxes m
          WHERE p.mailbox_id = m.id AND m.id = $1 AND m.applied_version >= m.version",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// The node refused something about a mailbox.
///
/// # Errors
/// Database errors.
pub async fn mailbox_failed(pool: &PgPool, id: Uuid, why: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mailboxes SET state = 'error', last_error = $2 WHERE id = $1")
        .bind(id)
        .bind(why)
        .execute(pool)
        .await?;
    Ok(())
}

/// Starts the removal of a mailbox.
///
/// # Errors
/// Database errors.
pub async fn mailbox_removing(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mailboxes SET state = 'removing', version = version + 1 WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes a mailbox's row (the node has let go of it).
///
/// # Errors
/// Database errors.
pub async fn delete_mailbox(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM mailboxes WHERE id = $1 AND state = 'removing'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets, changes or clears (`None`) the automatic reply of a mailbox.
///
/// # Errors
/// Database errors.
pub async fn set_autoresponder(
    pool: &PgPool,
    id: Uuid,
    reply: Option<(&str, &str, Option<i64>, Option<i64>)>,
) -> Result<(), sqlx::Error> {
    let (subject, body, from, until) = match reply {
        Some((s, b, f, u)) => (Some(s), Some(b), f, u),
        None => (None, None, None, None),
    };
    sqlx::query(
        "UPDATE mailboxes
            SET ar_subject = $2, ar_body = $3,
                ar_from = to_timestamp($4::bigint), ar_until = to_timestamp($5::bigint),
                ar_version = ar_version + 1
          WHERE id = $1",
    )
    .bind(id)
    .bind(subject)
    .bind(body)
    .bind(from)
    .bind(until)
    .execute(pool)
    .await?;
    Ok(())
}

/// The node has the mailbox's automatic reply of this version.
///
/// # Errors
/// Database errors.
pub async fn autoresponder_applied(
    pool: &PgPool,
    id: Uuid,
    version: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE mailboxes SET ar_applied = GREATEST(ar_applied, $2) WHERE id = $1")
        .bind(id)
        .bind(version)
        .execute(pool)
        .await?;
    Ok(())
}

/// Keeps what the mail server says each mailbox holds.
///
/// # Errors
/// Database errors.
pub async fn set_usage(
    pool: &PgPool,
    node_id: Uuid,
    usage: &[(String, i64)],
) -> Result<(), sqlx::Error> {
    for (address, bytes) in usage {
        let Some((local, fqdn)) = address.rsplit_once('@') else {
            continue;
        };
        sqlx::query(
            "UPDATE mailboxes m SET used_bytes = $4
               FROM mail_domains d
              WHERE d.id = m.mail_domain_id AND d.node_id = $1 AND d.fqdn = $3
                AND m.local_part = $2 AND m.used_bytes <> $4",
        )
        .bind(node_id)
        .bind(local)
        .bind(fqdn)
        .bind(bytes)
        .execute(pool)
        .await?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------- forwarders

/// A domain's forwarders.
///
/// # Errors
/// Database errors.
pub async fn forwarders_of_domain(
    pool: &PgPool,
    mail_domain_id: Uuid,
) -> Result<Vec<Forwarder>, sqlx::Error> {
    sqlx::query_as::<_, Forwarder>(sqlx::AssertSqlSafe(format!(
        "SELECT {FORWARDER} FROM mail_forwarders f JOIN mail_domains d ON d.id = f.mail_domain_id
         WHERE f.mail_domain_id = $1 ORDER BY f.local_part, f.destination"
    )))
    .bind(mail_domain_id)
    .fetch_all(pool)
    .await
}

/// A tenant's forwarders, of one domain or all.
///
/// # Errors
/// Database errors.
pub async fn forwarders_of_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    fqdn: Option<&str>,
) -> Result<Vec<Forwarder>, sqlx::Error> {
    sqlx::query_as::<_, Forwarder>(sqlx::AssertSqlSafe(format!(
        "SELECT {FORWARDER} FROM mail_forwarders f JOIN mail_domains d ON d.id = f.mail_domain_id
         WHERE d.tenant_id = $1 AND ($2::text IS NULL OR d.fqdn = $2)
         ORDER BY d.fqdn, f.local_part, f.destination"
    )))
    .bind(tenant_id)
    .bind(fqdn)
    .fetch_all(pool)
    .await
}

/// Adds a forwarder.
///
/// # Errors
/// [`MakeError::Taken`] when it exists.
pub async fn add_forwarder(
    pool: &PgPool,
    mail_domain_id: Uuid,
    local: &str,
    destination: &str,
) -> Result<Uuid, MakeError> {
    sqlx::query_scalar(
        "INSERT INTO mail_forwarders (mail_domain_id, local_part, destination)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(mail_domain_id)
    .bind(local)
    .bind(destination)
    .fetch_one(pool)
    .await
    .map_err(unique)
}

/// How many forwarders a domain has.
///
/// # Errors
/// Database errors.
pub async fn count_forwarders(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM mail_forwarders f JOIN mail_domains d ON d.id = f.mail_domain_id
         WHERE d.tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

/// One forwarder of a tenant.
///
/// # Errors
/// Database errors.
pub async fn forwarder(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<Forwarder>, sqlx::Error> {
    sqlx::query_as::<_, Forwarder>(sqlx::AssertSqlSafe(format!(
        "SELECT {FORWARDER} FROM mail_forwarders f JOIN mail_domains d ON d.id = f.mail_domain_id
         WHERE d.tenant_id = $1 AND f.id = $2"
    )))
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Removes a forwarder.
///
/// # Errors
/// Database errors.
pub async fn delete_forwarder(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM mail_forwarders WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// --------------------------------------------------------------------- servers and queues

/// A node's mail server row: the name wanted and the name the node has.
#[derive(Debug, Clone, FromRow)]
pub struct Server {
    /// The node.
    pub node_id: Uuid,
    /// The name it should say it is.
    pub hostname: String,
    /// The name it has been told.
    pub applied_hostname: Option<String>,
    /// Why the node refused.
    pub last_error: Option<String>,
}

/// Every node's mail server row.
///
/// # Errors
/// Database errors.
pub async fn servers(pool: &PgPool) -> Result<Vec<Server>, sqlx::Error> {
    sqlx::query_as("SELECT node_id, hostname, applied_hostname, last_error FROM mail_servers")
        .fetch_all(pool)
        .await
}

/// A node's mail server row.
///
/// # Errors
/// Database errors.
pub async fn server(pool: &PgPool, node_id: Uuid) -> Result<Option<Server>, sqlx::Error> {
    sqlx::query_as(
        "SELECT node_id, hostname, applied_hostname, last_error FROM mail_servers WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await
}

/// Says what a node's mail server should call itself.
///
/// # Errors
/// Database errors.
pub async fn want_hostname(
    pool: &PgPool,
    node_id: Uuid,
    hostname: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO mail_servers (node_id, hostname) VALUES ($1, $2)
         ON CONFLICT (node_id) DO UPDATE SET hostname = EXCLUDED.hostname",
    )
    .bind(node_id)
    .bind(hostname)
    .execute(pool)
    .await?;
    Ok(())
}

/// The node's mail server has this name now (or refused it).
///
/// # Errors
/// Database errors.
pub async fn server_applied(
    pool: &PgPool,
    node_id: Uuid,
    hostname: &str,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE mail_servers
            SET applied_hostname = CASE WHEN $3::text IS NULL THEN $2 ELSE applied_hostname END,
                last_error = $3
          WHERE node_id = $1",
    )
    .bind(node_id)
    .bind(hostname)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Keeps a node's last queue report.
///
/// # Errors
/// Database errors.
pub async fn save_queue(
    pool: &PgPool,
    node_id: Uuid,
    items: &Value,
    total: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO mail_queue_reports (node_id, items, total) VALUES ($1, $2, $3)
         ON CONFLICT (node_id) DO UPDATE
            SET items = EXCLUDED.items, total = EXCLUDED.total, reported_at = now()",
    )
    .bind(node_id)
    .bind(items)
    .bind(total)
    .execute(pool)
    .await?;
    Ok(())
}

/// The last queue report of every node: the node, its items, how many it holds in all.
///
/// # Errors
/// Database errors.
pub async fn queues(pool: &PgPool) -> Result<Vec<(Uuid, Value, i64)>, sqlx::Error> {
    sqlx::query_as("SELECT node_id, items, total FROM mail_queue_reports ORDER BY node_id")
        .fetch_all(pool)
        .await
}

/// Whether a node has a mail domain that is set up (so its mail server is worth asking).
///
/// # Errors
/// Database errors.
pub async fn node_has_mail(pool: &PgPool, node_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM mail_domains
                         WHERE node_id = $1 AND applied_version > 0 AND state <> 'removing')",
    )
    .bind(node_id)
    .fetch_one(pool)
    .await
}
