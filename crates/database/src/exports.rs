//! SQL exports of an account (T07.08): the dump the node sends to the control
//! plane, and the short-lived link a client downloads it with.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// An export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    /// Id.
    pub id: Uuid,
    /// The account it belongs to.
    pub tenant_id: Uuid,
    /// The database it dumps.
    pub database_id: Uuid,
    /// The database's name.
    pub database: String,
    /// `queued`, `running`, `ready`, `failed` or `expired`.
    pub status: String,
    /// Bytes received from the node so far.
    pub received_bytes: i64,
    /// The chunk expected next.
    pub next_chunk: i32,
    /// Size of the finished file (bytes).
    pub size_bytes: Option<i64>,
    /// SHA-256 of the finished file (hex).
    pub sha256: Option<String>,
    /// Why it failed.
    pub error: Option<String>,
    /// When it was asked for (Unix seconds).
    pub created_at: i64,
    /// When the file goes (Unix seconds).
    pub expires_at: Option<i64>,
}

/// Why an export could not be started.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The database already has an export that has not ended.
    #[error("an export of this database is still going")]
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
    status: String,
    received_bytes: i64,
    next_chunk: i32,
    size_bytes: Option<i64>,
    sha256: Option<String>,
    error: Option<String>,
    created_at: i64,
    expires_at: Option<i64>,
}

impl From<Row> for Export {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            database_id: r.database_id,
            database: r.database,
            status: r.status,
            received_bytes: r.received_bytes,
            next_chunk: r.next_chunk,
            size_bytes: r.size_bytes,
            sha256: r.sha256,
            error: r.error,
            created_at: r.created_at,
            expires_at: r.expires_at,
        }
    }
}

/// Starts an export: the row, with the token the node will upload with (its hash,
/// and the token sealed until the node has it).
///
/// # Errors
/// [`StartError::Active`] when the database has an export that has not ended.
pub async fn start(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    database_id: Uuid,
    token_hash: &[u8],
    created_by: Uuid,
) -> Result<Uuid, StartError> {
    sqlx::query_scalar!(
        "INSERT INTO db_exports (tenant_id, database_id, upload_token_hash, created_by)
         VALUES ($1, $2, $3, $4) RETURNING id",
        tenant_id,
        database_id,
        token_hash,
        created_by,
    )
    .fetch_one(conn)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint()) == Some("db_exports_one_active_idx")
        {
            StartError::Active
        } else {
            StartError::Database(err)
        }
    })
}

/// Keeps the sealed token (for the message to the node).
///
/// # Errors
/// Database errors.
pub async fn keep_sealed_token(
    conn: &mut PgConnection,
    id: Uuid,
    sealed: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE db_exports SET sealed_token = $2 WHERE id = $1",
        id,
        sealed,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The sealed token of an export that waits for or is being made by the node.
///
/// # Errors
/// Database errors.
pub async fn sealed_token_of(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT sealed_token FROM db_exports WHERE id = $1 AND status IN ('queued', 'running')",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// What a sealed token is sealed to: that export and nothing else.
#[must_use]
pub fn token_context(id: Uuid) -> Vec<u8> {
    format!("db-export-token:{id}").into_bytes()
}

/// One export of an account.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<Export>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT e.id, e.tenant_id, e.database_id, d.name AS database, e.status,
                  e.received_bytes, e.next_chunk, e.size_bytes, e.sha256, e.error,
                  extract(epoch FROM e.created_at)::bigint AS "created_at!",
                  extract(epoch FROM e.expires_at)::bigint AS expires_at
           FROM db_exports e JOIN databases d ON d.id = e.database_id
           WHERE e.tenant_id = $1 AND e.id = $2"#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Export::from))
}

/// An account's most recent exports (newest first), at most `limit`.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid, limit: i64) -> Result<Vec<Export>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT e.id, e.tenant_id, e.database_id, d.name AS database, e.status,
                  e.received_bytes, e.next_chunk, e.size_bytes, e.sha256, e.error,
                  extract(epoch FROM e.created_at)::bigint AS "created_at!",
                  extract(epoch FROM e.expires_at)::bigint AS expires_at
           FROM db_exports e JOIN databases d ON d.id = e.database_id
           WHERE e.tenant_id = $1 ORDER BY e.created_at DESC LIMIT $2"#,
        tenant_id,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Export::from).collect())
}

/// The export an upload with this token is for: its state and what the token
/// must hash to (only while the node is at it).
pub struct UploadTarget {
    /// The token's hash.
    pub token_hash: Vec<u8>,
    /// Bytes received so far.
    pub received_bytes: i64,
    /// The chunk expected next.
    pub next_chunk: i32,
}

/// Looks up the upload an export is waiting for.
///
/// # Errors
/// Database errors.
pub async fn upload_target(pool: &PgPool, id: Uuid) -> Result<Option<UploadTarget>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT upload_token_hash, received_bytes, next_chunk FROM db_exports
         WHERE id = $1 AND status IN ('queued', 'running')",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| {
        r.upload_token_hash.map(|token_hash| UploadTarget {
            token_hash,
            received_bytes: r.received_bytes,
            next_chunk: r.next_chunk,
        })
    }))
}

/// A chunk has been written: only when it is the one expected.
///
/// # Errors
/// Database errors.
pub async fn chunk_written(
    pool: &PgPool,
    id: Uuid,
    index: i32,
    new_total: i64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_exports SET received_bytes = $3, next_chunk = next_chunk + 1, updated_at = now()
         WHERE id = $1 AND status IN ('queued', 'running') AND next_chunk = $2",
        id,
        index,
        new_total,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The dump is whole and checked: the export is `ready` for `keep_hours`.
///
/// # Errors
/// Database errors.
pub async fn ready(
    pool: &PgPool,
    id: Uuid,
    size_bytes: i64,
    sha256: &str,
    keep_hours: i32,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_exports SET status = 'ready', size_bytes = $2, sha256 = $3,
                               finished_at = now(), updated_at = now(),
                               expires_at = now() + make_interval(hours => $4::int),
                               sealed_token = NULL, upload_token_hash = NULL
         WHERE id = $1 AND status IN ('queued', 'running')",
        id,
        size_bytes,
        sha256,
        keep_hours,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Ends an export that did not work out.
///
/// # Errors
/// Database errors.
pub async fn fail(pool: &PgPool, id: Uuid, error: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_exports SET status = 'failed', error = left($2, 500), finished_at = now(),
                               updated_at = now(), sealed_token = NULL, upload_token_hash = NULL
         WHERE id = $1 AND status IN ('queued', 'running')",
        id,
        error,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Removes a finished export's file from use (`expired`), for its owner or when
/// its time is up.
///
/// # Errors
/// Database errors.
pub async fn expire(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_exports SET status = 'expired', link_hash = NULL, link_expires_at = NULL,
                               updated_at = now()
         WHERE id = $1 AND status = 'ready'",
        id,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Expires every ready export whose time is up; returns how many.
///
/// # Errors
/// Database errors.
pub async fn expire_due(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE db_exports SET status = 'expired', link_hash = NULL, link_expires_at = NULL,
                               updated_at = now()
         WHERE status = 'ready' AND expires_at < now()"
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Gives a ready export a download link that works for `minutes`.
///
/// # Errors
/// Database errors.
pub async fn set_link(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    link_hash: &[u8],
    minutes: i32,
) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        r#"UPDATE db_exports SET link_hash = $3,
                  link_expires_at = now() + make_interval(mins => $4::int), updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND status = 'ready'
           RETURNING extract(epoch FROM link_expires_at)::bigint AS "at!""#,
        tenant_id,
        id,
        link_hash,
        minutes,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// What a download link must hash to, and the database's name (for the file's
/// name), while the link is good.
///
/// # Errors
/// Database errors.
pub async fn link_target(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(Vec<u8>, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT e.link_hash, d.name FROM db_exports e JOIN databases d ON d.id = e.database_id
         WHERE e.id = $1 AND e.status = 'ready' AND e.link_expires_at > now()",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.link_hash.map(|h| (h, r.name))))
}

/// Ids of exports whose file must be kept.
///
/// # Errors
/// Database errors.
pub async fn live_ids(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!("SELECT id FROM db_exports WHERE status IN ('queued', 'running', 'ready')")
        .fetch_all(pool)
        .await
}
