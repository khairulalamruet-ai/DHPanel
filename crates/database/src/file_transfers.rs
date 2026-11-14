//! Big files between the browser and a tenant's files (T08.07): uploads on their way in,
//! downloads on their way out, and the tokens and links that go with them.

use sqlx::{PgPool, types::Uuid};

/// A transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// Id.
    pub id: Uuid,
    /// The account it belongs to.
    pub tenant_id: Uuid,
    /// `upload` or `download`.
    pub kind: String,
    /// The file in the tenant's files.
    pub path: String,
    /// Replace a file that is already there (uploads).
    pub overwrite: bool,
    /// Size in bytes (an upload: announced; a download: known when it is whole).
    pub size_bytes: Option<i64>,
    /// Bytes received so far.
    pub received_bytes: i64,
    /// The chunk expected next.
    pub next_chunk: i32,
    /// `uploading`, `queued`, `running`, `ready`, `done`, `failed`, `cancelled`, `expired`.
    pub status: String,
    /// Why it failed.
    pub error: Option<String>,
    /// When it was started (Unix seconds).
    pub created_at: i64,
    /// When the file goes (Unix seconds; downloads).
    pub expires_at: Option<i64>,
}

struct Row {
    id: Uuid,
    tenant_id: Uuid,
    kind: String,
    path: String,
    overwrite: bool,
    size_bytes: Option<i64>,
    received_bytes: i64,
    next_chunk: i32,
    status: String,
    error: Option<String>,
    created_at: i64,
    expires_at: Option<i64>,
}

impl From<Row> for Transfer {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            kind: r.kind,
            path: r.path,
            overwrite: r.overwrite,
            size_bytes: r.size_bytes,
            received_bytes: r.received_bytes,
            next_chunk: r.next_chunk,
            status: r.status,
            error: r.error,
            created_at: r.created_at,
            expires_at: r.expires_at,
        }
    }
}

/// Starts a transfer. An upload waits for its chunks (`uploading`); a download is
/// `queued` for the node, with the token's hash (the sealed token is kept after).
///
/// # Errors
/// Database errors.
#[allow(clippy::too_many_arguments)]
pub async fn start(
    pool: &PgPool,
    tenant_id: Uuid,
    kind: &str,
    path: &str,
    overwrite: bool,
    size_bytes: Option<i64>,
    token_hash: Option<&[u8]>,
    created_by: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let status = if kind == "upload" {
        "uploading"
    } else {
        "queued"
    };
    sqlx::query_scalar!(
        "INSERT INTO file_transfers (tenant_id, kind, path, overwrite, size_bytes, status, token_hash, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        tenant_id,
        kind,
        path,
        overwrite,
        size_bytes,
        status,
        token_hash,
        created_by,
    )
    .fetch_one(pool)
    .await
}

/// One transfer of an account.
///
/// # Errors
/// Database errors.
pub async fn get(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<Transfer>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, kind, path, overwrite, size_bytes, received_bytes, next_chunk,
                  status, error, extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM expires_at)::bigint AS expires_at
           FROM file_transfers WHERE tenant_id = $1 AND id = $2"#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Transfer::from))
}

/// Keeps the sealed token (for the message to the node).
///
/// # Errors
/// Database errors.
pub async fn keep_sealed_token(pool: &PgPool, id: Uuid, sealed: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE file_transfers SET sealed_token = $2 WHERE id = $1",
        id,
        sealed
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The sealed token of a transfer the node is to work on.
///
/// # Errors
/// Database errors.
pub async fn sealed_token_of(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT sealed_token FROM file_transfers WHERE id = $1 AND status IN ('queued', 'running')",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// What a sealed token is sealed to.
#[must_use]
pub fn token_context(id: Uuid) -> Vec<u8> {
    format!("file-transfer-token:{id}").into_bytes()
}

/// A chunk of an upload has been written (only the one expected, only while uploading).
///
/// # Errors
/// Database errors.
pub async fn upload_chunk_written(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    index: i32,
    new_total: i64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE file_transfers SET received_bytes = $4, next_chunk = next_chunk + 1, updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND kind = 'upload' AND status = 'uploading'
           AND next_chunk = $3 AND $4 <= size_bytes",
        tenant_id,
        id,
        index,
        new_total,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The upload is whole and checked: `queued`, with the node's token.
///
/// # Errors
/// Database errors.
pub async fn queue_upload(
    pool: &PgPool,
    id: Uuid,
    token_hash: &[u8],
    sealed: &[u8],
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE file_transfers SET status = 'queued', token_hash = $2, sealed_token = $3, updated_at = now()
         WHERE id = $1 AND kind = 'upload' AND status = 'uploading'",
        id,
        token_hash,
        sealed,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Ends a transfer that did not work out (or that its owner gave up).
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
        "UPDATE file_transfers SET status = $2, error = left($3, 500), finished_at = now(), updated_at = now(),
                                   sealed_token = NULL, token_hash = NULL
         WHERE id = $1 AND status IN ('uploading', 'queued', 'running')",
        id,
        status,
        error,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// What the node's token must hash to: an upload the node fetches (`kind` `upload`) or a
/// download the node sends (`download`), while it is `queued` or `running`.
///
/// # Errors
/// Database errors.
pub async fn node_token_hash(
    pool: &PgPool,
    id: Uuid,
    kind: &str,
) -> Result<Option<(Vec<u8>, i64, i32)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT token_hash, received_bytes, next_chunk FROM file_transfers
         WHERE id = $1 AND kind = $2 AND status IN ('queued', 'running')",
        id,
        kind,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.token_hash.map(|h| (h, r.received_bytes, r.next_chunk))))
}

/// A chunk of a download has arrived from the node.
///
/// # Errors
/// Database errors.
pub async fn download_chunk_written(
    pool: &PgPool,
    id: Uuid,
    index: i32,
    new_total: i64,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE file_transfers SET received_bytes = $3, next_chunk = next_chunk + 1, updated_at = now()
         WHERE id = $1 AND kind = 'download' AND status IN ('queued', 'running') AND next_chunk = $2",
        id,
        index,
        new_total,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The download is whole and checked: `ready` for `keep_hours`.
///
/// # Errors
/// Database errors.
pub async fn download_ready(
    pool: &PgPool,
    id: Uuid,
    size_bytes: i64,
    sha256: &str,
    keep_hours: i32,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE file_transfers SET status = 'ready', size_bytes = $2, sha256 = $3, finished_at = now(),
                                   updated_at = now(), expires_at = now() + make_interval(hours => $4::int),
                                   sealed_token = NULL, token_hash = NULL
         WHERE id = $1 AND kind = 'download' AND status IN ('queued', 'running')",
        id,
        size_bytes,
        sha256,
        keep_hours,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Removes a finished download from use.
///
/// # Errors
/// Database errors.
pub async fn expire(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE file_transfers SET status = 'expired', link_hash = NULL, link_expires_at = NULL, updated_at = now()
         WHERE id = $1 AND status = 'ready'",
        id,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Expires downloads whose time is up and cancels uploads left alone for `idle_hours`.
///
/// # Errors
/// Database errors.
pub async fn tidy(pool: &PgPool, idle_hours: i32) -> Result<u64, sqlx::Error> {
    let a = sqlx::query!(
        "UPDATE file_transfers SET status = 'expired', link_hash = NULL, link_expires_at = NULL, updated_at = now()
         WHERE status = 'ready' AND expires_at < now()"
    )
    .execute(pool)
    .await?;
    let b = sqlx::query!(
        "UPDATE file_transfers SET status = 'cancelled', error = 'the upload was left unfinished',
                                   finished_at = now(), updated_at = now()
         WHERE status = 'uploading' AND updated_at < now() - make_interval(hours => $1::int)",
        idle_hours,
    )
    .execute(pool)
    .await?;
    Ok(a.rows_affected() + b.rows_affected())
}

/// A five-minute download link.
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
    sqlx::query_scalar!(
        r#"UPDATE file_transfers SET link_hash = $3, link_expires_at = now() + make_interval(mins => $4::int),
                  updated_at = now()
           WHERE tenant_id = $1 AND id = $2 AND kind = 'download' AND status = 'ready'
           RETURNING extract(epoch FROM link_expires_at)::bigint AS "at!""#,
        tenant_id,
        id,
        link_hash,
        minutes,
    )
    .fetch_optional(pool)
    .await
}

/// What a download link must hash to, and the file's own name, while the link is good.
///
/// # Errors
/// Database errors.
pub async fn link_target(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(Vec<u8>, String)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT link_hash, path FROM file_transfers
         WHERE id = $1 AND kind = 'download' AND status = 'ready' AND link_expires_at > now()",
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.link_hash.map(|h| (h, r.path))))
}

/// Ids of transfers whose spool file must be kept.
///
/// # Errors
/// Database errors.
pub async fn live_ids(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM file_transfers WHERE status IN ('uploading', 'queued', 'running', 'ready')"
    )
    .fetch_all(pool)
    .await
}
