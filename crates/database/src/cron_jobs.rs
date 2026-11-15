//! The `cron_jobs` table (T10.01).

use sqlx::{PgPool, types::Uuid};

/// A cron job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronJob {
    /// Id.
    pub id: Uuid,
    /// Its account.
    pub tenant_id: Uuid,
    /// A name, unique in the account.
    pub name: String,
    /// Five-field schedule (UTC).
    pub schedule: String,
    /// The command line.
    pub command: String,
    /// Whether it runs.
    pub enabled: bool,
    /// Stopped after this many seconds.
    pub timeout_seconds: i32,
    /// When it last ran (Unix seconds).
    pub last_run_at: Option<i64>,
    /// `ok`, `failed`, `timeout` or `skipped`.
    pub last_status: Option<String>,
    /// The exit code of the last run.
    pub last_exit_code: Option<i32>,
    /// The end of the last run's output.
    pub last_output: Option<String>,
    /// Counts every change.
    pub version: i64,
}

struct Row {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    schedule: String,
    command: String,
    enabled: bool,
    timeout_seconds: i32,
    last_run_at: Option<i64>,
    last_status: Option<String>,
    last_exit_code: Option<i32>,
    last_output: Option<String>,
    version: i64,
}

impl From<Row> for CronJob {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            schedule: r.schedule,
            command: r.command,
            enabled: r.enabled,
            timeout_seconds: r.timeout_seconds,
            last_run_at: r.last_run_at,
            last_status: r.last_status,
            last_exit_code: r.last_exit_code,
            last_output: r.last_output,
            version: r.version,
        }
    }
}

/// Why a job could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The name is used in this account.
    #[error("the name is taken")]
    Taken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn save_error(e: sqlx::Error) -> SaveError {
    if e.as_database_error().and_then(|d| d.constraint()) == Some("cron_jobs_tenant_id_name_key") {
        SaveError::Taken
    } else {
        SaveError::Database(e)
    }
}

/// What a job is made of.
#[derive(Debug, Clone)]
pub struct Fields<'a> {
    /// Its name.
    pub name: &'a str,
    /// Five-field schedule.
    pub schedule: &'a str,
    /// The command line.
    pub command: &'a str,
    /// Whether it runs.
    pub enabled: bool,
    /// Stopped after this many seconds.
    pub timeout_seconds: i32,
}

/// The jobs of an account, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<CronJob>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, schedule, command, enabled, timeout_seconds,
                  extract(epoch FROM last_run_at)::bigint AS last_run_at,
                  last_status, last_exit_code, last_output, version
           FROM cron_jobs WHERE tenant_id = $1 ORDER BY name"#,
        tenant_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One job of an account.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<CronJob>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, schedule, command, enabled, timeout_seconds,
                  extract(epoch FROM last_run_at)::bigint AS last_run_at,
                  last_status, last_exit_code, last_output, version
           FROM cron_jobs WHERE tenant_id = $1 AND id = $2"#,
        tenant_id,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// How many jobs an account has.
///
/// # Errors
/// Database errors.
pub async fn count(pool: &PgPool, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM cron_jobs WHERE tenant_id = $1"#,
        tenant_id
    )
    .fetch_one(pool)
    .await
}

/// Makes a job.
///
/// # Errors
/// [`SaveError::Taken`].
pub async fn create(pool: &PgPool, tenant_id: Uuid, f: &Fields<'_>) -> Result<Uuid, SaveError> {
    sqlx::query_scalar!(
        "INSERT INTO cron_jobs (tenant_id, name, schedule, command, enabled, timeout_seconds)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        tenant_id,
        f.name,
        f.schedule,
        f.command,
        f.enabled,
        f.timeout_seconds,
    )
    .fetch_one(pool)
    .await
    .map_err(save_error)
}

/// Changes a job. `false` when there is no such job.
///
/// # Errors
/// [`SaveError::Taken`].
pub async fn update(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    f: &Fields<'_>,
) -> Result<bool, SaveError> {
    Ok(sqlx::query!(
        "UPDATE cron_jobs
         SET name = $3, schedule = $4, command = $5, enabled = $6, timeout_seconds = $7,
             version = version + 1, updated_at = now()
         WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id,
        f.name,
        f.schedule,
        f.command,
        f.enabled,
        f.timeout_seconds,
    )
    .execute(pool)
    .await
    .map_err(save_error)?
    .rows_affected()
        == 1)
}

/// Removes a job.
///
/// # Errors
/// Database errors.
pub async fn delete(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM cron_jobs WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}
