//! The `wp_advisories` table (T11.10): the panel's own list of WordPress versions known to be
//! vulnerable.

use sqlx::{PgPool, types::Uuid};

/// One advisory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advisory {
    /// Id.
    pub id: Uuid,
    /// `core`, `plugin` or `theme`.
    pub component: String,
    /// The folder name (`wordpress` for the core).
    pub slug: String,
    /// Versions below this are affected.
    pub fixed_in: String,
    /// `low`, `medium`, `high` or `critical`.
    pub severity: String,
    /// What it is.
    pub title: String,
    /// Where to read more.
    pub reference: Option<String>,
}

/// Most advisories the list may hold.
pub const MAX: i64 = 20_000;

/// Why an advisory could not be added.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// It is there.
    #[error("already listed")]
    Exists,
    /// The list is full.
    #[error("the list is full")]
    Full,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Every advisory, by component and slug.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Advisory>, sqlx::Error> {
    sqlx::query_as!(
        Advisory,
        "SELECT id, component, slug, fixed_in, severity, title, reference
         FROM wp_advisories ORDER BY component, lower(slug), fixed_in, title LIMIT 20000"
    )
    .fetch_all(pool)
    .await
}

/// Adds an advisory.
///
/// # Errors
/// [`AddError::Exists`], [`AddError::Full`].
pub async fn add(pool: &PgPool, a: &Advisory, by: Uuid) -> Result<Uuid, AddError> {
    let n = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM wp_advisories"#)
        .fetch_one(pool)
        .await?;
    if n >= MAX {
        return Err(AddError::Full);
    }
    sqlx::query_scalar!(
        "INSERT INTO wp_advisories (component, slug, fixed_in, severity, title, reference, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        a.component,
        a.slug,
        a.fixed_in,
        a.severity,
        a.title,
        a.reference,
        by
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if e.as_database_error().and_then(|d| d.constraint()) == Some("wp_advisories_unique_idx") {
            AddError::Exists
        } else {
            AddError::Database(e)
        }
    })
}

/// Removes an advisory. `false` when there was none.
///
/// # Errors
/// Database errors.
pub async fn remove(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!("DELETE FROM wp_advisories WHERE id = $1", id)
        .execute(pool)
        .await?
        .rows_affected()
        == 1)
}
