//! Backups, where they go and when they are made (T09.02).

use sqlx::{PgPool, types::Uuid};

/// A place backups are stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Destination {
    /// Id.
    pub id: Uuid,
    /// Its name.
    pub name: String,
    /// `local`, `s3` or `sftp`.
    pub kind: String,
    /// What is not secret.
    pub settings: serde_json::Value,
    /// Whether secret credentials are kept for it.
    pub has_credentials: bool,
    /// May receive backups.
    pub enabled: bool,
    /// Where new backups go unless said otherwise.
    pub is_default: bool,
    /// When it was last tested (Unix seconds).
    pub last_tested_at: Option<i64>,
    /// The result of that test.
    pub last_test_ok: Option<bool>,
    /// What was wrong.
    pub last_test_error: Option<String>,
}

/// Why a destination could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The name is used.
    #[error("the name is taken")]
    Taken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    name: String,
    kind: String,
    settings: serde_json::Value,
    has_credentials: bool,
    enabled: bool,
    is_default: bool,
    last_tested_at: Option<i64>,
    last_test_ok: Option<bool>,
    last_test_error: Option<String>,
}

impl From<Row> for Destination {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            kind: r.kind,
            settings: r.settings,
            has_credentials: r.has_credentials,
            enabled: r.enabled,
            is_default: r.is_default,
            last_tested_at: r.last_tested_at,
            last_test_ok: r.last_test_ok,
            last_test_error: r.last_test_error,
        }
    }
}

/// Every destination, by name.
///
/// # Errors
/// Database errors.
pub async fn destinations(pool: &PgPool) -> Result<Vec<Destination>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, name, kind, settings, (sealed_credentials IS NOT NULL) AS "has_credentials!",
                  enabled, is_default,
                  extract(epoch FROM last_tested_at)::bigint AS last_tested_at,
                  last_test_ok, last_test_error
           FROM backup_destinations ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Destination::from).collect())
}

/// One destination.
///
/// # Errors
/// Database errors.
pub async fn destination(pool: &PgPool, id: Uuid) -> Result<Option<Destination>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, name, kind, settings, (sealed_credentials IS NOT NULL) AS "has_credentials!",
                  enabled, is_default,
                  extract(epoch FROM last_tested_at)::bigint AS last_tested_at,
                  last_test_ok, last_test_error
           FROM backup_destinations WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Destination::from))
}

/// Makes a destination (the first one made is the default).
///
/// # Errors
/// [`SaveError::Taken`] when the name is used.
pub async fn add_destination(
    pool: &PgPool,
    name: &str,
    kind: &str,
    settings: &serde_json::Value,
    created_by: Uuid,
) -> Result<Uuid, SaveError> {
    sqlx::query_scalar!(
        "INSERT INTO backup_destinations (name, kind, settings, created_by, is_default)
         VALUES ($1, $2, $3, $4, NOT EXISTS (SELECT 1 FROM backup_destinations))
         RETURNING id",
        name,
        kind,
        settings,
        created_by,
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint())
            == Some("backup_destinations_name_key")
        {
            SaveError::Taken
        } else {
            SaveError::Database(err)
        }
    })
}

/// Keeps the sealed credentials of a destination (`None` removes them).
///
/// # Errors
/// Database errors.
pub async fn keep_credentials(
    pool: &PgPool,
    id: Uuid,
    sealed: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backup_destinations SET sealed_credentials = $2, updated_at = now() WHERE id = $1",
        id,
        sealed,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The sealed credentials of a destination.
///
/// # Errors
/// Database errors.
pub async fn sealed_credentials(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT sealed_credentials FROM backup_destinations WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// What sealed credentials are sealed to.
#[must_use]
pub fn credentials_context(id: Uuid) -> Vec<u8> {
    format!("backup-destination:{id}").into_bytes()
}

/// Changes what is not secret about a destination.
///
/// # Errors
/// [`SaveError::Taken`] when the name is used.
pub async fn change_destination(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    settings: &serde_json::Value,
    enabled: bool,
) -> Result<bool, SaveError> {
    let done = sqlx::query!(
        "UPDATE backup_destinations SET name = $2, settings = $3, enabled = $4, updated_at = now() WHERE id = $1",
        id,
        name,
        settings,
        enabled,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint()) == Some("backup_destinations_name_key") {
            SaveError::Taken
        } else {
            SaveError::Database(err)
        }
    })?;
    Ok(done.rows_affected() == 1)
}

/// Makes a destination the default.
///
/// # Errors
/// Database errors.
pub async fn make_default(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!("UPDATE backup_destinations SET is_default = false WHERE is_default")
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query!(
        "UPDATE backup_destinations SET is_default = true WHERE id = $1 AND enabled",
        id,
    )
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 1 {
        tx.commit().await?;
        Ok(true)
    } else {
        tx.rollback().await?;
        Ok(false)
    }
}

/// Removes a destination nothing refers to; `false` when backups or schedules use it.
///
/// # Errors
/// Database errors.
pub async fn remove_destination(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM backup_destinations d WHERE d.id = $1
           AND NOT EXISTS (SELECT 1 FROM backups b WHERE b.destination_id = d.id AND b.status <> 'deleted')
           AND NOT EXISTS (SELECT 1 FROM backup_schedules s WHERE s.destination_id = d.id)",
        id,
    )
    .execute(pool)
    .await;
    match done {
        Ok(d) => Ok(d.rows_affected() == 1),
        // Deleted backups still point at it: the history keeps it.
        Err(err)
            if err
                .as_database_error()
                .is_some_and(|e| e.is_foreign_key_violation()) =>
        {
            Ok(false)
        }
        Err(err) => Err(err),
    }
}

/// Keeps the result of a test.
///
/// # Errors
/// Database errors.
pub async fn record_test(
    pool: &PgPool,
    id: Uuid,
    ok: bool,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backup_destinations SET last_tested_at = now(), last_test_ok = $2, last_test_error = left($3, 500)
         WHERE id = $1",
        id,
        ok,
        error,
    )
    .execute(pool)
    .await?;
    Ok(())
}

// ------------------------------------------------------------ backups and schedules (T09.07)

/// A backup schedule that is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueSchedule {
    /// The schedule.
    pub id: Uuid,
    /// The account it is for (`None`: the panel's own backup).
    pub tenant_id: Option<Uuid>,
    /// The cron expression.
    pub cron: String,
    /// The destination it names (the default one when none).
    pub destination_id: Option<Uuid>,
}

/// Schedules whose time has come, oldest first.
///
/// # Errors
/// Database errors.
pub async fn due_schedules(pool: &PgPool, limit: i64) -> Result<Vec<DueSchedule>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, tenant_id, cron, destination_id FROM backup_schedules
         WHERE enabled AND next_run_at IS NOT NULL AND next_run_at <= now()
         ORDER BY next_run_at LIMIT $1",
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DueSchedule {
            id: r.id,
            tenant_id: r.tenant_id,
            cron: r.cron,
            destination_id: r.destination_id,
        })
        .collect())
}

/// Notes that a schedule ran and when it runs next (Unix seconds; `None` stops it).
///
/// # Errors
/// Database errors.
pub async fn advance_schedule(
    pool: &PgPool,
    id: Uuid,
    next_run_at: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backup_schedules
         SET last_run_at = now(), next_run_at = to_timestamp($2::bigint), updated_at = now()
         WHERE id = $1",
        id,
        next_run_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Makes or changes the schedule of an account.
///
/// # Errors
/// Database errors.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_schedule(
    pool: &PgPool,
    tenant_id: Uuid,
    cron: &str,
    keep_daily: i32,
    keep_weekly: i32,
    destination_id: Option<Uuid>,
    enabled: bool,
    next_run_at: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO backup_schedules
             (tenant_id, kind, cron, keep_daily, keep_weekly, destination_id, enabled, next_run_at)
         VALUES ($1, 'tenant', $2, $3, $4, $5, $6, to_timestamp($7::bigint))
         ON CONFLICT (tenant_id) DO UPDATE
         SET cron = $2, keep_daily = $3, keep_weekly = $4, destination_id = $5,
             enabled = $6, next_run_at = to_timestamp($7::bigint), updated_at = now()",
        tenant_id,
        cron,
        keep_daily,
        keep_weekly,
        destination_id,
        enabled,
        next_run_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// What retention needs to know about an account's schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// The account.
    pub tenant_id: Uuid,
    /// Keep the newest of this many recent days.
    pub keep_daily: i32,
    /// Keep the newest of this many recent weeks.
    pub keep_weekly: i32,
}

/// The retention rules of every account that has a schedule.
///
/// # Errors
/// Database errors.
pub async fn retentions(pool: &PgPool) -> Result<Vec<Retention>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT tenant_id AS "tenant_id!", keep_daily, keep_weekly
           FROM backup_schedules WHERE kind = 'tenant' AND tenant_id IS NOT NULL"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Retention {
            tenant_id: r.tenant_id,
            keep_daily: r.keep_daily,
            keep_weekly: r.keep_weekly,
        })
        .collect())
}

/// A new backup that is waiting to run.
///
/// # Errors
/// Database errors.
pub async fn new_backup(
    pool: &PgPool,
    tenant_id: Uuid,
    account_name: &str,
    trigger: &str,
    destination_id: Uuid,
    created_by: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO backups (tenant_id, account_name, kind, trigger, destination_id, created_by)
         VALUES ($1, $2, 'tenant', $3, $4, $5) RETURNING id",
        tenant_id,
        account_name,
        trigger,
        destination_id,
        created_by,
    )
    .fetch_one(pool)
    .await
}

/// Keeps the sealed key of a backup.
///
/// # Errors
/// Database errors.
pub async fn keep_key(pool: &PgPool, id: Uuid, sealed: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backups SET sealed_key = $2 WHERE id = $1",
        id,
        sealed
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The sealed key of a backup.
///
/// # Errors
/// Database errors.
pub async fn sealed_key_of(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT sealed_key FROM backups WHERE id = $1", id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// What a sealed backup key is sealed to.
#[must_use]
pub fn key_context(id: Uuid) -> Vec<u8> {
    format!("backup-key:{id}").into_bytes()
}

/// The destination a backup is stored at.
///
/// # Errors
/// Database errors.
pub async fn destination_of(pool: &PgPool, id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!("SELECT destination_id FROM backups WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

/// The destination new scheduled backups go to when a schedule names none.
///
/// # Errors
/// Database errors.
pub async fn default_destination(pool: &PgPool) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM backup_destinations WHERE is_default AND enabled")
        .fetch_optional(pool)
        .await
}

/// Backups queued or running on a node (what limits how many start at once).
///
/// # Errors
/// Database errors.
pub async fn active_on_node(pool: &PgPool, node_id: Uuid) -> Result<i64, sqlx::Error> {
    Ok(sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM backups b JOIN tenants t ON t.id = b.tenant_id
           WHERE t.node_id = $1 AND b.status IN ('queued', 'running')"#,
        node_id
    )
    .fetch_one(pool)
    .await?)
}

/// Scheduled backups of an account that are ready: `(id, created_at)` in Unix seconds.
///
/// # Errors
/// Database errors.
pub async fn ready_scheduled(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<Vec<(Uuid, i64)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, extract(epoch FROM created_at)::bigint AS "at!" FROM backups
           WHERE tenant_id = $1 AND trigger = 'schedule' AND status = 'ready'"#,
        tenant_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.at)).collect())
}

/// Marks a ready backup as being removed. `false` when it is not ready any more.
///
/// # Errors
/// Database errors.
pub async fn mark_deleting(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE backups SET status = 'deleting', updated_at = now()
         WHERE id = $1 AND status = 'ready'",
        id
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

/// A backup the runner works on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The backup.
    pub id: Uuid,
    /// Its account.
    pub tenant_id: Option<Uuid>,
    /// Where it is.
    pub destination_id: Uuid,
    /// When it was made (Unix seconds).
    pub created_at: i64,
    /// When it last changed (Unix seconds).
    pub stamp: i64,
}

/// Backups that are `deleting` and have no removal queued yet are found through the
/// ledger: this lists every `deleting` one, and the runner enqueues (idempotently).
///
/// # Errors
/// Database errors.
pub async fn deleting(pool: &PgPool, limit: i64) -> Result<Vec<Stored>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, tenant_id, destination_id,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM updated_at)::bigint AS "stamp!"
           FROM backups WHERE status = 'deleting' ORDER BY updated_at LIMIT $1"#,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Stored {
            id: r.id,
            tenant_id: r.tenant_id,
            destination_id: r.destination_id,
            created_at: r.created_at,
            stamp: r.stamp,
        })
        .collect())
}

/// Backups that are `queued` and have no operation yet (the runner enqueues, idempotently).
///
/// # Errors
/// Database errors.
pub async fn queued(pool: &PgPool, limit: i64) -> Result<Vec<Stored>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, tenant_id, destination_id,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM updated_at)::bigint AS "stamp!"
           FROM backups WHERE status = 'queued' ORDER BY created_at LIMIT $1"#,
        limit
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Stored {
            id: r.id,
            tenant_id: r.tenant_id,
            destination_id: r.destination_id,
            created_at: r.created_at,
            stamp: r.stamp,
        })
        .collect())
}
