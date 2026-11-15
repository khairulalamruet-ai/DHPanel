//! The `blocked_user_agents` table (T11.05).

use sqlx::{PgPool, types::Uuid};

/// One blocked text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    /// Id.
    pub id: Uuid,
    /// The text a User-Agent contains.
    pub pattern: String,
    /// Why.
    pub note: Option<String>,
    /// When it was added (Unix seconds).
    pub created_at: i64,
}

/// Most texts the list may hold (the nodes' limit).
pub const MAX: i64 = 500;

/// Why a text could not be added.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// It is there (in any case).
    #[error("already listed")]
    Exists,
    /// The list is full.
    #[error("the list is full")]
    Full,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The list, oldest first.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Blocked>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT id, pattern, note, extract(epoch FROM created_at)::bigint AS "at!"
           FROM blocked_user_agents ORDER BY created_at, id"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Blocked {
            id: r.id,
            pattern: r.pattern,
            note: r.note,
            created_at: r.at,
        })
        .collect())
}

/// Adds a text.
///
/// # Errors
/// [`AddError::Exists`], [`AddError::Full`].
pub async fn add(
    pool: &PgPool,
    pattern: &str,
    note: Option<&str>,
    by: Uuid,
) -> Result<Uuid, AddError> {
    let mut tx = pool.begin().await?;
    sqlx::query!("LOCK TABLE blocked_user_agents IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM blocked_user_agents"#)
        .fetch_one(&mut *tx)
        .await?;
    if n >= MAX {
        return Err(AddError::Full);
    }
    let id = sqlx::query_scalar!(
        "INSERT INTO blocked_user_agents (pattern, note, created_by) VALUES ($1, $2, $3) RETURNING id",
        pattern,
        note,
        by,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if e.as_database_error().and_then(|d| d.constraint()) == Some("blocked_user_agents_pattern_idx") {
            AddError::Exists
        } else {
            AddError::Database(e)
        }
    })?;
    tx.commit().await?;
    Ok(id)
}

/// Removes a text. `false` when there was none.
///
/// # Errors
/// Database errors.
pub async fn remove(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query!("DELETE FROM blocked_user_agents WHERE id = $1", id)
            .execute(pool)
            .await?
            .rows_affected()
            == 1,
    )
}
