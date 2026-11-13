//! The transactional outbox (`outbox`): events written with the change that
//! caused them, delivered afterwards by a relay.

use serde_json::Value;
use sqlx::{PgConnection, PgPool, Postgres, Transaction, types::Uuid};

/// An event waiting to be delivered.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// Outbox id (time-ordered).
    pub id: Uuid,
    /// Wire name of the event.
    pub event_type: String,
    /// The event as JSON.
    pub payload: Value,
    /// Failed deliveries so far.
    pub attempts: i32,
}

/// Writes an event. Call it on the connection of the transaction that makes
/// the change the event is about: both commit, or neither.
///
/// # Errors
/// Database errors (including the size and name checks of the table).
pub async fn publish(
    conn: &mut PgConnection,
    event_type: &str,
    payload: &Value,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO outbox (event_type, payload) VALUES ($1, $2) RETURNING id",
        event_type,
        payload,
    )
    .fetch_one(conn)
    .await
}

/// Opens a transaction and locks up to `limit` events that are due, oldest
/// first. Rows locked by another relay are skipped, so two relays never hold
/// the same event. Finish each event with [`mark_delivered`] or
/// [`mark_failed`] on the returned transaction, then commit it; an event that
/// is not finished simply stays due.
///
/// # Errors
/// Database errors.
pub async fn claim(
    pool: &PgPool,
    limit: i64,
) -> Result<(Transaction<'static, Postgres>, Vec<Pending>), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as!(
        Pending,
        r#"SELECT id, event_type, payload, attempts FROM outbox
           WHERE delivered_at IS NULL AND dead_at IS NULL AND next_attempt_at <= now()
           ORDER BY id
           LIMIT $1
           FOR UPDATE SKIP LOCKED"#,
        limit,
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok((tx, rows))
}

/// Records that every module handled the event.
///
/// # Errors
/// Database errors.
pub async fn mark_delivered(conn: &mut PgConnection, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE outbox SET delivered_at = now(), last_error = NULL WHERE id = $1",
        id,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Records a failed delivery: tried again after `retry_in_secs`, or given up
/// on (`dead`) when this was attempt number `max_attempts`. Returns whether
/// the event is now dead.
///
/// # Errors
/// Database errors.
pub async fn mark_failed(
    conn: &mut PgConnection,
    id: Uuid,
    error: &str,
    retry_in_secs: i32,
    max_attempts: i32,
) -> Result<bool, sqlx::Error> {
    let dead = sqlx::query_scalar!(
        r#"UPDATE outbox
           SET attempts = attempts + 1,
               last_error = left($2, 2000),
               next_attempt_at = now() + make_interval(secs => $3::int),
               dead_at = CASE WHEN attempts + 1 >= $4 THEN now() ELSE NULL END
           WHERE id = $1
           RETURNING dead_at IS NOT NULL AS "dead!""#,
        id,
        error,
        retry_in_secs,
        max_attempts,
    )
    .fetch_one(conn)
    .await?;
    Ok(dead)
}

/// How many events are waiting, and how many were given up on.
///
/// # Errors
/// Database errors.
pub async fn counts(pool: &PgPool) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT count(*) FILTER (WHERE delivered_at IS NULL AND dead_at IS NULL) AS "pending!",
                  count(*) FILTER (WHERE dead_at IS NOT NULL) AS "dead!"
           FROM outbox"#
    )
    .fetch_one(pool)
    .await?;
    Ok((row.pending, row.dead))
}

/// Removes delivered events older than `keep_days`. Returns how many.
///
/// # Errors
/// Database errors.
pub async fn prune_delivered(pool: &PgPool, keep_days: i32) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM outbox WHERE delivered_at < now() - make_interval(days => $1::int)",
        keep_days,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}
