//! The `plans` table.

use serde_json::Value;
use sqlx::{PgConnection, PgPool, types::Uuid};

/// A plan, as stored. Unlimited resources are `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Id.
    pub id: Uuid,
    /// Unique name.
    pub name: String,
    /// Description.
    pub description: String,
    /// The feature set.
    pub feature_set_id: Uuid,
    /// Its name.
    pub feature_set_name: String,
    /// New accounts get this plan unless told otherwise.
    pub is_default: bool,
    /// CPU in thousandths of a core.
    pub cpu_millis: Option<i32>,
    /// Memory.
    pub memory_bytes: Option<i64>,
    /// Processes.
    pub pids: Option<i32>,
    /// Disk space.
    pub disk_bytes: Option<i64>,
    /// File count.
    pub inodes: Option<i64>,
    /// Transfer per month.
    pub bandwidth_bytes: Option<i64>,
    /// Module limits, key to number (-1 = unlimited).
    pub limits: Value,
    /// `standard` or `strict`.
    pub isolation: String,
    /// Created by the installation.
    pub built_in: bool,
    /// Accounts on this plan.
    pub users: i64,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Last change (Unix seconds).
    pub updated_at: i64,
}

/// What a plan is made of when written.
#[derive(Debug, Clone)]
pub struct PlanWrite<'a> {
    /// Unique name.
    pub name: &'a str,
    /// Description.
    pub description: &'a str,
    /// The feature set.
    pub feature_set_id: Uuid,
    /// CPU in thousandths of a core.
    pub cpu_millis: Option<i32>,
    /// Memory.
    pub memory_bytes: Option<i64>,
    /// Processes.
    pub pids: Option<i32>,
    /// Disk space.
    pub disk_bytes: Option<i64>,
    /// File count.
    pub inodes: Option<i64>,
    /// Transfer per month.
    pub bandwidth_bytes: Option<i64>,
    /// Module limits.
    pub limits: &'a Value,
    /// `standard` or `strict`.
    pub isolation: &'a str,
}

/// Why a write failed.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Another plan has this name.
    #[error("a plan with this name already exists")]
    NameTaken,
    /// The feature set does not exist.
    #[error("unknown feature set")]
    UnknownFeatureSet,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn classify(err: sqlx::Error) -> WriteError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("plans_name_key") => WriteError::NameTaken,
        Some("plans_feature_set_id_fkey") => WriteError::UnknownFeatureSet,
        _ => WriteError::Database(err),
    }
}

struct Row {
    id: Uuid,
    name: String,
    description: String,
    feature_set_id: Uuid,
    feature_set_name: String,
    is_default: bool,
    cpu_millis: Option<i32>,
    memory_bytes: Option<i64>,
    pids: Option<i32>,
    disk_bytes: Option<i64>,
    inodes: Option<i64>,
    bandwidth_bytes: Option<i64>,
    limits: Value,
    isolation: String,
    built_in: bool,
    users: i64,
    created_at: i64,
    updated_at: i64,
}

impl From<Row> for Plan {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            feature_set_id: r.feature_set_id,
            feature_set_name: r.feature_set_name,
            is_default: r.is_default,
            cpu_millis: r.cpu_millis,
            memory_bytes: r.memory_bytes,
            pids: r.pids,
            disk_bytes: r.disk_bytes,
            inodes: r.inodes,
            bandwidth_bytes: r.bandwidth_bytes,
            limits: r.limits,
            isolation: r.isolation,
            built_in: r.built_in,
            users: r.users,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// All plans, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Plan>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT p.id, p.name, p.description, p.feature_set_id, f.name AS feature_set_name,
                  p.is_default, p.cpu_millis, p.memory_bytes, p.pids, p.disk_bytes, p.inodes,
                  p.bandwidth_bytes, p.limits, p.isolation, p.built_in,
                  0::bigint AS "users!",
                  extract(epoch FROM p.created_at)::bigint AS "created_at!",
                  extract(epoch FROM p.updated_at)::bigint AS "updated_at!"
           FROM plans p JOIN feature_sets f ON f.id = p.feature_set_id
           ORDER BY p.name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One plan.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Plan>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT p.id, p.name, p.description, p.feature_set_id, f.name AS feature_set_name,
                  p.is_default, p.cpu_millis, p.memory_bytes, p.pids, p.disk_bytes, p.inodes,
                  p.bandwidth_bytes, p.limits, p.isolation, p.built_in,
                  0::bigint AS "users!",
                  extract(epoch FROM p.created_at)::bigint AS "created_at!",
                  extract(epoch FROM p.updated_at)::bigint AS "updated_at!"
           FROM plans p JOIN feature_sets f ON f.id = p.feature_set_id
           WHERE p.id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Creates a plan and returns its id.
///
/// # Errors
/// [`WriteError`].
pub async fn insert(conn: &mut PgConnection, plan: &PlanWrite<'_>) -> Result<Uuid, WriteError> {
    sqlx::query_scalar!(
        r#"INSERT INTO plans (name, description, feature_set_id, cpu_millis, memory_bytes, pids,
                              disk_bytes, inodes, bandwidth_bytes, limits, isolation)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id"#,
        plan.name,
        plan.description,
        plan.feature_set_id,
        plan.cpu_millis,
        plan.memory_bytes,
        plan.pids,
        plan.disk_bytes,
        plan.inodes,
        plan.bandwidth_bytes,
        plan.limits,
        plan.isolation,
    )
    .fetch_one(conn)
    .await
    .map_err(classify)
}

/// Replaces a plan's contents (the name too, unless it is built in). Returns
/// whether the plan exists.
///
/// # Errors
/// [`WriteError`].
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    plan: &PlanWrite<'_>,
) -> Result<bool, WriteError> {
    let done = sqlx::query!(
        r#"UPDATE plans
           SET name = CASE WHEN built_in THEN name ELSE $2 END,
               description = $3, feature_set_id = $4, cpu_millis = $5, memory_bytes = $6,
               pids = $7, disk_bytes = $8, inodes = $9, bandwidth_bytes = $10,
               limits = $11, isolation = $12
           WHERE id = $1"#,
        id,
        plan.name,
        plan.description,
        plan.feature_set_id,
        plan.cpu_millis,
        plan.memory_bytes,
        plan.pids,
        plan.disk_bytes,
        plan.inodes,
        plan.bandwidth_bytes,
        plan.limits,
        plan.isolation,
    )
    .execute(conn)
    .await
    .map_err(classify)?;
    Ok(done.rows_affected() == 1)
}

/// Makes this the default plan (and no other). Call it in the transaction of
/// the write.
///
/// # Errors
/// Database errors.
pub async fn make_default(conn: &mut PgConnection, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE plans SET is_default = false WHERE is_default AND id <> $1",
        id
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!("UPDATE plans SET is_default = true WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// What [`delete`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// Deleted.
    Deleted,
    /// No such plan.
    Unknown,
    /// Created by the installation.
    BuiltIn,
    /// It is the default plan.
    IsDefault,
    /// Accounts are on it.
    InUse,
}

/// Deletes a plan that is not built in, not the default and not in use.
///
/// # Errors
/// Database errors.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<DeleteOutcome, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT built_in, is_default FROM plans WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *conn)
    .await?;
    match row {
        None => Ok(DeleteOutcome::Unknown),
        Some(r) if r.built_in => Ok(DeleteOutcome::BuiltIn),
        Some(r) if r.is_default => Ok(DeleteOutcome::IsDefault),
        // Accounts on a plan arrive with the tenants table (T04.05); until then no plan is in use.
        Some(_) => {
            sqlx::query!("DELETE FROM plans WHERE id = $1", id)
                .execute(conn)
                .await?;
            Ok(DeleteOutcome::Deleted)
        }
    }
}

/// The largest machine among the nodes that reported their capabilities.
/// `None` for a figure no node has reported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeMaximum {
    /// Most CPU cores on one node.
    pub cpu_cores: Option<i32>,
    /// Most memory on one node, in bytes.
    pub memory_bytes: Option<i64>,
    /// Most disk space on one node, in bytes.
    pub disk_bytes: Option<i64>,
}

/// The largest node figures, ignoring revoked nodes.
///
/// # Errors
/// Database errors.
pub async fn node_maximum(pool: &PgPool) -> Result<NodeMaximum, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT max((capabilities ->> 'cpu_cores')::int) AS cpu_cores,
                  max((capabilities ->> 'memory_bytes')::bigint) AS memory_bytes,
                  max((capabilities ->> 'disk_total_bytes')::bigint) AS disk_bytes
           FROM nodes WHERE status <> 'revoked' AND capabilities_at IS NOT NULL"#
    )
    .fetch_one(pool)
    .await?;
    Ok(NodeMaximum {
        cpu_cores: row.cpu_cores,
        memory_bytes: row.memory_bytes,
        disk_bytes: row.disk_bytes,
    })
}
