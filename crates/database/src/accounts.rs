//! Account persistence.

use dhpanel_domain::{AccountKind, Email, Username};
use sqlx::{PgPool, types::Uuid};

/// Mirrors the `account_kind` PostgreSQL enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "account_kind", rename_all = "lowercase")]
pub enum DbAccountKind {
    /// Panel operator.
    Admin,
    /// Reseller.
    Reseller,
    /// Hosting customer.
    Client,
}

impl From<AccountKind> for DbAccountKind {
    fn from(k: AccountKind) -> Self {
        match k {
            AccountKind::Admin => Self::Admin,
            AccountKind::Reseller => Self::Reseller,
            AccountKind::Client => Self::Client,
        }
    }
}

/// Input for [`insert_account`]. The password must already be hashed.
#[derive(Debug, Clone)]
pub struct NewAccount<'a> {
    /// Account kind.
    pub kind: AccountKind,
    /// Validated username.
    pub username: &'a Username,
    /// Validated email.
    pub email: &'a Email,
    /// Argon2id PHC string.
    pub password_hash: &'a str,
}

/// Why an account could not be created.
#[derive(Debug, thiserror::Error)]
pub enum InsertAccountError {
    /// Another account already uses this username.
    #[error("username is already taken")]
    UsernameTaken,
    /// Another account already uses this email.
    #[error("email is already in use")]
    EmailTaken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Inserts an account and returns its id.
///
/// # Errors
/// [`InsertAccountError::UsernameTaken`] / [`InsertAccountError::EmailTaken`] on
/// unique violations, otherwise the database error.
pub async fn insert_account(
    pool: &PgPool,
    new: &NewAccount<'_>,
) -> Result<Uuid, InsertAccountError> {
    let kind = DbAccountKind::from(new.kind);
    sqlx::query_scalar!(
        r#"INSERT INTO accounts (kind, username, email, password_hash)
           VALUES ($1, $2, $3, $4)
           RETURNING id"#,
        kind as DbAccountKind,
        new.username.as_str(),
        new.email.as_str(),
        new.password_hash,
    )
    .fetch_one(pool)
    .await
    .map_err(
        |err| match err.as_database_error().and_then(|e| e.constraint()) {
            Some("accounts_username_key") => InsertAccountError::UsernameTaken,
            Some("accounts_email_key") => InsertAccountError::EmailTaken,
            _ => InsertAccountError::Database(err),
        },
    )
}

/// Number of accounts of the given kind (used to detect first-run bootstrap).
///
/// # Errors
/// Database errors.
pub async fn count_accounts(pool: &PgPool, kind: AccountKind) -> Result<i64, sqlx::Error> {
    let kind = DbAccountKind::from(kind);
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM accounts WHERE kind = $1"#,
        kind as DbAccountKind
    )
    .fetch_one(pool)
    .await
}
