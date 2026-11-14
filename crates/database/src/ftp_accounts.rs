//! SFTP and FTPS accounts of a hosting account (T08.12).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// An account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpAccount {
    /// Id.
    pub id: Uuid,
    /// The hosting account it belongs to.
    pub tenant_id: Uuid,
    /// The whole name.
    pub name: String,
    /// Its home, relative to the tenant's files.
    pub home: String,
    /// May log in over SFTP.
    pub sftp: bool,
    /// May log in over FTP with TLS.
    pub ftps: bool,
    /// May log in over FTP without TLS.
    pub ftp_plain: bool,
    /// May only read.
    pub read_only: bool,
    /// Disk limit in MB.
    pub quota_mb: Option<i64>,
    /// May log in at all.
    pub enabled: bool,
    /// `pending`, `ready`, `failed` or `deleting`.
    pub status: String,
}

/// Why an account could not be made.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    /// The name is taken (by any account on the panel).
    #[error("the name is taken")]
    Taken,
    /// Any other database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct Row {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    home: String,
    sftp: bool,
    ftps: bool,
    ftp_plain: bool,
    read_only: bool,
    quota_mb: Option<i64>,
    enabled: bool,
    status: String,
}

impl From<Row> for FtpAccount {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            home: r.home,
            sftp: r.sftp,
            ftps: r.ftps,
            ftp_plain: r.ftp_plain,
            read_only: r.read_only,
            quota_mb: r.quota_mb,
            enabled: r.enabled,
            status: r.status,
        }
    }
}

/// An account's accounts, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<FtpAccount>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        "SELECT id, tenant_id, name, home, sftp, ftps, ftp_plain, read_only, quota_mb, enabled, status
         FROM ftp_accounts WHERE tenant_id = $1 ORDER BY name",
        tenant_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(FtpAccount::from).collect())
}

/// One account of an account.
///
/// # Errors
/// Database errors.
pub async fn get(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<FtpAccount>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        "SELECT id, tenant_id, name, home, sftp, ftps, ftp_plain, read_only, quota_mb, enabled, status
         FROM ftp_accounts WHERE tenant_id = $1 AND id = $2",
        tenant_id,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(FtpAccount::from))
}

/// How many accounts an account has (those being removed excluded).
///
/// # Errors
/// Database errors.
pub async fn count(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM ftp_accounts WHERE tenant_id = $1 AND status <> 'deleting'"#,
        tenant_id,
    )
    .fetch_one(conn)
    .await
}

/// Makes an account (`pending`).
///
/// # Errors
/// [`AddError::Taken`] when the name is used.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    name: &str,
    home: &str,
    (sftp, ftps, ftp_plain): (bool, bool, bool),
    read_only: bool,
    quota_mb: Option<i64>,
) -> Result<FtpAccount, AddError> {
    let row = sqlx::query_as!(
        Row,
        "INSERT INTO ftp_accounts (tenant_id, name, home, sftp, ftps, ftp_plain, read_only, quota_mb)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, tenant_id, name, home, sftp, ftps, ftp_plain, read_only, quota_mb, enabled, status",
        tenant_id,
        name,
        home,
        sftp,
        ftps,
        ftp_plain,
        read_only,
        quota_mb,
    )
    .fetch_one(conn)
    .await
    .map_err(|err| {
        if err.as_database_error().and_then(|e| e.constraint()) == Some("ftp_accounts_name_key") {
            AddError::Taken
        } else {
            AddError::Database(err)
        }
    })?;
    Ok(row.into())
}

/// Changes an account (back to `pending` until the node has done it).
///
/// # Errors
/// Database errors.
#[allow(clippy::too_many_arguments)]
pub async fn change(
    conn: &mut PgConnection,
    id: Uuid,
    home: &str,
    (sftp, ftps, ftp_plain): (bool, bool, bool),
    read_only: bool,
    quota_mb: Option<i64>,
    enabled: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE ftp_accounts SET home = $2, sftp = $3, ftps = $4, ftp_plain = $5, read_only = $6,
                                 quota_mb = $7, enabled = $8, status = 'pending', updated_at = now()
         WHERE id = $1 AND status <> 'deleting'",
        id,
        home,
        sftp,
        ftps,
        ftp_plain,
        read_only,
        quota_mb,
        enabled,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks an account `deleting`; returns it.
///
/// # Errors
/// Database errors.
pub async fn mark_deleting(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<FtpAccount>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        "UPDATE ftp_accounts SET status = 'deleting', updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND status <> 'deleting'
         RETURNING id, tenant_id, name, home, sftp, ftps, ftp_plain, read_only, quota_mb, enabled, status",
        tenant_id,
        id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(FtpAccount::from))
}

/// Keeps a sealed password for the node.
///
/// # Errors
/// Database errors.
pub async fn keep_password(
    conn: &mut PgConnection,
    account_id: Uuid,
    sealed: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO ftp_account_passwords (account_id, sealed) VALUES ($1, $2)
         ON CONFLICT (account_id) DO UPDATE SET sealed = EXCLUDED.sealed, created_at = now()",
        account_id,
        sealed,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The sealed password waiting for the account called `name`, with the account's id.
///
/// # Errors
/// Database errors.
pub async fn sealed_password_of(
    pool: &PgPool,
    name: &str,
) -> Result<Option<(Uuid, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT p.account_id, p.sealed FROM ftp_account_passwords p
         JOIN ftp_accounts a ON a.id = p.account_id WHERE a.name = $1",
        name,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.account_id, r.sealed)))
}

/// What a sealed password is sealed to.
#[must_use]
pub fn password_context(account_id: Uuid) -> Vec<u8> {
    format!("ftp-account-password:{account_id}").into_bytes()
}
