//! The `cache_instances` table (T12.10).

use sqlx::{PgPool, types::Uuid};

/// One cache server of an account.
#[derive(Debug, Clone)]
pub struct Instance {
    /// Id.
    pub id: Uuid,
    /// The account.
    pub tenant_id: Uuid,
    /// `redis` or `memcached`.
    pub kind: String,
    /// Memory it may use (MiB).
    pub memory_mb: i32,
    /// `pending`, `ready`, `failed` or `removing`.
    pub status: String,
    /// The operation awaited.
    pub op_id: Option<Uuid>,
    /// The sealed password (Redis).
    pub sealed_password: Option<Vec<u8>>,
    /// The name the account's sites reach it by.
    pub host: Option<String>,
    /// Its port.
    pub port: Option<i32>,
    /// Why it failed.
    pub error: Option<String>,
}

/// What a sealed password is sealed to: that instance and nothing else.
#[must_use]
pub fn password_context(id: Uuid) -> Vec<u8> {
    format!("cache-password:{id}").into_bytes()
}

/// An account's cache servers.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Instance>, sqlx::Error> {
    sqlx::query_as!(
        Instance,
        "SELECT id, tenant_id, kind, memory_mb, status, op_id, sealed_password, host, port, error
         FROM cache_instances WHERE tenant_id = $1 ORDER BY kind",
        tenant_id
    )
    .fetch_all(pool)
    .await
}

/// One of an account's cache servers by kind.
///
/// # Errors
/// Database errors.
pub async fn get(
    pool: &PgPool,
    tenant_id: Uuid,
    kind: &str,
) -> Result<Option<Instance>, sqlx::Error> {
    sqlx::query_as!(
        Instance,
        "SELECT id, tenant_id, kind, memory_mb, status, op_id, sealed_password, host, port, error
         FROM cache_instances WHERE tenant_id = $1 AND kind = $2",
        tenant_id,
        kind
    )
    .fetch_optional(pool)
    .await
}

/// Memory the account's other cache servers use (MiB).
///
/// # Errors
/// Database errors.
pub async fn memory_used(
    pool: &PgPool,
    tenant_id: Uuid,
    except_kind: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(sum(memory_mb), 0)::bigint AS "n!" FROM cache_instances
           WHERE tenant_id = $1 AND kind <> $2 AND status <> 'removing'"#,
        tenant_id,
        except_kind
    )
    .fetch_one(pool)
    .await
}

/// Makes the row for a cache server, or changes the memory of the one there (pending again).
/// Returns its id.
///
/// # Errors
/// Database errors.
pub async fn upsert(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    kind: &str,
    memory_mb: i32,
    sealed_password: Option<&[u8]>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO cache_instances (tenant_id, kind, memory_mb, sealed_password)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, kind) DO UPDATE
           SET memory_mb = $3, status = 'pending', error = NULL, updated_at = now(),
               sealed_password = COALESCE(cache_instances.sealed_password, $4)
         RETURNING id",
        tenant_id,
        kind,
        memory_mb,
        sealed_password
    )
    .fetch_one(conn)
    .await
}

/// Notes the operation a row waits for.
///
/// # Errors
/// Database errors.
pub async fn waiting_for(
    pool: &PgPool,
    id: Uuid,
    op_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE cache_instances SET op_id = $2, status = $3, updated_at = now() WHERE id = $1",
        id,
        op_id,
        status
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Rows whose operation answer is awaited.
///
/// # Errors
/// Database errors.
pub async fn awaiting(pool: &PgPool) -> Result<Vec<Instance>, sqlx::Error> {
    sqlx::query_as!(
        Instance,
        "SELECT id, tenant_id, kind, memory_mb, status, op_id, sealed_password, host, port, error
         FROM cache_instances WHERE op_id IS NOT NULL AND status IN ('pending', 'removing')"
    )
    .fetch_all(pool)
    .await
}

/// The node said it is up.
///
/// # Errors
/// Database errors.
pub async fn ready(pool: &PgPool, id: Uuid, host: &str, port: i32) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE cache_instances SET status = 'ready', op_id = NULL, host = $2, port = $3, error = NULL, updated_at = now()
         WHERE id = $1",
        id,
        host,
        port
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
        "UPDATE cache_instances SET status = 'failed', op_id = NULL, error = $2, updated_at = now() WHERE id = $1",
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
    sqlx::query!("DELETE FROM cache_instances WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The sealed password of an instance by id.
///
/// # Errors
/// Database errors.
pub async fn sealed_password(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT sealed_password FROM cache_instances WHERE id = $1",
        id
    )
    .fetch_one(pool)
    .await
}
