//! The `container_images` table (T12.12): the images clients may run as containers.
//!
//! Plain queries, so this table needs no offline cache.

use sqlx::{PgConnection, PgPool, Row as _, postgres::PgRow, types::Uuid};

/// One approved image.
#[derive(Debug, Clone)]
pub struct Image {
    /// Id.
    pub id: Uuid,
    /// What clients see.
    pub name: String,
    /// `repo@sha256:<digest>`.
    pub reference: String,
    /// The port it listens on.
    pub default_port: i32,
    /// The memory it starts with (MiB).
    pub default_memory_mb: i32,
    /// A note for clients.
    pub note: String,
    /// Whether clients may choose it.
    pub enabled: bool,
}

const COLUMNS: &str = "id, name, reference, default_port, default_memory_mb, note, enabled";

fn row(r: &PgRow) -> Result<Image, sqlx::Error> {
    Ok(Image {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        reference: r.try_get("reference")?,
        default_port: r.try_get("default_port")?,
        default_memory_mb: r.try_get("default_memory_mb")?,
        note: r.try_get("note")?,
        enabled: r.try_get("enabled")?,
    })
}

/// What to store for an image.
#[derive(Debug)]
pub struct Write<'a> {
    /// Name.
    pub name: &'a str,
    /// Reference.
    pub reference: &'a str,
    /// Port.
    pub default_port: i32,
    /// Memory (MiB).
    pub default_memory_mb: i32,
    /// Note.
    pub note: &'a str,
    /// Enabled.
    pub enabled: bool,
}

/// Why a write failed.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The name or the reference is already in the catalog.
    #[error("the name or the image is already in the catalog")]
    Taken,
    /// Anything else.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn classify(e: sqlx::Error) -> WriteError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => WriteError::Taken,
        _ => WriteError::Database(e),
    }
}

/// The catalog, by name; `only_enabled` for what a client may choose.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, only_enabled: bool) -> Result<Vec<Image>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM container_images WHERE ($1 = false OR enabled) ORDER BY name"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(only_enabled)
        .fetch_all(pool)
        .await?
        .iter()
        .map(row)
        .collect()
}

/// One image.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Image>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM container_images WHERE id = $1");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row)
        .transpose()
}

/// One image, inside a transaction.
///
/// # Errors
/// Database errors.
pub async fn get_in(conn: &mut PgConnection, id: Uuid) -> Result<Option<Image>, sqlx::Error> {
    let sql = format!("SELECT {COLUMNS} FROM container_images WHERE id = $1");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .as_ref()
        .map(row)
        .transpose()
}

/// Adds an image to the catalog.
///
/// # Errors
/// [`WriteError`].
pub async fn insert(pool: &PgPool, w: &Write<'_>) -> Result<Image, WriteError> {
    let sql = format!(
        "INSERT INTO container_images (name, reference, default_port, default_memory_mb, note, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    );
    let r = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(w.name)
        .bind(w.reference)
        .bind(w.default_port)
        .bind(w.default_memory_mb)
        .bind(w.note)
        .bind(w.enabled)
        .fetch_one(pool)
        .await
        .map_err(classify)?;
    Ok(row(&r)?)
}

/// Changes what clients see of an image and whether they may choose it. The image itself (its
/// reference) never changes: add another and retire this one.
///
/// # Errors
/// [`WriteError`].
pub async fn update(pool: &PgPool, id: Uuid, w: &Write<'_>) -> Result<bool, WriteError> {
    let done = sqlx::query(
        "UPDATE container_images SET name = $2, default_port = $3, default_memory_mb = $4, \
         note = $5, enabled = $6, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(w.name)
    .bind(w.default_port)
    .bind(w.default_memory_mb)
    .bind(w.note)
    .bind(w.enabled)
    .execute(pool)
    .await
    .map_err(classify)?;
    Ok(done.rows_affected() == 1)
}

/// Why an image could not be removed.
#[derive(Debug, PartialEq, Eq)]
pub enum Removed {
    /// Removed.
    Yes,
    /// There is none.
    NoSuchImage,
    /// A container still runs it.
    InUse,
}

/// Removes an image nobody runs.
///
/// # Errors
/// Database errors.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<Removed, sqlx::Error> {
    match sqlx::query("DELETE FROM container_images WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(done) if done.rows_affected() == 1 => Ok(Removed::Yes),
        Ok(_) => Ok(Removed::NoSuchImage),
        // RESTRICT answers 23001, a plain foreign key 23503.
        Err(sqlx::Error::Database(d))
            if d.is_foreign_key_violation() || d.code().as_deref() == Some("23001") =>
        {
            Ok(Removed::InUse)
        }
        Err(e) => Err(e),
    }
}

/// Records which image a container application runs.
///
/// # Errors
/// Database errors.
pub async fn set_for_app(
    conn: &mut PgConnection,
    app_id: Uuid,
    image_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO runtime_app_images (app_id, image_id) VALUES ($1, $2) \
         ON CONFLICT (app_id) DO UPDATE SET image_id = EXCLUDED.image_id",
    )
    .bind(app_id)
    .bind(image_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The image a container application runs.
///
/// # Errors
/// Database errors.
pub async fn of_app(conn: &mut PgConnection, app_id: Uuid) -> Result<Option<Image>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM container_images WHERE id = \
         (SELECT image_id FROM runtime_app_images WHERE app_id = $1)"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(app_id)
        .fetch_optional(conn)
        .await?
        .as_ref()
        .map(row)
        .transpose()
}

/// The images of several applications: app id to image id.
///
/// # Errors
/// Database errors.
pub async fn of_apps(pool: &PgPool, app_ids: &[Uuid]) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT app_id, image_id FROM runtime_app_images WHERE app_id = ANY($1)",
    )
    .bind(app_ids)
    .fetch_all(pool)
    .await
}
