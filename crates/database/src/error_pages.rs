//! Pages shown when a site cannot answer (T06.13).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// The pages an administrator wrote: `(kind, html)`, by kind.
///
/// # Errors
/// Database errors.
pub async fn custom(pool: &PgPool) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = sqlx::query!("SELECT kind, body FROM error_pages ORDER BY kind")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.kind, r.body)).collect())
}

/// Keeps a page.
///
/// # Errors
/// Database errors.
pub async fn set(
    conn: &mut PgConnection,
    kind: &str,
    body: &str,
    by: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO error_pages (kind, body, updated_by) VALUES ($1, $2, $3)
         ON CONFLICT (kind) DO UPDATE SET body = EXCLUDED.body, updated_by = EXCLUDED.updated_by",
        kind,
        body,
        by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Goes back to the installation's own page. Returns whether there was one.
///
/// # Errors
/// Database errors.
pub async fn reset(conn: &mut PgConnection, kind: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!("DELETE FROM error_pages WHERE kind = $1", kind)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
