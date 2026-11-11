//! Administrator management queries (accounts of kind `admin`).
//!
//! Functions take a plain `&mut PgConnection` (a pooled connection, or
//! `&mut *transaction`), so a handler can run a change and its audit entry in
//! one transaction. Take [`lock_admin_changes`] first in such a transaction:
//! it serialises changes, which makes "never remove the last super
//! administrator" checks race-free.
//!
//! (Not generic over `sqlx::Acquire` on purpose: that makes async handlers
//! fail to compile with "implementation of `Acquire` is not general enough".)

use sqlx::{PgConnection, PgPool, types::Uuid};

use crate::accounts::DbAccountStatus;

/// Name of the role that cannot be left empty.
pub const SUPER_ADMIN_ROLE: &str = "super_admin";

/// One administrator, as listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRow {
    /// Account id.
    pub id: Uuid,
    /// Username.
    pub username: String,
    /// Email.
    pub email: String,
    /// Status.
    pub status: DbAccountStatus,
    /// Two-factor authentication is mandatory for this account.
    pub mfa_required: bool,
    /// A second factor is enrolled and confirmed.
    pub mfa_enabled: bool,
    /// Role names, sorted.
    pub roles: Vec<String>,
    /// Unix seconds of account creation.
    pub created_at: i64,
    /// Unix seconds of the last successful login.
    pub last_login_at: Option<i64>,
}

impl AdminRow {
    /// Whether the account holds the role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Whether this account currently counts as a super administrator.
    #[must_use]
    pub fn is_active_super_admin(&self) -> bool {
        self.status == DbAccountStatus::Active && self.has_role(SUPER_ADMIN_ROLE)
    }
}

/// Administrators ordered by id (creation order), `limit` of them after `after`.
///
/// # Errors
/// Database errors.
pub async fn list_admins(
    pool: &PgPool,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<AdminRow>, sqlx::Error> {
    sqlx::query_as!(
        AdminRow,
        r#"SELECT a.id, a.username, a.email,
                  a.status AS "status: DbAccountStatus",
                  a.mfa_required,
                  EXISTS (SELECT 1 FROM account_mfa m
                          WHERE m.account_id = a.id AND m.kind = 'totp'
                            AND m.confirmed_at IS NOT NULL) AS "mfa_enabled!",
                  COALESCE((SELECT array_agg(r.name ORDER BY r.name)
                            FROM account_roles ar JOIN roles r ON r.id = ar.role_id
                            WHERE ar.account_id = a.id), '{}') AS "roles!",
                  extract(epoch FROM a.created_at)::bigint AS "created_at!",
                  (SELECT extract(epoch FROM max(le.at))::bigint FROM login_events le
                   WHERE le.account_id = a.id AND le.success) AS "last_login_at?"
           FROM accounts a
           WHERE a.kind = 'admin' AND ($1::uuid IS NULL OR a.id > $1)
           ORDER BY a.id
           LIMIT $2"#,
        after,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// One administrator by id; `None` for unknown ids and non-administrators.
///
/// # Errors
/// Database errors.
pub async fn find_admin(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<AdminRow>, sqlx::Error> {
    sqlx::query_as!(
        AdminRow,
        r#"SELECT a.id, a.username, a.email,
                  a.status AS "status: DbAccountStatus",
                  a.mfa_required,
                  EXISTS (SELECT 1 FROM account_mfa m
                          WHERE m.account_id = a.id AND m.kind = 'totp'
                            AND m.confirmed_at IS NOT NULL) AS "mfa_enabled!",
                  COALESCE((SELECT array_agg(r.name ORDER BY r.name)
                            FROM account_roles ar JOIN roles r ON r.id = ar.role_id
                            WHERE ar.account_id = a.id), '{}') AS "roles!",
                  extract(epoch FROM a.created_at)::bigint AS "created_at!",
                  (SELECT extract(epoch FROM max(le.at))::bigint FROM login_events le
                   WHERE le.account_id = a.id AND le.success) AS "last_login_at?"
           FROM accounts a
           WHERE a.kind = 'admin' AND a.id = $1"#,
        id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Takes the transaction-scoped lock that serialises administrator changes.
/// Released at commit or rollback.
///
/// # Errors
/// Database errors.
pub async fn lock_admin_changes(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("SELECT pg_advisory_xact_lock(7301001) AS \"locked!: ()\"")
        .fetch_one(&mut *conn)
        .await?;
    Ok(())
}

/// Number of active administrators holding [`SUPER_ADMIN_ROLE`].
///
/// # Errors
/// Database errors.
pub async fn count_active_super_admins(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!"
           FROM accounts a
           JOIN account_roles ar ON ar.account_id = a.id
           JOIN roles r ON r.id = ar.role_id
           WHERE a.kind = 'admin' AND a.status = 'active' AND r.name = $1"#,
        SUPER_ADMIN_ROLE,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Why [`update_admin`] failed.
#[derive(Debug, thiserror::Error)]
pub enum UpdateAdminError {
    /// Another account already uses this email.
    #[error("email is already in use")]
    EmailTaken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Changes the given fields of an administrator (`None` leaves a field as is).
/// Returns whether the administrator exists.
///
/// # Errors
/// [`UpdateAdminError::EmailTaken`] or database errors.
pub async fn update_admin(
    conn: &mut PgConnection,
    id: Uuid,
    email: Option<&str>,
    status: Option<DbAccountStatus>,
    mfa_required: Option<bool>,
) -> Result<bool, UpdateAdminError> {
    let updated = sqlx::query!(
        r#"UPDATE accounts
           SET email = COALESCE($2, email),
               status = COALESCE($3, status),
               mfa_required = COALESCE($4, mfa_required)
           WHERE id = $1 AND kind = 'admin'"#,
        id,
        email,
        status as Option<DbAccountStatus>,
        mfa_required,
    )
    .execute(&mut *conn)
    .await
    .map_err(
        |err| match err.as_database_error().and_then(|e| e.constraint()) {
            Some("accounts_email_key") => UpdateAdminError::EmailTaken,
            _ => UpdateAdminError::Database(err),
        },
    )?;
    Ok(updated.rows_affected() == 1)
}

/// Deletes an administrator. Sessions, roles and second factors go with the
/// account (their rows cascade); audit entries stay. End the account's
/// sessions in the session store first — their Redis entries are not cascaded.
/// Returns whether it existed.
///
/// # Errors
/// Database errors.
pub async fn delete_admin(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let deleted = sqlx::query!("DELETE FROM accounts WHERE id = $1 AND kind = 'admin'", id)
        .execute(&mut *conn)
        .await?;
    Ok(deleted.rows_affected() == 1)
}

/// Removes the account's second factor and backup codes (account recovery).
/// Returns whether a factor existed.
///
/// # Errors
/// Database errors.
pub async fn reset_mfa(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let removed = sqlx::query!("DELETE FROM account_mfa WHERE account_id = $1", id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    sqlx::query!("DELETE FROM mfa_backup_codes WHERE account_id = $1", id)
        .execute(&mut *conn)
        .await?;
    Ok(removed > 0)
}
