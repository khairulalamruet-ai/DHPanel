//! PHP versions of accounts and domains (T06.08).

use sqlx::{PgConnection, PgPool, types::Uuid};

/// Where an account's PHP stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhpState {
    /// The account's version.
    pub version: String,
    /// Goes up whenever what the PHP containers should be changes.
    pub state_version: i64,
    /// The state the node has reported done.
    pub applied_version: i64,
}

/// An account's PHP state.
///
/// # Errors
/// Database errors.
pub async fn state(pool: &PgPool, tenant_id: Uuid) -> Result<Option<PhpState>, sqlx::Error> {
    sqlx::query_as!(
        PhpState,
        r#"SELECT php_version AS version, php_state_version AS state_version,
                  php_applied_version AS applied_version
           FROM tenants WHERE id = $1"#,
        tenant_id,
    )
    .fetch_optional(pool)
    .await
}

/// Sets an account's version. Returns whether it changed.
///
/// # Errors
/// Database errors.
pub async fn set_account_version(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    version: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE tenants SET php_version = $2 WHERE id = $1 AND php_version <> $2",
        tenant_id,
        version,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Sets the version a domain uses instead of the account's (`None`: the
/// account's). Returns whether the domain exists.
///
/// # Errors
/// Database errors.
pub async fn set_domain_version(
    conn: &mut PgConnection,
    domain_id: Uuid,
    version: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE domains SET php_version = $2 WHERE id = $1",
        domain_id,
        version,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// What the PHP containers of an account should be besides the main one: the
/// versions its domains ask for that are not the account's own, sorted.
///
/// # Errors
/// Database errors.
pub async fn extra_versions(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT DISTINCT d.php_version AS "version!"
           FROM domains d JOIN tenants t ON t.id = d.tenant_id
           WHERE d.tenant_id = $1 AND d.php_version IS NOT NULL
             AND d.php_version <> t.php_version
           ORDER BY 1"#,
        tenant_id,
    )
    .fetch_all(pool)
    .await
}

/// A domain's own version.
///
/// # Errors
/// Database errors.
pub async fn domain_version(pool: &PgPool, domain_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT php_version FROM domains WHERE id = $1", domain_id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}
