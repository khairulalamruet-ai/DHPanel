//! SQL imports of an account (T07.07): the uploaded dump, its progress and
//! the one-time token the node fetches it with.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// An import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// Id.
    pub id: Uuid,
    /// The account it belongs to.
    pub tenant_id: Uuid,
    /// The database it fills.
    pub database_id: Uuid,
    /// The database's name.
    pub database: String,
    /// Whether the dump is gzip-compressed.
    pub gzip: bool,
    /// Size of the file (bytes).
    pub size_bytes: i64,
    /// SHA-256 of the file (hex).
    pub sha256: String,
    /// `uploading`, `queued`, `running`, `done`, `failed` or `cancelled`.
    pub status: String,
    /// Bytes received so far.
    pub received_bytes: i64,
    /// The chunk that is expected next.
    pub next_chunk: i32,
    /// What the node last said it was doing.
    pub stage: Option<String>,
    /// Bytes of the file the node has fetched or run.
    pub bytes_done: i64,
    /// Statements the node has run.
    pub statements: i64,
    /// Why it failed.
    pub error: Option<String>,
    /// When it was started (Unix seconds).
    pub created_at: i64,
    /// When it ended (Unix seconds).
    pub finished_at: Option<i64>,
}

/// Why an import could not be started.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The database already has an import that has not ended.
    #[error("an import of this database is still going")]
    Active,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    tenant_id: Uuid,
    database_id: Uuid,
    database: String,
    gzip: bool,
    size_bytes: i64,
    sha256: String,
    status: String,
    received_bytes: i64,
    next_chunk: i32,
    stage: Option<String>,
    bytes_done: i64,
    statements: i64,
    error: Option<String>,
    created_at: i64,
    finished_at: Option<i64>,
}

impl From<Row> for Import {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            database_id: r.database_id,
            database: r.database,
            gzip: r.gzip,
            size_bytes: r.size_bytes,
            sha256: r.sha256,
            status: r.status,
            received_bytes: r.received_bytes,
            next_chunk: r.next_chunk,
            stage: r.stage,
            bytes_done: r.bytes_done,
            statements: r.statements,
            error: r.error,
            created_at: r.created_at,
            finished_at: r.finished_at,
        }
    }
}

/// Starts an import: the row that the upload fills.
///
/// # Errors
/// [`StartError::Active`] when the database has an import that has not ended.
pub async fn start(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    database_id: Uuid,
    gzip: bool,
    size_bytes: i64,
    sha256: &str,
    created_by: Uuid,
) -> Result<Uuid, StartError> {
    sqlx::query_scalar!(
        "INSERT INTO db_imports (tenant_id, database_id, gzip, size_bytes, sha256, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        tenant_id,
        database_id,
        gzip,
        size_bytes,
        sha256,
        created_by,
    )
    .fetch_one(conn)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint()) == Some("db_imports_one_active_idx")
        {
            StartError::Active
        } else {
            StartError::Database(err)
        }
    })
}

/// One import of an account.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<Import>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT i.id, i.tenant_id, i.database_id, d.name AS database, i.gzip, i.size_bytes,
                  i.sha256, i.status, i.received_bytes, i.next_chunk, i.stage,
                  i.bytes_done, i.statements, i.error,
                  extract(epoch FROM i.created_at)::bigint AS "created_at!",
                  extract(epoch FROM i.finished_at)::bigint AS finished_at
           FROM db_imports i JOIN databases d ON d.id = i.database_id
           WHERE i.tenant_id = $1 AND i.id = $2"#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Import::from))
}

/// An account's most recent imports (newest first), at most `limit`.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid, limit: i64) -> Result<Vec<Import>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT i.id, i.tenant_id, i.database_id, d.name AS database, i.gzip, i.size_bytes,
                  i.sha256, i.status, i.received_bytes, i.next_chunk, i.stage,
                  i.bytes_done, i.statements, i.error,
                  extract(epoch FROM i.created_at)::bigint AS "created_at!",
                  extract(epoch FROM i.finished_at)::bigint AS finished_at
           FROM db_imports i JOIN databases d ON d.id = i.database_id
           WHERE i.tenant_id = $1 ORDER BY i.created_at DESC LIMIT $2"#,
        tenant_id,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Import::from).collect())
}

/// Records a chunk that has been written: only when it is the one expected and
/// the import is still uploading. Returns the new totals, or `None` when not.
///
/// # Errors
/// Database errors.
pub async fn chunk_written(
    conn: &mut PgConnection,
    id: Uuid,
    index: i32,
    new_total: i64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_imports SET received_bytes = $3, next_chunk = next_chunk + 1, updated_at = now()
         WHERE id = $1 AND status = 'uploading' AND next_chunk = $2 AND $3 <= size_bytes",
        id,
        index,
        new_total,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The upload is whole and checked: keeps the token (its hash, and the token
/// sealed until the node has it) and marks the import `queued`.
///
/// # Errors
/// Database errors.
pub async fn queue(
    conn: &mut PgConnection,
    id: Uuid,
    token_hash: &[u8],
    sealed_token: &[u8],
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_imports SET status = 'queued', token_hash = $2, sealed_token = $3,
                               updated_at = now()
         WHERE id = $1 AND status = 'uploading'",
        id,
        token_hash,
        sealed_token,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Ends an import that has not reached the node (or that the node never
/// answered): `cancelled` by its owner, `failed` by the panel.
///
/// # Errors
/// Database errors.
pub async fn end(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    error: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_imports SET status = $2, error = left($3, 500), finished_at = now(),
                               sealed_token = NULL, token_hash = NULL, updated_at = now()
         WHERE id = $1 AND status IN ('uploading', 'queued', 'running')",
        id,
        status,
        error,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// What the node's token must match for the download of `id`: the token's hash,
/// while the import waits for or is being run by the node.
///
/// # Errors
/// Database errors.
pub async fn download_token_hash(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT token_hash FROM db_imports WHERE id = $1 AND status IN ('queued', 'running')",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// The sealed token of an import (for the message to the node).
///
/// # Errors
/// Database errors.
pub async fn sealed_token_of(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT sealed_token FROM db_imports WHERE id = $1 AND status IN ('queued', 'running')",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// What a sealed token is sealed to: that import and nothing else.
#[must_use]
pub fn token_context(id: Uuid) -> Vec<u8> {
    format!("db-import-token:{id}").into_bytes()
}

/// Running imports and the node that runs each (for the progress pass).
///
/// # Errors
/// Database errors.
pub async fn running(pool: &PgPool) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT i.id, t.node_id FROM db_imports i JOIN tenants t ON t.id = i.tenant_id
         WHERE i.status = 'running'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.node_id)).collect())
}

/// Keeps what the node reported of a running import.
///
/// # Errors
/// Database errors.
pub async fn record_progress(
    pool: &PgPool,
    id: Uuid,
    stage: &str,
    bytes_done: i64,
    statements: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE db_imports SET stage = left($2, 20), bytes_done = $3, statements = $4, updated_at = now()
         WHERE id = $1 AND status = 'running'",
        id,
        stage,
        bytes_done,
        statements,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Cancels uploads that were left alone for `idle_hours`; returns how many.
///
/// # Errors
/// Database errors.
pub async fn cancel_abandoned(pool: &PgPool, idle_hours: i32) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_imports SET status = 'cancelled', error = 'the upload was left unfinished',
                               finished_at = now(), updated_at = now()
         WHERE status = 'uploading' AND updated_at < now() - make_interval(hours => $1::int)",
        idle_hours,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// The ids of every import with a spool file to keep (not ended).
///
/// # Errors
/// Database errors.
pub async fn live_ids(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM db_imports WHERE status IN ('uploading', 'queued', 'running')"
    )
    .fetch_all(pool)
    .await
}

/// The node does not know a running import (it restarted): ends it as failed,
/// but only a minute after it was last heard of, in case the node has simply not
/// started it yet.
///
/// # Errors
/// Database errors.
pub async fn lost(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE db_imports SET status = 'failed', finished_at = now(), updated_at = now(),
                               error = 'the node lost the import (it restarted); upload the file again',
                               sealed_token = NULL, token_hash = NULL
         WHERE id = $1 AND status = 'running' AND updated_at < now() - interval '60 seconds'",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}
