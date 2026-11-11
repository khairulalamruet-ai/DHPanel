//! Role and permission persistence (`roles`, `role_permissions`, `account_roles`).

use sqlx::{PgPool, types::Uuid};

use crate::accounts::{DbAccountKind, DbAccountStatus};

/// What an account may do, as stored: its status, kind, role names and the
/// raw permission grants (exact keys and wildcards) of all its roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountAccess {
    /// Account status; only active accounts should be let in.
    pub status: DbAccountStatus,
    /// Account kind.
    pub kind: DbAccountKind,
    /// Role names, sorted.
    pub roles: Vec<String>,
    /// Distinct permission grants, sorted.
    pub permissions: Vec<String>,
}

/// Loads an account's access. `None` when the account does not exist.
///
/// # Errors
/// Database errors.
pub async fn load_access(
    pool: &PgPool,
    account_id: Uuid,
) -> Result<Option<AccountAccess>, sqlx::Error> {
    sqlx::query_as!(
        AccountAccess,
        r#"SELECT a.status AS "status: DbAccountStatus",
                  a.kind AS "kind: DbAccountKind",
                  COALESCE((SELECT array_agg(r.name ORDER BY r.name)
                            FROM account_roles ar JOIN roles r ON r.id = ar.role_id
                            WHERE ar.account_id = a.id), '{}') AS "roles!",
                  COALESCE((SELECT array_agg(DISTINCT rp.permission ORDER BY rp.permission)
                            FROM account_roles ar JOIN role_permissions rp ON rp.role_id = ar.role_id
                            WHERE ar.account_id = a.id), '{}') AS "permissions!"
           FROM accounts a WHERE a.id = $1"#,
        account_id
    )
    .fetch_optional(pool)
    .await
}

/// Whether a role with this name exists.
///
/// # Errors
/// Database errors.
pub async fn role_exists(pool: &PgPool, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM roles WHERE name = $1) AS "exists!""#,
        name
    )
    .fetch_one(pool)
    .await
}

/// Outcome of [`assign_role`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignRole {
    /// The account now has the role.
    Assigned,
    /// The account already had it; nothing changed.
    AlreadyAssigned,
    /// No role with that name.
    UnknownRole,
}

/// Gives an account a role. `granted_by` is the acting administrator, or
/// `None` for the command line.
///
/// # Errors
/// Database errors (including a foreign-key error for an unknown account).
pub async fn assign_role(
    pool: &PgPool,
    account_id: Uuid,
    role: &str,
    granted_by: Option<Uuid>,
) -> Result<AssignRole, sqlx::Error> {
    let Some(role_id) = sqlx::query_scalar!("SELECT id FROM roles WHERE name = $1", role)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(AssignRole::UnknownRole);
    };
    let inserted = sqlx::query!(
        r#"INSERT INTO account_roles (account_id, role_id, granted_by)
           VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"#,
        account_id,
        role_id,
        granted_by
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(if inserted == 1 {
        AssignRole::Assigned
    } else {
        AssignRole::AlreadyAssigned
    })
}

/// Takes a role away from an account. Returns whether the account had it.
///
/// # Errors
/// Database errors.
pub async fn revoke_role(pool: &PgPool, account_id: Uuid, role: &str) -> Result<bool, sqlx::Error> {
    let removed = sqlx::query!(
        r#"DELETE FROM account_roles ar USING roles r
           WHERE r.id = ar.role_id AND ar.account_id = $1 AND r.name = $2"#,
        account_id,
        role
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(removed == 1)
}
