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
