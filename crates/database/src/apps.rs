//! The `apps` table (T12.01): which application runs on which domain and path.

use sqlx::{PgPool, types::Uuid};

/// One application.
#[derive(Debug, Clone)]
pub struct App {
    /// Id.
    pub id: Uuid,
    /// The hosting account.
    pub tenant_id: Uuid,
    /// The domain it is served on (`None` when the domain was removed).
    pub domain_id: Option<Uuid>,
    /// The domain's name, when there is one.
    pub fqdn: Option<String>,
    /// The path under the domain (`/`, `/blog`).
    pub path: String,
    /// The catalog slug.
    pub kind: String,
    /// What the owner calls it.
    pub name: String,
    /// The version installed.
    pub version: Option<String>,
    /// `installing`, `active`, `failed` or `removing`.
    pub status: String,
    /// The database it uses.
    pub database_id: Option<Uuid>,
    /// Settings of the kind.
    pub settings: serde_json::Value,
    /// Why an install failed.
    pub error: Option<String>,
    /// Created (Unix seconds).
    pub created_at: i64,
}

/// What to track.
#[derive(Debug, Clone)]
pub struct NewApp<'a> {
    /// The hosting account.
    pub tenant_id: Uuid,
    /// The domain (it must belong to the account).
    pub domain_id: Option<Uuid>,
    /// The path under the domain.
    pub path: &'a str,
    /// The catalog slug.
    pub kind: &'a str,
    /// A name.
    pub name: &'a str,
    /// The version.
    pub version: Option<&'a str>,
    /// The database it uses (it must belong to the account).
    pub database_id: Option<Uuid>,
    /// Settings of the kind.
    pub settings: &'a serde_json::Value,
}

/// Why an application could not be tracked.
#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    /// Another application is in that place.
    #[error("the place is taken")]
    Taken,
    /// The domain or the database is not the account's.
    #[error("not the account's")]
    NotOwned,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// An account's applications, oldest first; only those on `domain_id` when it is given.
///
/// # Errors
/// Database errors.
pub async fn list(
    pool: &PgPool,
    tenant_id: Uuid,
    domain_id: Option<Uuid>,
) -> Result<Vec<App>, sqlx::Error> {
    sqlx::query_as!(
        App,
        r#"SELECT a.id, a.tenant_id, a.domain_id, d.fqdn AS "fqdn?", a.path, a.kind, a.name,
                  a.version, a.status, a.database_id, a.settings, a.error,
                  extract(epoch FROM a.created_at)::bigint AS "created_at!"
           FROM apps a LEFT JOIN domains d ON d.id = a.domain_id
           WHERE a.tenant_id = $1 AND ($2::uuid IS NULL OR a.domain_id = $2)
           ORDER BY a.created_at, a.id"#,
        tenant_id,
        domain_id
    )
    .fetch_all(pool)
    .await
}

/// One of an account's applications.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<App>, sqlx::Error> {
    sqlx::query_as!(
        App,
        r#"SELECT a.id, a.tenant_id, a.domain_id, d.fqdn AS "fqdn?", a.path, a.kind, a.name,
                  a.version, a.status, a.database_id, a.settings, a.error,
                  extract(epoch FROM a.created_at)::bigint AS "created_at!"
           FROM apps a LEFT JOIN domains d ON d.id = a.domain_id
           WHERE a.tenant_id = $1 AND a.id = $2"#,
        tenant_id,
        id
    )
    .fetch_optional(pool)
    .await
}

/// Starts tracking an application (status `installing`).
///
/// # Errors
/// [`CreateError::Taken`], [`CreateError::NotOwned`].
pub async fn create(pool: &PgPool, new: &NewApp<'_>) -> Result<Uuid, CreateError> {
    if let Some(domain) = new.domain_id {
        let ok = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM domains WHERE id = $1 AND tenant_id = $2) AS "e!""#,
            domain,
            new.tenant_id
        )
        .fetch_one(pool)
        .await?;
        if !ok {
            return Err(CreateError::NotOwned);
        }
    }
    if let Some(database) = new.database_id {
        let ok = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM databases WHERE id = $1 AND tenant_id = $2) AS "e!""#,
            database,
            new.tenant_id
        )
        .fetch_one(pool)
        .await?;
        if !ok {
            return Err(CreateError::NotOwned);
        }
    }
    sqlx::query_scalar!(
        "INSERT INTO apps (tenant_id, domain_id, path, kind, name, version, database_id, settings)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        new.tenant_id,
        new.domain_id,
        new.path,
        new.kind,
        new.name,
        new.version,
        new.database_id,
        new.settings
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if e.as_database_error().and_then(|d| d.constraint()) == Some("apps_place_idx") {
            CreateError::Taken
        } else {
            CreateError::Database(e)
        }
    })
}

/// Moves an application to another status (`error` for a failure).
///
/// # Errors
/// Database errors.
pub async fn set_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    version: Option<&str>,
    error: Option<&str>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE apps SET status = $2, version = COALESCE($3, version), error = $4, updated_at = now()
         WHERE id = $1",
        id,
        status,
        version,
        error
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

/// Stops tracking an application. `false` when there was none.
///
/// # Errors
/// Database errors.
pub async fn remove(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM apps WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}
