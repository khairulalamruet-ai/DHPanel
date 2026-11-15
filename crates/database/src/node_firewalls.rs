//! The `node_firewalls` table (T11.06): the firewall policy of each node, as the JSON document
//! the API checked.

use sqlx::{PgPool, types::Uuid};

/// The policy of a node (`None`: none was set).
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, node: Uuid) -> Result<Option<serde_json::Value>, sqlx::Error> {
    sqlx::query_scalar!("SELECT policy FROM node_firewalls WHERE node_id = $1", node)
        .fetch_optional(pool)
        .await
}

/// Every node's policy.
///
/// # Errors
/// Database errors.
pub async fn all(pool: &PgPool) -> Result<Vec<(Uuid, serde_json::Value)>, sqlx::Error> {
    let rows = sqlx::query!("SELECT node_id, policy FROM node_firewalls")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.node_id, r.policy)).collect())
}

/// Sets the policy of a node. `false` when there is no such node.
///
/// # Errors
/// Database errors.
pub async fn set(
    pool: &PgPool,
    node: Uuid,
    policy: &serde_json::Value,
    by: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "INSERT INTO node_firewalls (node_id, policy, updated_by)
         SELECT id, $2, $3 FROM nodes WHERE id = $1
         ON CONFLICT (node_id) DO UPDATE SET policy = $2, updated_by = $3, updated_at = now()",
        node,
        policy,
        by
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(done == 1)
}

/// Whether the node exists.
///
/// # Errors
/// Database errors.
pub async fn node_exists(pool: &PgPool, node: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM nodes WHERE id = $1) AS "e!""#,
        node
    )
    .fetch_one(pool)
    .await
}
