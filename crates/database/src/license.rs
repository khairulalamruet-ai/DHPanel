//! The `license_state` table (T17.09): this installation's license, and the verdict on it.

use sqlx::{PgPool, types::Uuid};

/// What the table holds.
#[derive(Debug, Clone)]
pub struct State {
    /// Licensing is on (an authority is configured).
    pub enforced: bool,
    /// What the license client keeps (opaque here).
    pub stored: serde_json::Value,
    /// `unlicensed`, `active`, `grace` or `restricted`.
    pub mode: String,
    /// Why, when it is not `active`.
    pub reason: Option<String>,
    /// What the entitlement switches on.
    pub features: Vec<String>,
    /// Its numeric limits (`-1`: none).
    pub limits: serde_json::Value,
    /// The license.
    pub license_id: Option<Uuid>,
    /// Its plan's name.
    pub plan: Option<String>,
    /// `active`, `suspended` or `revoked`, as the authority last said.
    pub license_status: Option<String>,
    /// When the license ends (Unix seconds).
    pub license_expires_at: Option<i64>,
    /// When the entitlement ends (Unix seconds).
    pub entitlement_expires_at: Option<i64>,
    /// When the offline grace ends (Unix seconds).
    pub grace_ends_at: Option<i64>,
    /// What went wrong last time the authority was asked.
    pub last_error: Option<String>,
    /// When the verdict was made (Unix seconds).
    pub evaluated_at: i64,
}

impl State {
    /// A numeric limit: `None` when the entitlement does not say, `Some(-1)` for none.
    #[must_use]
    pub fn limit(&self, name: &str) -> Option<i64> {
        self.limits.get(name).and_then(serde_json::Value::as_i64)
    }

    /// Whether management is restricted.
    #[must_use]
    pub fn restricted(&self) -> bool {
        self.enforced && self.mode == "restricted"
    }
}

/// The row.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool) -> Result<State, sqlx::Error> {
    sqlx::query_as!(
        State,
        r#"SELECT enforced, stored, mode, reason, features, limits, license_id, plan, license_status,
                  extract(epoch FROM license_expires_at)::bigint AS license_expires_at,
                  extract(epoch FROM entitlement_expires_at)::bigint AS entitlement_expires_at,
                  extract(epoch FROM grace_ends_at)::bigint AS grace_ends_at,
                  last_error, extract(epoch FROM evaluated_at)::bigint AS "evaluated_at!"
           FROM license_state WHERE id"#
    )
    .fetch_one(pool)
    .await
}

/// What an evaluation decided, to be written down.
#[derive(Debug, Clone)]
pub struct Verdict<'a> {
    /// Licensing is on.
    pub enforced: bool,
    /// `unlicensed`, `active`, `grace` or `restricted`.
    pub mode: &'a str,
    /// Why.
    pub reason: Option<&'a str>,
    /// Features.
    pub features: &'a [String],
    /// Limits.
    pub limits: &'a serde_json::Value,
    /// When the entitlement ends.
    pub entitlement_expires_at: Option<i64>,
    /// When the grace ends.
    pub grace_ends_at: Option<i64>,
}

/// Writes the verdict of an evaluation.
///
/// # Errors
/// Database errors.
pub async fn save_verdict(pool: &PgPool, v: &Verdict<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE license_state SET enforced = $1, mode = $2, reason = $3, features = $4, limits = $5,
                entitlement_expires_at = to_timestamp($6::bigint), grace_ends_at = to_timestamp($7::bigint),
                evaluated_at = now() WHERE id",
        v.enforced,
        v.mode,
        v.reason,
        v.features,
        v.limits,
        v.entitlement_expires_at,
        v.grace_ends_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// What the license client keeps, with the license's description for display.
#[derive(Debug, Clone)]
pub struct Kept<'a> {
    /// The client's stored form.
    pub stored: &'a serde_json::Value,
    /// The license.
    pub license_id: Option<Uuid>,
    /// Its plan.
    pub plan: Option<&'a str>,
    /// Its status.
    pub license_status: Option<&'a str>,
    /// When it ends (Unix seconds).
    pub license_expires_at: Option<i64>,
}

/// Keeps what the license client stored (and clears the last error).
///
/// # Errors
/// Database errors.
pub async fn save_stored(pool: &PgPool, k: &Kept<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE license_state SET stored = $1, license_id = $2, plan = $3, license_status = $4,
                license_expires_at = to_timestamp($5::bigint), last_error = NULL, updated_at = now()
         WHERE id",
        k.stored,
        k.license_id,
        k.plan,
        k.license_status,
        k.license_expires_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Remembers why the authority could not be used.
///
/// # Errors
/// Database errors.
pub async fn note_error(pool: &PgPool, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error
        .chars()
        .filter(|c| !c.is_control())
        .take(500)
        .collect();
    sqlx::query!(
        "UPDATE license_state SET last_error = $1, updated_at = now() WHERE id",
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// What a limit refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitReached {
    /// The most the license allows.
    pub limit: i64,
}

/// Whether the license lets the count reach `wanted` (T17.11). Nothing is refused when
/// licensing is off, when the entitlement says nothing about `name`, or says `-1` (no limit).
/// Call it in the transaction that counts and inserts, like the plan limits.
///
/// # Errors
/// Database errors.
pub async fn check_limit(
    conn: &mut sqlx::PgConnection,
    name: &str,
    wanted: i64,
) -> Result<Option<LimitReached>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT enforced, mode, limits -> $1 AS limit_value FROM license_state WHERE id",
        name
    )
    .fetch_one(conn)
    .await?;
    if !row.enforced || !matches!(row.mode.as_str(), "active" | "grace") {
        return Ok(None);
    }
    let limit = row.limit_value.and_then(|v| v.as_i64());
    Ok(limit
        .filter(|l| *l >= 0 && wanted > *l)
        .map(|limit| LimitReached { limit }))
}
