//! Databases, database users and grants of an account (T07.06).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// A database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Db {
    /// Id.
    pub id: Uuid,
    /// The account it belongs to.
    pub tenant_id: Uuid,
    /// The whole name.
    pub name: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub status: String,
    /// Size, as last measured (bytes).
    pub size_bytes: i64,
    /// When it was measured (Unix seconds).
    pub size_checked_at: Option<i64>,
}

/// A database user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbUser {
    /// Id.
    pub id: Uuid,
    /// The account it belongs to.
    pub tenant_id: Uuid,
    /// The whole name.
    pub name: String,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub status: String,
}

/// A user's access to a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// The database.
    pub database_id: Uuid,
    /// The user.
    pub user_id: Uuid,
    /// `read_only`, `read_write` or `all`.
    pub access: String,
}

/// Why a name was refused.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// Someone (any account) has the name.
    #[error("the name is taken")]
    Taken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn taken(err: sqlx::Error) -> AddError {
    match err.as_database_error().and_then(|e| e.constraint()) {
        Some("databases_name_key" | "db_users_name_key") => AddError::Taken,
        _ => AddError::Database(err),
    }
}

/// An account's databases, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Db>, sqlx::Error> {
    sqlx::query_as!(
        Db,
        r#"SELECT id, tenant_id, name, status, size_bytes,
                  extract(epoch FROM size_checked_at)::bigint AS size_checked_at
           FROM databases WHERE tenant_id = $1 ORDER BY name"#,
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// One of an account's databases.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<Db>, sqlx::Error> {
    sqlx::query_as!(
        Db,
        r#"SELECT id, tenant_id, name, status, size_bytes,
                  extract(epoch FROM size_checked_at)::bigint AS size_checked_at
           FROM databases WHERE tenant_id = $1 AND id = $2"#,
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// How many databases an account has (the ones being removed do not count).
///
/// # Errors
/// Database errors.
pub async fn count(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM databases WHERE tenant_id = $1 AND status <> 'deleting'"#,
        tenant_id,
    )
    .fetch_one(conn)
    .await
}

/// Adds a database (`pending`).
///
/// # Errors
/// [`AddError`].
pub async fn create(conn: &mut PgConnection, tenant_id: Uuid, name: &str) -> Result<Db, AddError> {
    sqlx::query_as!(
        Db,
        r#"INSERT INTO databases (tenant_id, name) VALUES ($1, $2)
           RETURNING id, tenant_id, name, status, size_bytes,
                     extract(epoch FROM size_checked_at)::bigint AS size_checked_at"#,
        tenant_id,
        name,
    )
    .fetch_one(conn)
    .await
    .map_err(taken)
}

/// Marks a database as being removed. Returns whether it was there and not already so.
///
/// # Errors
/// Database errors.
pub async fn mark_deleting(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<Db>, sqlx::Error> {
    sqlx::query_as!(
        Db,
        r#"UPDATE databases SET status = 'deleting'
           WHERE tenant_id = $1 AND id = $2 AND status <> 'deleting'
           RETURNING id, tenant_id, name, status, size_bytes,
                     extract(epoch FROM size_checked_at)::bigint AS size_checked_at"#,
        tenant_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// An account's database users, by name.
///
/// # Errors
/// Database errors.
pub async fn users(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<DbUser>, sqlx::Error> {
    sqlx::query_as!(
        DbUser,
        "SELECT id, tenant_id, name, status FROM db_users WHERE tenant_id = $1 ORDER BY name",
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// One of an account's database users.
///
/// # Errors
/// Database errors.
pub async fn user(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<Option<DbUser>, sqlx::Error> {
    sqlx::query_as!(
        DbUser,
        "SELECT id, tenant_id, name, status FROM db_users WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// How many database users an account has (the ones being removed do not count).
///
/// # Errors
/// Database errors.
pub async fn count_users(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM db_users WHERE tenant_id = $1 AND status <> 'deleting'"#,
        tenant_id,
    )
    .fetch_one(conn)
    .await
}

/// Adds a database user (`pending`).
///
/// # Errors
/// [`AddError`].
pub async fn create_user(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
) -> Result<DbUser, AddError> {
    sqlx::query_as!(
        DbUser,
        "INSERT INTO db_users (tenant_id, name) VALUES ($1, $2)
         RETURNING id, tenant_id, name, status",
        tenant_id,
        name,
    )
    .fetch_one(conn)
    .await
    .map_err(taken)
}

/// Marks a database user as being removed.
///
/// # Errors
/// Database errors.
pub async fn mark_user_deleting(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<DbUser>, sqlx::Error> {
    sqlx::query_as!(
        DbUser,
        "UPDATE db_users SET status = 'deleting'
         WHERE tenant_id = $1 AND id = $2 AND status <> 'deleting'
         RETURNING id, tenant_id, name, status",
        tenant_id,
        id,
    )
    .fetch_optional(conn)
    .await
}

/// Keeps a user's sealed password until the node has it.
///
/// # Errors
/// Database errors.
pub async fn keep_password(
    conn: &mut PgConnection,
    user_id: Uuid,
    sealed: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO db_user_passwords (user_id, sealed) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET sealed = EXCLUDED.sealed, created_at = now()",
        user_id,
        sealed,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The sealed password waiting for the user called `name`, with the user's id
/// (what it was sealed for).
///
/// # Errors
/// Database errors.
pub async fn sealed_password_of(
    pool: &PgPool,
    name: &str,
) -> Result<Option<(Uuid, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT p.user_id, p.sealed FROM db_user_passwords p
         JOIN db_users u ON u.id = p.user_id WHERE u.name = $1",
        name,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.user_id, r.sealed)))
}

/// What a sealed password is sealed to: that user and nothing else.
#[must_use]
pub fn password_context(user_id: Uuid) -> Vec<u8> {
    format!("db-user-password:{user_id}").into_bytes()
}

/// All the grants of an account.
///
/// # Errors
/// Database errors.
pub async fn grants(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<Grant>, sqlx::Error> {
    sqlx::query_as!(
        Grant,
        "SELECT database_id, user_id, access FROM db_grants WHERE tenant_id = $1",
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// Gives a user an access level in a database (replacing the one it had).
///
/// # Errors
/// Database errors (a database or user of another account is refused by the table).
pub async fn set_grant(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    database_id: Uuid,
    user_id: Uuid,
    access: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO db_grants (tenant_id, database_id, user_id, access) VALUES ($1, $2, $3, $4)
         ON CONFLICT (database_id, user_id) DO UPDATE SET access = EXCLUDED.access",
        tenant_id,
        database_id,
        user_id,
        access,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Takes a grant away. Returns whether there was one.
///
/// # Errors
/// Database errors.
pub async fn remove_grant(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    database_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "DELETE FROM db_grants WHERE tenant_id = $1 AND database_id = $2 AND user_id = $3",
        tenant_id,
        database_id,
        user_id,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// The remote addresses of a user.
///
/// # Errors
/// Database errors.
pub async fn remote_hosts(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT cidr FROM db_remote_hosts WHERE user_id = $1 ORDER BY cidr",
        user_id,
    )
    .fetch_all(pool)
    .await
}

/// Replaces a user's remote addresses.
///
/// # Errors
/// Database errors.
pub async fn set_remote_hosts(
    conn: &mut PgConnection,
    user_id: Uuid,
    cidrs: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM db_remote_hosts WHERE user_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    for cidr in cidrs {
        sqlx::query!(
            "INSERT INTO db_remote_hosts (user_id, cidr) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            user_id,
            cidr,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Keeps the sizes a node measured (by database name, only for the databases of
/// accounts on that node). Returns how many were updated.
///
/// # Errors
/// Database errors.
pub async fn record_sizes(
    conn: &mut PgConnection,
    node_id: Uuid,
    sizes: &[(String, i64)],
) -> Result<u64, sqlx::Error> {
    let mut updated = 0;
    for (name, bytes) in sizes {
        let done = sqlx::query!(
            "UPDATE databases d SET size_bytes = $3, size_checked_at = now()
             FROM tenants t
             WHERE d.tenant_id = t.id AND t.node_id = $1 AND d.name = $2",
            node_id,
            name,
            (*bytes).max(0),
        )
        .execute(&mut *conn)
        .await?;
        updated += done.rows_affected();
    }
    Ok(updated)
}

/// Whether any account on the node has a database.
///
/// # Errors
/// Database errors.
pub async fn node_has_databases(pool: &PgPool, node_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM databases d JOIN tenants t ON t.id = d.tenant_id
                          WHERE t.node_id = $1) AS "has!""#,
        node_id,
    )
    .fetch_one(pool)
    .await
}
