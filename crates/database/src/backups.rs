//! Backups, where they go and when they are made (T09.02).

use sqlx::{PgPool, types::Uuid};

/// A place backups are stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Destination {
    /// Id.
    pub id: Uuid,
    /// Its name.
    pub name: String,
    /// `local`, `s3` or `sftp`.
    pub kind: String,
    /// What is not secret.
    pub settings: serde_json::Value,
    /// Whether secret credentials are kept for it.
    pub has_credentials: bool,
    /// May receive backups.
    pub enabled: bool,
    /// Where new backups go unless said otherwise.
    pub is_default: bool,
    /// When it was last tested (Unix seconds).
    pub last_tested_at: Option<i64>,
    /// The result of that test.
    pub last_test_ok: Option<bool>,
    /// What was wrong.
    pub last_test_error: Option<String>,
}

/// Why a destination could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The name is used.
    #[error("the name is taken")]
    Taken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    name: String,
    kind: String,
    settings: serde_json::Value,
    has_credentials: bool,
    enabled: bool,
    is_default: bool,
    last_tested_at: Option<i64>,
    last_test_ok: Option<bool>,
    last_test_error: Option<String>,
}

impl From<Row> for Destination {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            kind: r.kind,
            settings: r.settings,
            has_credentials: r.has_credentials,
            enabled: r.enabled,
            is_default: r.is_default,
            last_tested_at: r.last_tested_at,
            last_test_ok: r.last_test_ok,
            last_test_error: r.last_test_error,
        }
    }
}

/// Every destination, by name.
///
/// # Errors
/// Database errors.
pub async fn destinations(pool: &PgPool) -> Result<Vec<Destination>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, name, kind, settings, (sealed_credentials IS NOT NULL) AS "has_credentials!",
                  enabled, is_default,
                  extract(epoch FROM last_tested_at)::bigint AS last_tested_at,
                  last_test_ok, last_test_error
           FROM backup_destinations ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Destination::from).collect())
}

/// One destination.
///
/// # Errors
/// Database errors.
pub async fn destination(pool: &PgPool, id: Uuid) -> Result<Option<Destination>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, name, kind, settings, (sealed_credentials IS NOT NULL) AS "has_credentials!",
                  enabled, is_default,
                  extract(epoch FROM last_tested_at)::bigint AS last_tested_at,
                  last_test_ok, last_test_error
           FROM backup_destinations WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Destination::from))
}

/// Makes a destination (the first one made is the default).
///
/// # Errors
/// [`SaveError::Taken`] when the name is used.
pub async fn add_destination(
    pool: &PgPool,
    name: &str,
    kind: &str,
    settings: &serde_json::Value,
    created_by: Uuid,
) -> Result<Uuid, SaveError> {
    sqlx::query_scalar!(
        "INSERT INTO backup_destinations (name, kind, settings, created_by, is_default)
         VALUES ($1, $2, $3, $4, NOT EXISTS (SELECT 1 FROM backup_destinations))
         RETURNING id",
        name,
        kind,
        settings,
        created_by,
    )
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint())
            == Some("backup_destinations_name_key")
        {
            SaveError::Taken
        } else {
            SaveError::Database(err)
        }
    })
}

/// Keeps the sealed credentials of a destination (`None` removes them).
///
/// # Errors
/// Database errors.
pub async fn keep_credentials(
    pool: &PgPool,
    id: Uuid,
    sealed: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backup_destinations SET sealed_credentials = $2, updated_at = now() WHERE id = $1",
        id,
        sealed,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The sealed credentials of a destination.
///
/// # Errors
/// Database errors.
pub async fn sealed_credentials(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT sealed_credentials FROM backup_destinations WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

/// What sealed credentials are sealed to.
#[must_use]
pub fn credentials_context(id: Uuid) -> Vec<u8> {
    format!("backup-destination:{id}").into_bytes()
}

/// Changes what is not secret about a destination.
///
/// # Errors
/// [`SaveError::Taken`] when the name is used.
pub async fn change_destination(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    settings: &serde_json::Value,
    enabled: bool,
) -> Result<bool, SaveError> {
    let done = sqlx::query!(
        "UPDATE backup_destinations SET name = $2, settings = $3, enabled = $4, updated_at = now() WHERE id = $1",
        id,
        name,
        settings,
        enabled,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint()) == Some("backup_destinations_name_key") {
            SaveError::Taken
        } else {
            SaveError::Database(err)
        }
    })?;
    Ok(done.rows_affected() == 1)
}

/// Makes a destination the default.
///
/// # Errors
/// Database errors.
pub async fn make_default(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!("UPDATE backup_destinations SET is_default = false WHERE is_default")
        .execute(&mut *tx)
        .await?;
    let done = sqlx::query!(
        "UPDATE backup_destinations SET is_default = true WHERE id = $1 AND enabled",
        id,
    )
    .execute(&mut *tx)
    .await?;
    if done.rows_affected() == 1 {
        tx.commit().await?;
        Ok(true)
    } else {
        tx.rollback().await?;
        Ok(false)
    }
}

/// Removes a destination nothing refers to; `false` when backups or schedules use it.
///
/// # Errors
/// Database errors.
pub async fn remove_destination(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM backup_destinations d WHERE d.id = $1
           AND NOT EXISTS (SELECT 1 FROM backups b WHERE b.destination_id = d.id AND b.status <> 'deleted')
           AND NOT EXISTS (SELECT 1 FROM backup_schedules s WHERE s.destination_id = d.id)",
        id,
    )
    .execute(pool)
    .await;
    match done {
        Ok(d) => Ok(d.rows_affected() == 1),
        // Deleted backups still point at it: the history keeps it.
        Err(err)
            if err
                .as_database_error()
                .is_some_and(|e| e.is_foreign_key_violation()) =>
        {
            Ok(false)
        }
        Err(err) => Err(err),
    }
}

/// Keeps the result of a test.
///
/// # Errors
/// Database errors.
pub async fn record_test(
    pool: &PgPool,
    id: Uuid,
    ok: bool,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE backup_destinations SET last_tested_at = now(), last_test_ok = $2, last_test_error = left($3, 500)
         WHERE id = $1",
        id,
        ok,
        error,
    )
    .execute(pool)
    .await?;
    Ok(())
}
