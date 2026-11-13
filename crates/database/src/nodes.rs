//! Nodes and their certificates (`nodes`, `node_certificates`).

use std::net::IpAddr;

use sqlx::{PgConnection, types::Uuid};

/// A node to create at enrollment.
#[derive(Debug, Clone)]
pub struct NewNode<'a> {
    /// Chosen by the caller, because the node's certificate names it.
    pub id: Uuid,
    /// Unique node name (from the enrollment token).
    pub name: &'a str,
    /// Host name the agent reported.
    pub hostname: Option<&'a str>,
    /// Address the agent connected from, as seen by the control plane.
    pub public_ip: Option<IpAddr>,
    /// Agent build version.
    pub agent_version: Option<&'a str>,
}

/// Why [`insert_node`] failed.
#[derive(Debug, thiserror::Error)]
pub enum InsertNodeError {
    /// Another node already has this name.
    #[error("a node with this name already exists")]
    NameTaken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Creates a node (status `offline` until its agent connects).
///
/// # Errors
/// [`InsertNodeError::NameTaken`] or database errors.
pub async fn insert_node(
    conn: &mut PgConnection,
    node: &NewNode<'_>,
) -> Result<(), InsertNodeError> {
    sqlx::query!(
        r#"INSERT INTO nodes (id, name, hostname, public_ip, agent_version)
           VALUES ($1, $2, $3, $4::text::inet, $5)"#,
        node.id,
        node.name,
        node.hostname,
        node.public_ip.map(|ip| ip.to_string()),
        node.agent_version,
    )
    .execute(conn)
    .await
    .map_err(
        |err| match err.as_database_error().and_then(|e| e.constraint()) {
            Some("nodes_name_key") => InsertNodeError::NameTaken,
            _ => InsertNodeError::Database(err),
        },
    )?;
    Ok(())
}

/// A certificate the CA issued to a node.
#[derive(Debug, Clone)]
pub struct NewNodeCertificate<'a> {
    /// The node it belongs to.
    pub node_id: Uuid,
    /// 32 lowercase hex digits.
    pub serial: &'a str,
    /// SHA-256 of the DER certificate, 64 lowercase hex digits.
    pub fingerprint_sha256: &'a str,
    /// Start of validity (Unix seconds).
    pub not_before: i64,
    /// End of validity (Unix seconds).
    pub not_after: i64,
}

/// Records an issued certificate.
///
/// # Errors
/// Database errors (including unique violations on serial or fingerprint).
pub async fn insert_node_certificate(
    conn: &mut PgConnection,
    cert: &NewNodeCertificate<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO node_certificates (node_id, serial, fingerprint_sha256, not_before, not_after)
           VALUES ($1, $2, $3, to_timestamp($4::bigint), to_timestamp($5::bigint))"#,
        cert.node_id,
        cert.serial,
        cert.fingerprint_sha256,
        cert.not_before,
        cert.not_after,
    )
    .execute(conn)
    .await?;
    Ok(())
}

// ------------------------------------------------------------------ sessions

/// A node whose certificate may open a session.
#[derive(Debug, Clone)]
pub struct SessionNode {
    /// Node id.
    pub id: Uuid,
    /// Node name.
    pub name: String,
}

/// Finds the node a presented certificate belongs to, if the certificate may
/// still be used: known to the CA, not revoked, inside its validity window, and
/// the node itself not revoked. The fingerprint is the SHA-256 of the DER
/// certificate (64 lowercase hex digits).
///
/// # Errors
/// Database errors.
pub async fn node_for_certificate(
    pool: &sqlx::PgPool,
    fingerprint_sha256: &str,
) -> Result<Option<SessionNode>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT n.id, n.name
           FROM node_certificates c
           JOIN nodes n ON n.id = c.node_id
           WHERE c.fingerprint_sha256 = $1
             AND c.revoked_at IS NULL
             AND c.not_before <= now()
             AND c.not_after > now()
             AND n.status <> 'revoked'"#,
        fingerprint_sha256,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| SessionNode {
        id: r.id,
        name: r.name,
    }))
}

/// Records that the node's agent connected. A draining node stays draining.
/// `ip` is the address the connection came from, as text.
///
/// # Errors
/// Database errors.
pub async fn mark_online(
    pool: &sqlx::PgPool,
    id: Uuid,
    agent_version: &str,
    ip: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE nodes
           SET status = CASE WHEN status = 'draining' THEN status ELSE 'online' END,
               last_seen_at = now(),
               agent_version = left($2, 64),
               public_ip = COALESCE($3::text::inet, public_ip)
           WHERE id = $1 AND status <> 'revoked'"#,
        id,
        agent_version,
        ip,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Heartbeat: the agent answered recently.
///
/// # Errors
/// Database errors.
pub async fn touch_seen(pool: &sqlx::PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE nodes SET last_seen_at = now() WHERE id = $1 AND status IN ('online', 'draining')",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The session ended.
///
/// # Errors
/// Database errors.
pub async fn mark_offline(pool: &sqlx::PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE nodes SET status = 'offline' WHERE id = $1 AND status = 'online'",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Marks `online` nodes that have not been heard from for `silent_secs` as
/// `offline`; catches sessions lost without a goodbye, and nodes left `online`
/// by a control plane that was stopped. Returns how many changed.
///
/// # Errors
/// Database errors.
pub async fn mark_silent_offline(
    pool: &sqlx::PgPool,
    silent_secs: i32,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"UPDATE nodes SET status = 'offline'
           WHERE status = 'online'
             AND (last_seen_at IS NULL OR last_seen_at < now() - make_interval(secs => $1::int))"#,
        silent_secs,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

// ------------------------------------------------------------------- listing

/// A node as shown in the admin list.
#[derive(Debug, Clone)]
pub struct NodeSummary {
    /// Node id.
    pub id: Uuid,
    /// Node name.
    pub name: String,
    /// Host name the agent reported.
    pub hostname: Option<String>,
    /// Address the agent last connected from.
    pub public_ip: Option<String>,
    /// `online`, `offline`, `draining` or `revoked`. A node stored as `online`
    /// whose last heartbeat is older than the liveness window shows `offline`,
    /// even before the background sweep has caught up.
    pub status: String,
    /// Agent build version.
    pub agent_version: Option<String>,
    /// Last heartbeat (Unix seconds).
    pub last_seen_at: Option<i64>,
    /// Enrollment time (Unix seconds).
    pub created_at: i64,
}

/// The newest `limit` nodes. `liveness_secs` is how long a heartbeat counts as
/// recent.
///
/// # Errors
/// Database errors.
pub async fn list_nodes(
    pool: &sqlx::PgPool,
    limit: i64,
    liveness_secs: i32,
) -> Result<Vec<NodeSummary>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, name, hostname, host(public_ip) AS public_ip, agent_version,
                  CASE WHEN status = 'online'
                        AND (last_seen_at IS NULL OR last_seen_at < now() - make_interval(secs => $2::int))
                       THEN 'offline' ELSE status END AS "status!",
                  extract(epoch FROM last_seen_at)::bigint AS last_seen_at,
                  extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM nodes
           ORDER BY created_at DESC, id DESC
           LIMIT $1"#,
        limit,
        liveness_secs,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| NodeSummary {
            id: r.id,
            name: r.name,
            hostname: r.hostname,
            public_ip: r.public_ip,
            status: r.status,
            agent_version: r.agent_version,
            last_seen_at: r.last_seen_at,
            created_at: r.created_at,
        })
        .collect())
}

// ---------------------------------------------------------------- revocation

/// Outcome of [`revoke_node`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeNodeOutcome {
    /// The node is now revoked, together with this many of its certificates.
    Revoked {
        /// Certificates that were still valid and are now revoked.
        certificates: u64,
    },
    /// The node was revoked before.
    AlreadyRevoked,
    /// No such node.
    Unknown,
}

/// Revokes a node and every certificate of it that is not revoked yet. Final:
/// a revoked node never connects again; it has to be enrolled afresh. Call it
/// inside a transaction together with the audit entry.
///
/// # Errors
/// Database errors.
pub async fn revoke_node(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<RevokeNodeOutcome, sqlx::Error> {
    let current = sqlx::query_scalar!("SELECT status FROM nodes WHERE id = $1 FOR UPDATE", id)
        .fetch_optional(&mut *conn)
        .await?;
    match current.as_deref() {
        None => return Ok(RevokeNodeOutcome::Unknown),
        Some("revoked") => return Ok(RevokeNodeOutcome::AlreadyRevoked),
        Some(_) => {}
    }
    sqlx::query!("UPDATE nodes SET status = 'revoked' WHERE id = $1", id)
        .execute(&mut *conn)
        .await?;
    let done = sqlx::query!(
        "UPDATE node_certificates SET revoked_at = now(), revoke_reason = 'node_revoked'
         WHERE node_id = $1 AND revoked_at IS NULL",
        id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(RevokeNodeOutcome::Revoked {
        certificates: done.rows_affected(),
    })
}

/// A node's stored status (`online`, `offline`, `draining`, `revoked`), if it exists.
///
/// # Errors
/// Database errors.
pub async fn status_of(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT status FROM nodes WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}
