//! The `skeleton_files` table: default files for new tenants (T04.14).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// One default file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonFile {
    /// Path below the docroot.
    pub path: String,
    /// The text.
    pub content: String,
    /// Last change (Unix seconds).
    pub updated_at: i64,
}

/// Every file, by path.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<SkeletonFile>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    list_in(&mut conn).await
}

/// [`list`] on a plain connection (inside a transaction).
///
/// # Errors
/// Database errors.
pub async fn list_in(conn: &mut PgConnection) -> Result<Vec<SkeletonFile>, sqlx::Error> {
    sqlx::query_as!(
        SkeletonFile,
        r#"SELECT path, content, extract(epoch FROM updated_at)::bigint AS "updated_at!"
           FROM skeleton_files ORDER BY path"#
    )
    .fetch_all(conn)
    .await
}

/// Adds a file or replaces its content.
///
/// # Errors
/// Database errors (a path or size the table refuses is a check violation).
pub async fn put(
    conn: &mut PgConnection,
    path: &str,
    content: &str,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO skeleton_files (path, content, updated_by) VALUES ($1, $2, $3)
         ON CONFLICT (path) DO UPDATE SET content = $2, updated_by = $3",
        path,
        content,
        by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes a file. Returns whether it existed.
///
/// # Errors
/// Database errors.
pub async fn delete(conn: &mut PgConnection, path: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!("DELETE FROM skeleton_files WHERE path = $1", path)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}
