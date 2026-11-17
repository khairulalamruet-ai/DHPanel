//! Node-to-node transfers of a hosting account (T15.07). Plain queries, so these tables need
//! no offline cache.

use sqlx::{PgPool, types::Uuid};

/// A transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// Id.
    pub id: Uuid,
    /// The account.
    pub tenant_id: Uuid,
    /// Where it is.
    pub from_node: Uuid,
    /// Where it goes.
    pub to_node: Uuid,
    /// The backup destination both nodes reach.
    pub destination_id: Uuid,
    /// Go on to the last pass by itself.
    pub auto_cutover: bool,
    /// Where it stands.
    pub state: String,
    /// The operation, backup or restore it waits for.
    pub waiting_for: Option<Uuid>,
    /// The first (full) backup.
    pub first_backup: Option<Uuid>,
    /// The last pass's backup.
    pub delta_backup: Option<Uuid>,
    /// When the first copy began (Unix seconds).
    pub copy_started_at: Option<i64>,
    /// When the account was frozen for the last pass (Unix seconds).
    pub frozen_at: Option<i64>,
    /// How long the site was off, in seconds.
    pub downtime_secs: Option<i32>,
    /// What went wrong.
    pub error: Option<String>,
    /// Who asked.
    pub created_by: Option<Uuid>,
    /// When (Unix seconds).
    pub created_at: i64,
    /// When it last changed (Unix seconds).
    pub updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    tenant_id: Uuid,
    from_node: Uuid,
    to_node: Uuid,
    destination_id: Uuid,
    auto_cutover: bool,
    state: String,
    waiting_for: Option<Uuid>,
    first_backup: Option<Uuid>,
    delta_backup: Option<Uuid>,
    copy_started_at: Option<i64>,
    frozen_at: Option<i64>,
    downtime_secs: Option<i32>,
    error: Option<String>,
    created_by: Option<Uuid>,
    created_at: i64,
    updated_at: i64,
}

const COLUMNS: &str = "id, tenant_id, from_node, to_node, destination_id, auto_cutover, state,
    waiting_for, first_backup, delta_backup, copy_started_at, frozen_at, downtime_secs, error,
    created_by, extract(epoch FROM created_at)::bigint AS created_at,
    extract(epoch FROM updated_at)::bigint AS updated_at";

fn transfer(r: Row) -> Transfer {
    Transfer {
        id: r.id,
        tenant_id: r.tenant_id,
        from_node: r.from_node,
        to_node: r.to_node,
        destination_id: r.destination_id,
        auto_cutover: r.auto_cutover,
        state: r.state,
        waiting_for: r.waiting_for,
        first_backup: r.first_backup,
        delta_backup: r.delta_backup,
        copy_started_at: r.copy_started_at,
        frozen_at: r.frozen_at,
        downtime_secs: r.downtime_secs,
        error: r.error,
        created_by: r.created_by,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

/// Why a transfer could not be made.
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    /// The account has one that is not over.
    #[error("the account is being transferred already")]
    Busy,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Starts a transfer (`requested`).
///
/// # Errors
/// [`CreateError::Busy`], or database errors.
pub async fn create(
    pool: &PgPool,
    tenant_id: Uuid,
    to_node: Uuid,
    destination_id: Uuid,
    auto_cutover: bool,
    created_by: Option<Uuid>,
) -> Result<Transfer, CreateError> {
    let row = sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO tenant_transfers (tenant_id, from_node, to_node, destination_id, auto_cutover, created_by)
         SELECT t.id, t.node_id, $2, $3, $4, $5 FROM tenants t WHERE t.id = $1
         RETURNING {COLUMNS}"
    )))
    .bind(tenant_id)
    .bind(to_node)
    .bind(destination_id)
    .bind(auto_cutover)
    .bind(created_by)
    .fetch_optional(pool)
    .await
    .map_err(|e| match e.as_database_error().and_then(|d| d.constraint()) {
        Some("tenant_transfers_live_idx") => CreateError::Busy,
        _ => CreateError::Database(e),
    })?;
    row.map(transfer)
        .ok_or(CreateError::Database(sqlx::Error::RowNotFound))
}

/// One transfer.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Transfer>, sqlx::Error> {
    Ok(sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM tenant_transfers WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(transfer))
}

/// The newest transfers.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, limit: i64) -> Result<Vec<Transfer>, sqlx::Error> {
    Ok(sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM tenant_transfers ORDER BY created_at DESC LIMIT $1"
    )))
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(transfer)
    .collect())
}

/// The transfers that are not over.
///
/// # Errors
/// Database errors.
pub async fn active(pool: &PgPool) -> Result<Vec<Transfer>, sqlx::Error> {
    Ok(sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM tenant_transfers
         WHERE state NOT IN ('done', 'cancelled', 'failed', 'precopied', 'switched')
            OR (state = 'precopied' AND auto_cutover)
         ORDER BY created_at"
    )))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(transfer)
    .collect())
}

/// Moves a transfer to a state, with what it waits for next.
///
/// # Errors
/// Database errors.
pub async fn set_state(
    pool: &PgPool,
    id: Uuid,
    state: &str,
    waiting_for: Option<Uuid>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tenant_transfers SET state = $2, waiting_for = $3, error = $4, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(state)
    .bind(waiting_for)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// Keeps one of the transfer's own fields.
///
/// # Errors
/// Database errors.
pub async fn keep(
    pool: &PgPool,
    id: Uuid,
    first_backup: Option<Uuid>,
    delta_backup: Option<Uuid>,
    copy_started_at: Option<i64>,
    frozen_at: Option<i64>,
    downtime_secs: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE tenant_transfers SET
             first_backup = COALESCE($2, first_backup), delta_backup = COALESCE($3, delta_backup),
             copy_started_at = COALESCE($4, copy_started_at), frozen_at = COALESCE($5, frozen_at),
             downtime_secs = COALESCE($6, downtime_secs), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(first_backup)
    .bind(delta_backup)
    .bind(copy_started_at)
    .bind(frozen_at)
    .bind(downtime_secs)
    .execute(pool)
    .await?;
    Ok(())
}

/// What a backup or a restore of a transfer needs beyond an ordinary one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extras {
    /// The node that does it.
    pub node_id: Uuid,
    /// Only files after this time (Unix seconds).
    pub files_since: Option<i64>,
    /// Carry the database users' passwords.
    pub with_db_users: bool,
    /// Make databases that are not there.
    pub create_databases: bool,
    /// Make the users the backup carries.
    pub restore_db_users: bool,
}

/// Attaches [`Extras`] to a backup or a restore.
///
/// # Errors
/// Database errors.
pub async fn add_extras(
    pool: &PgPool,
    id: Uuid,
    transfer_id: Uuid,
    extras: &Extras,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO transfer_extras
             (id, transfer_id, node_id, files_since, with_db_users, create_databases, restore_db_users)
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(transfer_id)
    .bind(extras.node_id)
    .bind(extras.files_since)
    .bind(extras.with_db_users)
    .bind(extras.create_databases)
    .bind(extras.restore_db_users)
    .execute(pool)
    .await?;
    Ok(())
}

/// The [`Extras`] of a backup or a restore, if it belongs to a transfer.
///
/// # Errors
/// Database errors.
pub async fn extras(pool: &PgPool, id: Uuid) -> Result<Option<Extras>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (Uuid, Option<i64>, bool, bool, bool)>(
        "SELECT node_id, files_since, with_db_users, create_databases, restore_db_users
         FROM transfer_extras WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .map(
        |(node_id, files_since, with_db_users, create_databases, restore_db_users)| Extras {
            node_id,
            files_since,
            with_db_users,
            create_databases,
            restore_db_users,
        },
    ))
}

/// What an account has that a transfer cannot carry yet, named.
///
/// # Errors
/// Database errors.
pub async fn unsupported(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<&'static str>, sqlx::Error> {
    let mut found = Vec::new();
    for (what, query) in [
        (
            "applications (Node.js, Python or containers)",
            "SELECT EXISTS (SELECT 1 FROM runtime_apps WHERE tenant_id = $1)",
        ),
        (
            "cache add-ons",
            "SELECT EXISTS (SELECT 1 FROM cache_instances WHERE tenant_id = $1)",
        ),
        (
            "FTP or SFTP accounts",
            "SELECT EXISTS (SELECT 1 FROM ftp_accounts WHERE tenant_id = $1)",
        ),
        (
            "PostgreSQL databases",
            "SELECT EXISTS (SELECT 1 FROM databases WHERE tenant_id = $1 AND engine <> 'mysql')",
        ),
        (
            "databases on a server of its own",
            "SELECT EXISTS (SELECT 1 FROM tenant_db_servers WHERE tenant_id = $1 AND state <> 'shared')",
        ),
    ] {
        if sqlx::query_scalar::<_, bool>(query)
            .bind(tenant_id)
            .fetch_one(pool)
            .await?
        {
            found.push(what);
        }
    }
    Ok(found)
}
