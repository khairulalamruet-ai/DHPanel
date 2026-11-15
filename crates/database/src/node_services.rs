//! The `node_services` table (T10.13).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// One service of a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeService {
    /// `database`, `phpmyadmin`, `sftp`, `caddy` or `agent`.
    pub name: String,
    /// `running`, `stopped`, `unreachable` or `not_installed`.
    pub state: String,
    /// Docker's words, or what went wrong.
    pub detail: String,
    /// When it came to be in this state (Unix seconds).
    pub since: i64,
    /// When it was last looked at (Unix seconds).
    pub checked_at: i64,
}

/// What a reading of one service means for the people who watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing to say.
    None,
    /// It is down and nobody has been told yet.
    Down,
    /// It was down and runs again.
    Recovered,
}

/// Keeps one service's reading and says whether it is news: a service that is `stopped` or
/// `unreachable` is announced once (until it runs again); one that is not installed is not a
/// failure.
///
/// # Errors
/// Database errors.
pub async fn record(
    conn: &mut PgConnection,
    node_id: Uuid,
    name: &str,
    state: &str,
    detail: &str,
) -> Result<Change, sqlx::Error> {
    let down = matches!(state, "stopped" | "unreachable");
    let before = sqlx::query!(
        "SELECT state, alerted FROM node_services WHERE node_id = $1 AND name = $2 FOR UPDATE",
        node_id,
        name
    )
    .fetch_optional(&mut *conn)
    .await?;
    let alerted_now = down;
    sqlx::query!(
        "INSERT INTO node_services (node_id, name, state, detail, alerted)
         VALUES ($1, $2, $3, left($4, 500), $5)
         ON CONFLICT (node_id, name) DO UPDATE
         SET state = EXCLUDED.state, detail = EXCLUDED.detail, checked_at = now(),
             since = CASE WHEN node_services.state = EXCLUDED.state THEN node_services.since ELSE now() END,
             alerted = EXCLUDED.alerted",
        node_id,
        name,
        state,
        detail,
        alerted_now,
    )
    .execute(&mut *conn)
    .await?;
    Ok(match (&before, down) {
        (Some(b), true) if !b.alerted => Change::Down,
        (None, true) => Change::Down,
        (Some(b), false) if b.alerted && state == "running" => Change::Recovered,
        _ => Change::None,
    })
}

/// The services of a node, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, node_id: Uuid) -> Result<Vec<NodeService>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT name, state, detail, extract(epoch FROM since)::bigint AS "since!",
                  extract(epoch FROM checked_at)::bigint AS "checked_at!"
           FROM node_services WHERE node_id = $1 ORDER BY name"#,
        node_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| NodeService {
            name: r.name,
            state: r.state,
            detail: r.detail,
            since: r.since,
            checked_at: r.checked_at,
        })
        .collect())
}
