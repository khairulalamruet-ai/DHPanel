//! The `runtime_apps` table (T12.07, T12.08): a tenant's own Node.js and Python applications.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// One application of an account.
#[derive(Debug, Clone)]
pub struct RuntimeApp {
    /// Id.
    pub id: Uuid,
    /// The account.
    pub tenant_id: Uuid,
    /// Its short name.
    pub name: String,
    /// `node` or `python`.
    pub kind: String,
    /// The runtime's version (`24`, `3.13`).
    pub version: String,
    /// The folder below the web directory.
    pub dir: String,
    /// What starts it.
    pub start_command: String,
    /// Whether the install step runs first.
    pub install: bool,
    /// The port it listens on.
    pub port: i32,
    /// Memory limit (MiB).
    pub memory_mb: i32,
    /// The sealed environment.
    pub sealed_env: Option<Vec<u8>>,
    /// The names of the variables.
    pub env_names: Vec<String>,
    /// The domain that shows it.
    pub domain_id: Option<Uuid>,
    /// `pending`, `ready`, `failed` or `removing`.
    pub status: String,
    /// The operation awaited.
    pub op_id: Option<Uuid>,
    /// Where the web server reaches it (`name:port`).
    pub host: Option<String>,
    /// Why it failed.
    pub error: Option<String>,
}

/// What a sealed environment is sealed to: that application and nothing else.
#[must_use]
pub fn env_context(id: Uuid) -> Vec<u8> {
    format!("runtime-app-env:{id}").into_bytes()
}

/// An account's applications, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<RuntimeApp>, sqlx::Error> {
    sqlx::query_as!(
        RuntimeApp,
        "SELECT id, tenant_id, name, kind, version, dir, start_command, install, port, memory_mb,
                sealed_env, env_names, domain_id, status, op_id, host, error
         FROM runtime_apps WHERE tenant_id = $1 ORDER BY name",
        tenant_id
    )
    .fetch_all(pool)
    .await
}

/// One application of an account.
///
/// # Errors
/// Database errors.
pub async fn get(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<RuntimeApp>, sqlx::Error> {
    sqlx::query_as!(
        RuntimeApp,
        "SELECT id, tenant_id, name, kind, version, dir, start_command, install, port, memory_mb,
                sealed_env, env_names, domain_id, status, op_id, host, error
         FROM runtime_apps WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id
    )
    .fetch_optional(pool)
    .await
}

/// One application of an account, inside a transaction.
///
/// # Errors
/// Database errors.
pub async fn get_in(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<RuntimeApp>, sqlx::Error> {
    sqlx::query_as!(
        RuntimeApp,
        "SELECT id, tenant_id, name, kind, version, dir, start_command, install, port, memory_mb,
                sealed_env, env_names, domain_id, status, op_id, host, error
         FROM runtime_apps WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id
    )
    .fetch_optional(conn)
    .await
}

/// How many applications an account has (not counting those being removed).
///
/// # Errors
/// Database errors.
pub async fn count(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM runtime_apps WHERE tenant_id = $1 AND status <> 'removing'"#,
        tenant_id
    )
    .fetch_one(conn)
    .await
}

/// Memory the account's applications may use together (MiB), except one.
///
/// # Errors
/// Database errors.
pub async fn memory_used(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    except: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(sum(memory_mb), 0)::bigint AS "n!" FROM runtime_apps
           WHERE tenant_id = $1 AND status <> 'removing' AND ($2::uuid IS NULL OR id <> $2)"#,
        tenant_id,
        except
    )
    .fetch_one(conn)
    .await
}

/// What to store for an application.
#[derive(Debug, Clone)]
pub struct Write<'a> {
    /// Short name.
    pub name: &'a str,
    /// `node` or `python`.
    pub kind: &'a str,
    /// Runtime version.
    pub version: &'a str,
    /// Folder.
    pub dir: &'a str,
    /// Start command.
    pub start_command: &'a str,
    /// Run the install step first.
    pub install: bool,
    /// Port.
    pub port: i32,
    /// Memory (MiB).
    pub memory_mb: i32,
    /// Domain.
    pub domain_id: Option<Uuid>,
    /// Names of the environment variables.
    pub env_names: &'a [String],
}

/// Why a write failed.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The name or the domain is taken.
    #[error("an application with this name exists, or the domain has one")]
    Taken,
    /// The domain is not the account's.
    #[error("the domain is not one of the account's")]
    UnknownDomain,
    /// Anything else.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn classify(e: sqlx::Error) -> WriteError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => WriteError::Taken,
        sqlx::Error::Database(d) if d.is_foreign_key_violation() => WriteError::UnknownDomain,
        _ => WriteError::Database(e),
    }
}

/// Makes an application (pending). Returns its id.
///
/// # Errors
/// [`WriteError`].
pub async fn insert(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    w: &Write<'_>,
) -> Result<Uuid, WriteError> {
    if let Some(domain) = w.domain_id {
        let own = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM domains WHERE id = $1 AND tenant_id = $2) AS "own!""#,
            domain,
            tenant_id
        )
        .fetch_one(&mut *conn)
        .await?;
        if !own {
            return Err(WriteError::UnknownDomain);
        }
    }
    sqlx::query_scalar!(
        "INSERT INTO runtime_apps (tenant_id, name, kind, version, dir, start_command, install,
                                   port, memory_mb, domain_id, env_names)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
        tenant_id,
        w.name,
        w.kind,
        w.version,
        w.dir,
        w.start_command,
        w.install,
        w.port,
        w.memory_mb,
        w.domain_id,
        w.env_names,
    )
    .fetch_one(conn)
    .await
    .map_err(classify)
}

/// Changes an application (pending again); the name and the kind stay.
///
/// # Errors
/// [`WriteError`].
pub async fn update(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
    w: &Write<'_>,
) -> Result<bool, WriteError> {
    if let Some(domain) = w.domain_id {
        let own = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM domains WHERE id = $1 AND tenant_id = $2) AS "own!""#,
            domain,
            tenant_id
        )
        .fetch_one(&mut *conn)
        .await?;
        if !own {
            return Err(WriteError::UnknownDomain);
        }
    }
    let done = sqlx::query!(
        "UPDATE runtime_apps
            SET version = $3, dir = $4, start_command = $5, install = $6, port = $7,
                memory_mb = $8, domain_id = $9, env_names = $10,
                status = 'pending', error = NULL, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status <> 'removing'",
        tenant_id,
        id,
        w.version,
        w.dir,
        w.start_command,
        w.install,
        w.port,
        w.memory_mb,
        w.domain_id,
        w.env_names,
    )
    .execute(conn)
    .await
    .map_err(classify)?;
    Ok(done.rows_affected() == 1)
}

/// Stores the sealed environment.
///
/// # Errors
/// Database errors.
pub async fn set_env(conn: &mut PgConnection, id: Uuid, sealed: &[u8]) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE runtime_apps SET sealed_env = $2 WHERE id = $1",
        id,
        sealed
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The sealed environment.
///
/// # Errors
/// Database errors.
pub async fn sealed_env(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT sealed_env FROM runtime_apps WHERE id = $1", id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// Notes the operation a row waits for and its status.
///
/// # Errors
/// Database errors.
pub async fn waiting_for(
    conn: &mut PgConnection,
    id: Uuid,
    op_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE runtime_apps SET op_id = $2, status = $3, updated_at = now() WHERE id = $1",
        id,
        op_id,
        status
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Rows whose operation answer is awaited.
///
/// # Errors
/// Database errors.
pub async fn awaiting(pool: &PgPool) -> Result<Vec<RuntimeApp>, sqlx::Error> {
    sqlx::query_as!(
        RuntimeApp,
        "SELECT id, tenant_id, name, kind, version, dir, start_command, install, port, memory_mb,
                sealed_env, env_names, domain_id, status, op_id, host, error
         FROM runtime_apps WHERE op_id IS NOT NULL AND status IN ('pending', 'removing')"
    )
    .fetch_all(pool)
    .await
}

/// The node said it is up.
///
/// # Errors
/// Database errors.
pub async fn ready(pool: &PgPool, id: Uuid, host: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE runtime_apps SET status = 'ready', op_id = NULL, host = $2, error = NULL, updated_at = now()
         WHERE id = $1",
        id,
        host
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The node could not make it.
///
/// # Errors
/// Database errors.
pub async fn failed(pool: &PgPool, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error
        .chars()
        .filter(|c| !c.is_control())
        .take(500)
        .collect();
    sqlx::query!(
        "UPDATE runtime_apps SET status = 'failed', op_id = NULL, error = $2, updated_at = now() WHERE id = $1",
        id,
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The node removed it: the row goes.
///
/// # Errors
/// Database errors.
pub async fn forget(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM runtime_apps WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}
