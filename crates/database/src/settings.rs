//! The `settings` table: stored setting values. What a value may be is decided
//! by the module's schema in the settings service; this layer only stores.

use serde_json::Value;
use sqlx::{PgConnection, types::Uuid};

/// Advisory lock that serialises writers of one settings change.
const WRITE_LOCK: i64 = 7_303_003;

/// Takes the lock that serialises settings changes for the rest of the transaction.
///
/// # Errors
/// Database errors.
pub async fn lock(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", WRITE_LOCK)
        .execute(conn)
        .await?;
    Ok(())
}

/// The stored values of a module in one scope, by key.
///
/// # Errors
/// Database errors.
pub async fn load(
    conn: &mut PgConnection,
    scope: &str,
    scope_id: Option<Uuid>,
    module: &str,
) -> Result<Vec<(String, Value)>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT key, value FROM settings
           WHERE scope = $1 AND scope_id IS NOT DISTINCT FROM $2 AND module = $3
           ORDER BY key"#,
        scope,
        scope_id,
        module,
    )
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
}

/// Stores a value, replacing an earlier one.
///
/// # Errors
/// Database errors, including the size and name checks of the table.
pub async fn upsert(
    conn: &mut PgConnection,
    scope: &str,
    scope_id: Option<Uuid>,
    module: &str,
    key: &str,
    value: &Value,
    updated_by: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO settings (scope, scope_id, module, key, value, updated_by)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (scope, scope_id, module, key)
           DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, updated_at = now()"#,
        scope,
        scope_id,
        module,
        key,
        value,
        updated_by,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Removes a stored value, so the setting has its default again.
///
/// # Errors
/// Database errors.
pub async fn remove(
    conn: &mut PgConnection,
    scope: &str,
    scope_id: Option<Uuid>,
    module: &str,
    key: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        r#"DELETE FROM settings
           WHERE scope = $1 AND scope_id IS NOT DISTINCT FROM $2 AND module = $3 AND key = $4"#,
        scope,
        scope_id,
        module,
        key,
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}
