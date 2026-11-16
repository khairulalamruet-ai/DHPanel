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

/// What an installer's secret is sealed to: that application and that name.
#[must_use]
pub fn secret_context(app_id: Uuid, name: &str) -> Vec<u8> {
    format!("app-secret:{app_id}:{name}").into_bytes()
}

/// Keeps a sealed secret of an application.
///
/// # Errors
/// Database errors.
pub async fn keep_secret(
    conn: &mut sqlx::PgConnection,
    app_id: Uuid,
    name: &str,
    sealed: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO app_secrets (app_id, name, sealed) VALUES ($1, $2, $3)
         ON CONFLICT (app_id, name) DO UPDATE SET sealed = EXCLUDED.sealed",
        app_id,
        name,
        sealed
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A sealed secret of an application (`None` once the install ended).
///
/// # Errors
/// Database errors.
pub async fn sealed_secret(
    pool: &PgPool,
    app_id: Uuid,
    name: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT sealed FROM app_secrets WHERE app_id = $1 AND name = $2",
        app_id,
        name
    )
    .fetch_optional(pool)
    .await
}

/// Forgets one secret of an application.
///
/// # Errors
/// Database errors.
pub async fn forget_secret(pool: &PgPool, app_id: Uuid, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM app_secrets WHERE app_id = $1 AND name = $2",
        app_id,
        name
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Forgets the secrets of an application.
///
/// # Errors
/// Database errors.
pub async fn forget_secrets(pool: &PgPool, app_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM app_secrets WHERE app_id = $1", app_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// An application whose install is under way, with what the dispatcher needs.
#[derive(Debug, Clone)]
pub struct Installing {
    /// The application.
    pub id: Uuid,
    /// The account.
    pub tenant_id: Uuid,
    /// Its node.
    pub node_id: Uuid,
    /// The account's system name.
    pub username: String,
    /// The account's numeric user id.
    pub uid: i32,
    /// The path under the domain.
    pub path: String,
    /// The catalog slug.
    pub kind: String,
    /// The domain's name.
    pub fqdn: Option<String>,
    /// Where the install stands (and the ids it needs).
    pub settings: serde_json::Value,
}

/// Applications being installed, oldest first.
///
/// # Errors
/// Database errors.
pub async fn installing(pool: &PgPool, limit: i64) -> Result<Vec<Installing>, sqlx::Error> {
    sqlx::query_as!(
        Installing,
        r#"SELECT a.id, a.tenant_id, t.node_id, t.username, t.uid, a.path, a.kind,
                  d.fqdn AS "fqdn?", a.settings
           FROM apps a JOIN tenants t ON t.id = a.tenant_id LEFT JOIN domains d ON d.id = a.domain_id
           WHERE a.status = 'installing'
           ORDER BY a.created_at LIMIT $1"#,
        limit
    )
    .fetch_all(pool)
    .await
}

/// Replaces an application's settings (where its install stands).
///
/// # Errors
/// Database errors.
pub async fn set_settings(
    pool: &PgPool,
    id: Uuid,
    settings: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE apps SET settings = $2, updated_at = now() WHERE id = $1",
        id,
        settings
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The next number for a name like `alice_wp<n>`: one more than the highest taken.
///
/// # Errors
/// Database errors.
pub async fn next_name_number(
    pool: &PgPool,
    tenant_id: Uuid,
    stem: &str,
) -> Result<i64, sqlx::Error> {
    let highest = sqlx::query_scalar!(
        r#"SELECT max(substring(name FROM char_length($2) + 1)::bigint) AS "n"
           FROM (SELECT name FROM databases WHERE tenant_id = $1 AND name ~ ('^' || $2 || '[0-9]+$')
                 UNION ALL
                 SELECT name FROM db_users WHERE tenant_id = $1 AND name ~ ('^' || $2 || '[0-9]+$')) x"#,
        tenant_id,
        stem
    )
    .fetch_one(pool)
    .await?;
    Ok(highest.unwrap_or(0) + 1)
}

/// The database and the user an install made: their names and where they stand.
#[derive(Debug, Clone)]
pub struct SetupState {
    /// The database's name.
    pub database: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub database_status: String,
    /// The user's name.
    pub user: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub user_status: String,
}

/// Where the database and the user of an install stand (`None` when either is gone).
///
/// # Errors
/// Database errors.
pub async fn setup_state(
    pool: &PgPool,
    database_id: Uuid,
    user_id: Uuid,
) -> Result<Option<SetupState>, sqlx::Error> {
    sqlx::query_as!(
        SetupState,
        r#"SELECT d.name AS "database!", d.status AS "database_status!",
                  u.name AS "user!", u.status AS "user_status!"
           FROM databases d, db_users u WHERE d.id = $1 AND u.id = $2"#,
        database_id,
        user_id
    )
    .fetch_optional(pool)
    .await
}
