//! Web server configuration templates (T06.03).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VhostTemplate {
    /// Id.
    pub id: Uuid,
    /// The web server it is for (`nginx`).
    pub server: String,
    /// Its name.
    pub name: String,
    /// The configuration with `{{variables}}`.
    pub body: String,
    /// Whether new sites use it.
    pub is_default: bool,
    /// Which save it is (1 for the first).
    pub version: i32,
    /// Last change (Unix seconds).
    pub updated_at: i64,
}

/// The template new sites of `server` use.
///
/// # Errors
/// Database errors.
pub async fn default_for(
    pool: &PgPool,
    server: &str,
) -> Result<Option<VhostTemplate>, sqlx::Error> {
    sqlx::query_as!(
        VhostTemplate,
        r#"SELECT id, server, name, body, is_default, version,
                  EXTRACT(EPOCH FROM updated_at)::bigint AS "updated_at!"
           FROM vhost_templates WHERE server = $1 AND is_default"#,
        server,
    )
    .fetch_optional(pool)
    .await
}

/// Why a write was refused.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// Another template of the server has the name.
    #[error("a template with this name exists")]
    NameTaken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn classify(err: sqlx::Error) -> WriteError {
    match &err {
        sqlx::Error::Database(db) if db.constraint() == Some("vhost_templates_server_name_key") => {
            WriteError::NameTaken
        }
        _ => WriteError::Database(err),
    }
}

/// Every template of the web server, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, server: &str) -> Result<Vec<VhostTemplate>, sqlx::Error> {
    sqlx::query_as!(
        VhostTemplate,
        r#"SELECT id, server, name, body, is_default, version,
                  EXTRACT(EPOCH FROM updated_at)::bigint AS "updated_at!"
           FROM vhost_templates WHERE server = $1 ORDER BY name"#,
        server,
    )
    .fetch_all(pool)
    .await
}

/// One template.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<VhostTemplate>, sqlx::Error> {
    sqlx::query_as!(
        VhostTemplate,
        r#"SELECT id, server, name, body, is_default, version,
                  EXTRACT(EPOCH FROM updated_at)::bigint AS "updated_at!"
           FROM vhost_templates WHERE id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// Adds a template (version 1) and returns its id.
///
/// # Errors
/// [`WriteError`].
pub async fn create(
    conn: &mut PgConnection,
    name: &str,
    body: &str,
    changed_by: Option<Uuid>,
) -> Result<Uuid, WriteError> {
    let id = sqlx::query_scalar!(
        "INSERT INTO vhost_templates (name, body) VALUES ($1, $2) RETURNING id",
        name,
        body,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(classify)?;
    sqlx::query!(
        "INSERT INTO vhost_template_versions (template_id, version, name, body, changed_by)
         VALUES ($1, 1, $2, $3, $4)",
        id,
        name,
        body,
        changed_by,
    )
    .execute(conn)
    .await?;
    Ok(id)
}

/// Saves a template as its next version. Returns the new version, or `None` for
/// no such template. Saving what is already there changes nothing.
///
/// # Errors
/// [`WriteError`].
pub async fn update(
    conn: &mut PgConnection,
    id: Uuid,
    name: &str,
    body: &str,
    changed_by: Option<Uuid>,
) -> Result<Option<i32>, WriteError> {
    let Some(current) = sqlx::query!(
        "SELECT name, body, version FROM vhost_templates WHERE id = $1 FOR UPDATE",
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    if current.name == name && current.body == body {
        return Ok(Some(current.version));
    }
    let version = current.version + 1;
    sqlx::query!(
        "UPDATE vhost_templates SET name = $2, body = $3, version = $4 WHERE id = $1",
        id,
        name,
        body,
        version,
    )
    .execute(&mut *conn)
    .await
    .map_err(classify)?;
    sqlx::query!(
        "INSERT INTO vhost_template_versions (template_id, version, name, body, changed_by)
         VALUES ($1, $2, $3, $4, $5)",
        id,
        version,
        name,
        body,
        changed_by,
    )
    .execute(conn)
    .await?;
    Ok(Some(version))
}

/// Removes a template that is not the default. Returns whether one was removed.
///
/// # Errors
/// Database errors.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM vhost_templates WHERE id = $1 AND NOT is_default",
        id
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Makes a template the one new sites use. Returns whether it exists.
///
/// # Errors
/// Database errors.
pub async fn make_default(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let server = sqlx::query_scalar!("SELECT server FROM vhost_templates WHERE id = $1", id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(server) = server else {
        return Ok(false);
    };
    sqlx::query!(
        "UPDATE vhost_templates SET is_default = false WHERE server = $1 AND is_default AND id <> $2",
        server,
        id,
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE vhost_templates SET is_default = true WHERE id = $1",
        id
    )
    .execute(conn)
    .await?;
    Ok(true)
}

/// One saved version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Which save.
    pub version: i32,
    /// The name then.
    pub name: String,
    /// The text then.
    pub body: String,
    /// Who saved it, if still known.
    pub changed_by: Option<Uuid>,
    /// When (Unix seconds).
    pub created_at: i64,
}

/// A template's history, newest first (at most 50).
///
/// # Errors
/// Database errors.
pub async fn versions(pool: &PgPool, id: Uuid) -> Result<Vec<Version>, sqlx::Error> {
    sqlx::query_as!(
        Version,
        r#"SELECT version, name, body, changed_by,
                  EXTRACT(EPOCH FROM created_at)::bigint AS "created_at!"
           FROM vhost_template_versions WHERE template_id = $1
           ORDER BY version DESC LIMIT 50"#,
        id,
    )
    .fetch_all(pool)
    .await
}

/// Makes every tenant that has a web server send its files again: what the
/// dispatcher does for a tenant whose routes are out of date, and a changed
/// template changes every file. Returns how many tenants.
///
/// # Errors
/// Database errors.
pub async fn resend_all(conn: &mut PgConnection) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE tenants SET routes_version = routes_version + 1
         WHERE status IN ('active', 'suspended')"
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}
