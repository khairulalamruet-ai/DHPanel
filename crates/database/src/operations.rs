//! The operations ledger (`operations`): what the control plane asked nodes to
//! do, and how it went. See the migration for the status diagram; the database
//! itself refuses moves that are not on it.

use serde_json::Value;
use sqlx::{PgPool, types::Uuid};

/// Status of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Waiting to be sent.
    Pending,
    /// Sent to the node; no answer yet.
    Dispatched,
    /// Done.
    Succeeded,
    /// Failed for good.
    Failed,
    /// Failed, will be sent again.
    Retryable,
}

impl Status {
    /// The value stored in the database.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Dispatched => "dispatched",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Retryable => "retryable",
        }
    }

    /// Reads a stored or user-given status name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        ["pending", "dispatched", "succeeded", "failed", "retryable"]
            .contains(&name)
            .then(|| Self::parse(name))
    }

    fn parse(s: &str) -> Self {
        match s {
            "dispatched" => Self::Dispatched,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "retryable" => Self::Retryable,
            _ => Self::Pending,
        }
    }

    /// `succeeded` and `failed` never change again.
    #[must_use]
    pub fn is_final(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// One ledger row.
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    /// Ledger id (also the `op_id` on the wire).
    pub id: Uuid,
    /// Wire name of the operation.
    pub kind: String,
    /// Tenant it is about.
    pub tenant_id: Option<Uuid>,
    /// Node that runs it.
    pub node_id: Uuid,
    /// The typed operation as sent.
    pub payload: Value,
    /// Where it stands.
    pub status: Status,
    /// Deliveries so far.
    pub attempts: i32,
    /// Most deliveries allowed.
    pub max_attempts: i32,
    /// The caller's key for this request.
    pub idempotency_key: String,
    /// Result on success.
    pub output: Option<Value>,
    /// Failure detail.
    pub error: Option<Value>,
    /// Who asked.
    pub created_by: Option<Uuid>,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Not sent after this (Unix seconds).
    pub expires_at: i64,
    /// Earliest next delivery (Unix seconds).
    pub next_attempt_at: i64,
    /// Last delivery (Unix seconds).
    pub dispatched_at: Option<i64>,
    /// When it became final (Unix seconds).
    pub finished_at: Option<i64>,
}

/// A request to enqueue.
#[derive(Debug, Clone)]
pub struct NewOperation<'a> {
    /// Node that will run it.
    pub node_id: Uuid,
    /// Wire name (`ping`, ...).
    pub kind: &'a str,
    /// Tenant it is about.
    pub tenant_id: Option<Uuid>,
    /// The serialized operation (a JSON object).
    pub payload: &'a Value,
    /// Same key on the same node = same request.
    pub idempotency_key: &'a str,
    /// Who asked.
    pub created_by: Option<Uuid>,
    /// Seconds until it is no longer worth sending.
    pub ttl_secs: i32,
    /// Most deliveries (1..=20).
    pub max_attempts: i32,
}

/// What [`enqueue`] did.
#[derive(Debug, Clone, PartialEq)]
pub enum Enqueued {
    /// A new row.
    Created(Operation),
    /// The same request was enqueued before; this is that row.
    Existing(Operation),
}

impl Enqueued {
    /// The row, whichever it is.
    #[must_use]
    pub fn operation(&self) -> &Operation {
        match self {
            Self::Created(op) | Self::Existing(op) => op,
        }
    }
}

/// Why [`enqueue`] failed.
#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    /// The key was used before for a different request.
    #[error("the idempotency key was already used for a different operation")]
    KeyReused,
    /// No such node.
    #[error("unknown node")]
    UnknownNode,
    /// Any other database error (including constraint violations).
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    kind: String,
    tenant_id: Option<Uuid>,
    node_id: Uuid,
    payload: Value,
    status: String,
    attempts: i32,
    max_attempts: i32,
    idempotency_key: String,
    output: Option<Value>,
    error: Option<Value>,
    created_by: Option<Uuid>,
    created_at: i64,
    expires_at: i64,
    next_attempt_at: i64,
    dispatched_at: Option<i64>,
    finished_at: Option<i64>,
}

impl From<Row> for Operation {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            kind: r.kind,
            tenant_id: r.tenant_id,
            node_id: r.node_id,
            payload: r.payload,
            status: Status::parse(&r.status),
            attempts: r.attempts,
            max_attempts: r.max_attempts,
            idempotency_key: r.idempotency_key,
            output: r.output,
            error: r.error,
            created_by: r.created_by,
            created_at: r.created_at,
            expires_at: r.expires_at,
            next_attempt_at: r.next_attempt_at,
            dispatched_at: r.dispatched_at,
            finished_at: r.finished_at,
        }
    }
}

/// Adds an operation, or returns the existing row when the same request
/// (same node, same key, same kind and payload) was enqueued before.
///
/// # Errors
/// [`EnqueueError::KeyReused`] when the key belongs to a different request,
/// [`EnqueueError::UnknownNode`], or database errors.
pub async fn enqueue(pool: &PgPool, new: &NewOperation<'_>) -> Result<Enqueued, EnqueueError> {
    let mut conn = pool.acquire().await?;
    enqueue_in(&mut conn, new).await
}

/// Like [`enqueue`], on a plain connection so it can run inside a transaction
/// together with the change it belongs to (`&mut *tx`).
///
/// # Errors
/// As [`enqueue`].
pub async fn enqueue_in(
    conn: &mut sqlx::PgConnection,
    new: &NewOperation<'_>,
) -> Result<Enqueued, EnqueueError> {
    let inserted = sqlx::query_as!(
        Row,
        r#"INSERT INTO operations
               (kind, tenant_id, node_id, payload, idempotency_key, created_by, expires_at, max_attempts)
           VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(secs => $7::int), $8)
           ON CONFLICT (node_id, idempotency_key) DO NOTHING
           RETURNING id, kind, tenant_id, node_id, payload, status, attempts, max_attempts,
                     idempotency_key, output, error, created_by,
                     extract(epoch FROM created_at)::bigint AS "created_at!",
                     extract(epoch FROM expires_at)::bigint AS "expires_at!",
                     extract(epoch FROM next_attempt_at)::bigint AS "next_attempt_at!",
                     extract(epoch FROM dispatched_at)::bigint AS dispatched_at,
                     extract(epoch FROM finished_at)::bigint AS finished_at"#,
        new.kind,
        new.tenant_id,
        new.node_id,
        new.payload,
        new.idempotency_key,
        new.created_by,
        new.ttl_secs,
        new.max_attempts,
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|err| match err.as_database_error().and_then(|e| e.constraint()) {
        Some("operations_node_id_fkey") => EnqueueError::UnknownNode,
        _ => EnqueueError::Database(err),
    })?;
    if let Some(row) = inserted {
        return Ok(Enqueued::Created(row.into()));
    }
    // Lost the race with an earlier request that used the same key.
    let existing = sqlx::query_as!(
        Row,
        r#"SELECT id, kind, tenant_id, node_id, payload, status, attempts, max_attempts,
                  idempotency_key, output, error, created_by,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM expires_at)::bigint AS "expires_at!",
                  extract(epoch FROM next_attempt_at)::bigint AS "next_attempt_at!",
                  extract(epoch FROM dispatched_at)::bigint AS dispatched_at,
                  extract(epoch FROM finished_at)::bigint AS finished_at
           FROM operations WHERE node_id = $1 AND idempotency_key = $2"#,
        new.node_id,
        new.idempotency_key,
    )
    .fetch_one(&mut *conn)
    .await?;
    let existing = Operation::from(existing);
    if existing.kind == new.kind && existing.payload == *new.payload {
        Ok(Enqueued::Existing(existing))
    } else {
        Err(EnqueueError::KeyReused)
    }
}

/// One operation by id.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Operation>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, kind, tenant_id, node_id, payload, status, attempts, max_attempts,
                  idempotency_key, output, error, created_by,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM expires_at)::bigint AS "expires_at!",
                  extract(epoch FROM next_attempt_at)::bigint AS "next_attempt_at!",
                  extract(epoch FROM dispatched_at)::bigint AS dispatched_at,
                  extract(epoch FROM finished_at)::bigint AS finished_at
           FROM operations WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Takes up to `limit` operations that are due (pending or retryable, not
/// expired, for the given nodes) and marks them `dispatched`, counting the
/// attempt. Several workers can call this at once: each operation goes to
/// exactly one of them.
///
/// # Errors
/// Database errors.
pub async fn claim_due(
    pool: &PgPool,
    nodes: &[Uuid],
    limit: i64,
) -> Result<Vec<Operation>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"UPDATE operations o
           SET status = 'dispatched', attempts = o.attempts + 1, dispatched_at = now()
           WHERE o.id IN (
               SELECT id FROM operations
               WHERE status IN ('pending', 'retryable')
                 AND next_attempt_at <= now()
                 AND expires_at > now()
                 AND node_id = ANY($1)
               ORDER BY created_at, id
               LIMIT $2
               FOR UPDATE SKIP LOCKED
           )
           RETURNING o.id, o.kind, o.tenant_id, o.node_id, o.payload, o.status, o.attempts, o.max_attempts,
                     o.idempotency_key, o.output, o.error, o.created_by,
                     extract(epoch FROM o.created_at)::bigint AS "created_at!",
                     extract(epoch FROM o.expires_at)::bigint AS "expires_at!",
                     extract(epoch FROM o.next_attempt_at)::bigint AS "next_attempt_at!",
                     extract(epoch FROM o.dispatched_at)::bigint AS dispatched_at,
                     extract(epoch FROM o.finished_at)::bigint AS finished_at"#,
        nodes,
        limit,
    )
    .fetch_all(pool)
    .await?;
    let mut ops: Vec<Operation> = rows.into_iter().map(Into::into).collect();
    ops.sort_by_key(|o| (o.created_at, o.id));
    Ok(ops)
}

/// Puts a dispatched operation back to `pending` because it could not be
/// delivered (the connection was gone); the attempt is not counted.
/// Returns whether anything changed.
///
/// # Errors
/// Database errors.
pub async fn release_undelivered(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE operations SET status = 'pending', attempts = attempts - 1
         WHERE id = $1 AND status = 'dispatched' AND attempts > 0",
        id,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Records a successful result of a dispatched operation. Returns whether the
/// operation was `dispatched` (anything else is left alone).
///
/// # Errors
/// Database errors.
pub async fn succeed(pool: &PgPool, id: Uuid, output: &Value) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE operations SET status = 'succeeded', output = $2, error = NULL, finished_at = now()
         WHERE id = $1 AND status = 'dispatched'",
        id,
        output,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Records a failure of a dispatched operation. `retry_in_secs` is `Some` when
/// the failure is retryable: the operation waits that long and is sent again,
/// unless it has used up its attempts or would expire first, in which case it
/// fails for good. Returns the status it ended in, or `None` if the operation
/// was not `dispatched`.
///
/// # Errors
/// Database errors.
pub async fn fail(
    pool: &PgPool,
    id: Uuid,
    error: &Value,
    retry_in_secs: Option<i32>,
) -> Result<Option<Status>, sqlx::Error> {
    let status = sqlx::query_scalar!(
        r#"UPDATE operations
           SET status = CASE
                   WHEN $3::int IS NOT NULL
                        AND attempts < max_attempts
                        AND now() + make_interval(secs => $3::int) < expires_at
                   THEN 'retryable' ELSE 'failed' END,
               error = $2,
               next_attempt_at = CASE
                   WHEN $3::int IS NOT NULL THEN now() + make_interval(secs => $3::int)
                   ELSE next_attempt_at END,
               finished_at = CASE
                   WHEN $3::int IS NOT NULL
                        AND attempts < max_attempts
                        AND now() + make_interval(secs => $3::int) < expires_at
                   THEN NULL ELSE now() END
           WHERE id = $1 AND status = 'dispatched'
           RETURNING status AS "status!""#,
        id,
        error,
        retry_in_secs,
    )
    .fetch_optional(pool)
    .await?;
    Ok(status.map(|s| Status::parse(&s)))
}

/// Fails operations that were never sent and ran out of time (`pending` or
/// `retryable` past `expires_at`). Returns how many.
///
/// # Errors
/// Database errors.
pub async fn expire_overdue(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        r#"UPDATE operations
           SET status = 'failed', finished_at = now(),
               error = '{"code": "expired", "message": "the operation expired before it could be delivered", "retryable": false}'::jsonb
           WHERE status IN ('pending', 'retryable') AND expires_at <= now()"#,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Recent operations, newest first, optionally of one node, status or kind.
///
/// # Errors
/// Database errors.
pub async fn list_recent(
    pool: &PgPool,
    node_id: Option<Uuid>,
    status: Option<Status>,
    kind: Option<&str>,
    limit: i64,
) -> Result<Vec<Operation>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, kind, tenant_id, node_id, payload, status, attempts, max_attempts,
                  idempotency_key, output, error, created_by,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM expires_at)::bigint AS "expires_at!",
                  extract(epoch FROM next_attempt_at)::bigint AS "next_attempt_at!",
                  extract(epoch FROM dispatched_at)::bigint AS dispatched_at,
                  extract(epoch FROM finished_at)::bigint AS finished_at
           FROM operations
           WHERE ($1::uuid IS NULL OR node_id = $1)
             AND ($2::text IS NULL OR status = $2)
             AND ($3::text IS NULL OR kind = $3)
           ORDER BY created_at DESC, id DESC
           LIMIT $4"#,
        node_id,
        status.map(Status::as_str),
        kind,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// How long an operation of a long-running kind (`import_sql`, `export_sql`) may stay
/// unanswered: 6.5 hours, a little over the node's own limit.
pub const LONG_RUNNING_SECS: i32 = 6 * 3600 + 1800;

/// Handles operations that were sent but never answered within `timeout_secs`
/// (the node vanished, restarted, or lost the frame): they become `retryable`
/// after `retry_in_secs` while attempts and time remain, otherwise `failed`.
/// Returns the ids and the status each ended in. An SQL import (`import_sql`, `export_sql`)
/// runs for hours and gets [`LONG_RUNNING_SECS`] instead of `timeout_secs`.
///
/// # Errors
/// Database errors.
pub async fn time_out_dispatched(
    pool: &PgPool,
    timeout_secs: i32,
    retry_in_secs: i32,
) -> Result<Vec<(Uuid, Status)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"UPDATE operations
           SET status = CASE
                   WHEN attempts < max_attempts
                        AND now() + make_interval(secs => $2::int) < expires_at
                   THEN 'retryable' ELSE 'failed' END,
               error = '{"code": "timeout", "message": "the node did not answer in time", "retryable": true}'::jsonb,
               next_attempt_at = now() + make_interval(secs => $2::int),
               finished_at = CASE
                   WHEN attempts < max_attempts
                        AND now() + make_interval(secs => $2::int) < expires_at
                   THEN NULL ELSE now() END
           WHERE status = 'dispatched'
             AND dispatched_at < now() - make_interval(
                     secs => CASE WHEN kind IN ('import_sql', 'export_sql', 'open_phpmyadmin', 'fetch_file', 'send_file',
                                          'create_backup', 'restore_backup', 'delete_backup')
                                  OR (kind = 'file_op' AND payload #>> '{request,type}'
                                      IN ('archive', 'extract', 'copy', 'fix_permissions', 'chmod', 'empty_trash', 'disk_usage'))
                              THEN $3::int ELSE $1::int END)
           RETURNING id, status AS "status!""#,
        timeout_secs,
        retry_in_secs,
        LONG_RUNNING_SECS,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, Status::parse(&r.status)))
        .collect())
}
