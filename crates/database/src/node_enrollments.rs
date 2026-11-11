//! Node enrollment tokens (`node_enrollment_tokens`).
//!
//! The table holds only the SHA-256 of each token. All timing decisions use the
//! database clock, so every API instance agrees on whether a token has expired.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// Where a token stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStatus {
    /// Can still be used.
    Pending,
    /// Not used and no longer valid.
    Expired,
    /// Used to enrol a node.
    Consumed,
    /// Cancelled by an administrator.
    Revoked,
}

impl TokenStatus {
    /// Wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
            Self::Revoked => "revoked",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "consumed" => Self::Consumed,
            "revoked" => Self::Revoked,
            "expired" => Self::Expired,
            _ => Self::Pending,
        }
    }
}

/// A token as listed (never includes the token itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentToken {
    /// Token id.
    pub id: Uuid,
    /// The node name it was issued for.
    pub name: String,
    /// Who created it, if that account still exists.
    pub created_by: Option<Uuid>,
    /// Creation time (Unix seconds).
    pub created_at: i64,
    /// Expiry (Unix seconds).
    pub expires_at: i64,
    /// Current state.
    pub status: TokenStatus,
}

struct Row {
    id: Uuid,
    name: String,
    created_by: Option<Uuid>,
    created_at: i64,
    expires_at: i64,
    status: String,
}

impl From<Row> for EnrollmentToken {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            created_by: r.created_by,
            created_at: r.created_at,
            expires_at: r.expires_at,
            status: TokenStatus::parse(&r.status),
        }
    }
}

/// Whether a node, or a still-valid token, already uses this name.
///
/// # Errors
/// Database errors.
pub async fn name_in_use(conn: &mut PgConnection, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (EXISTS (SELECT 1 FROM nodes WHERE name = $1)
                OR EXISTS (SELECT 1 FROM node_enrollment_tokens
                           WHERE name = $1 AND consumed_at IS NULL AND revoked_at IS NULL
                             AND expires_at > now())) AS "in_use!""#,
        name
    )
    .fetch_one(conn)
    .await
}

/// Stores a new token (by hash) that expires `ttl_secs` seconds from now.
///
/// # Errors
/// Database errors (including a hash collision, which cannot happen with random tokens).
pub async fn insert_token(
    conn: &mut PgConnection,
    name: &str,
    token_sha256: &[u8; 32],
    created_by: Uuid,
    ttl_secs: i64,
) -> Result<EnrollmentToken, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"INSERT INTO node_enrollment_tokens (name, token_sha256, created_by, expires_at)
           VALUES ($1, $2, $3, now() + make_interval(secs => $4::double precision))
           RETURNING id, name, created_by,
                     extract(epoch FROM created_at)::bigint AS "created_at!",
                     extract(epoch FROM expires_at)::bigint AS "expires_at!",
                     'pending' AS "status!""#,
        name,
        &token_sha256[..],
        created_by,
        ttl_secs as f64,
    )
    .fetch_one(conn)
    .await?;
    Ok(row.into())
}

/// The most recent tokens, newest first.
///
/// # Errors
/// Database errors.
pub async fn list_tokens(pool: &PgPool, limit: i64) -> Result<Vec<EnrollmentToken>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, name, created_by,
                  extract(epoch FROM created_at)::bigint AS "created_at!",
                  extract(epoch FROM expires_at)::bigint AS "expires_at!",
                  CASE WHEN consumed_at IS NOT NULL THEN 'consumed'
                       WHEN revoked_at IS NOT NULL THEN 'revoked'
                       WHEN expires_at <= now() THEN 'expired'
                       ELSE 'pending' END AS "status!"
           FROM node_enrollment_tokens
           ORDER BY created_at DESC, id DESC
           LIMIT $1"#,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Outcome of [`revoke_token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokeOutcome {
    /// The token was pending and is now revoked.
    Revoked,
    /// The token exists but is already used, revoked or expired.
    NotPending,
    /// No such token.
    Unknown,
}

/// Cancels a token that is still pending. An expired or used token is left alone.
///
/// # Errors
/// Database errors.
pub async fn revoke_token(conn: &mut PgConnection, id: Uuid) -> Result<RevokeOutcome, sqlx::Error> {
    let revoked = sqlx::query!(
        r#"UPDATE node_enrollment_tokens SET revoked_at = now()
           WHERE id = $1 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > now()"#,
        id
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if revoked == 1 {
        return Ok(RevokeOutcome::Revoked);
    }
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM node_enrollment_tokens WHERE id = $1) AS "exists!""#,
        id
    )
    .fetch_one(conn)
    .await?;
    Ok(if exists {
        RevokeOutcome::NotPending
    } else {
        RevokeOutcome::Unknown
    })
}

/// A token that was just used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedToken {
    /// Token id.
    pub id: Uuid,
    /// The node name it was issued for.
    pub name: String,
}

/// Uses a token: succeeds **at most once**, and only while the token is
/// pending (not used, not revoked, not expired). A single `UPDATE` makes the
/// check and the use atomic, so two agents racing with the same token cannot
/// both win. Unknown, used, revoked and expired tokens all give `None`; the
/// caller must not say which.
///
/// # Errors
/// Database errors.
pub async fn consume_token(
    conn: &mut PgConnection,
    token_sha256: &[u8; 32],
) -> Result<Option<ConsumedToken>, sqlx::Error> {
    sqlx::query_as!(
        ConsumedToken,
        r#"UPDATE node_enrollment_tokens SET consumed_at = now()
           WHERE token_sha256 = $1 AND consumed_at IS NULL AND revoked_at IS NULL
             AND expires_at > now()
           RETURNING id, name"#,
        &token_sha256[..],
    )
    .fetch_optional(conn)
    .await
}

/// Records which node a consumed token created.
///
/// # Errors
/// Database errors.
pub async fn attach_node(
    conn: &mut PgConnection,
    token_id: Uuid,
    node_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE node_enrollment_tokens SET node_id = $2 WHERE id = $1 AND consumed_at IS NOT NULL",
        token_id,
        node_id
    )
    .execute(conn)
    .await?;
    Ok(())
}
