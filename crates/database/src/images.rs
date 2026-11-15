//! The `image_scans` table (T11.12).

use sqlx::PgPool;

/// One image of the allow-list with what is known about it.
#[derive(Debug, Clone)]
pub struct Image {
    /// The pinned reference (`name@sha256:…`).
    pub reference: String,
    /// The role it plays.
    pub role: String,
    /// The moving tag it was pinned from.
    pub tag: Option<String>,
    /// What it is.
    pub note: String,
    /// What the registry says the tag is now.
    pub latest_digest: Option<String>,
    /// When it was asked (Unix seconds).
    pub checked_at: Option<i64>,
    /// When it was scanned (Unix seconds).
    pub scanned_at: Option<i64>,
    /// Findings by severity (`None`: never scanned).
    pub critical: Option<i32>,
    /// Findings by severity.
    pub high: Option<i32>,
    /// Findings by severity.
    pub medium: Option<i32>,
    /// Findings by severity.
    pub low: Option<i32>,
    /// The worst findings.
    pub top: serde_json::Value,
}

/// Makes a row for each listed image (existing rows keep what they know and take the new
/// role, tag and note).
///
/// # Errors
/// Database errors.
pub async fn sync(
    pool: &PgPool,
    entries: &[(String, String, Option<String>, String)],
) -> Result<(), sqlx::Error> {
    for (reference, role, tag, note) in entries {
        sqlx::query!(
            "INSERT INTO image_scans (reference, role, tag, note) VALUES ($1, $2, $3, $4)
             ON CONFLICT (reference) DO UPDATE SET role = $2, tag = $3, note = $4",
            reference,
            role,
            tag.as_deref(),
            note
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Every image, by role.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Image>, sqlx::Error> {
    sqlx::query_as!(
        Image,
        r#"SELECT reference, role, tag, note, latest_digest,
                  extract(epoch FROM checked_at)::bigint AS checked_at,
                  extract(epoch FROM scanned_at)::bigint AS scanned_at,
                  critical, high, medium, low, top
           FROM image_scans ORDER BY role, reference"#
    )
    .fetch_all(pool)
    .await
}

/// One image.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, reference: &str) -> Result<Option<Image>, sqlx::Error> {
    sqlx::query_as!(
        Image,
        r#"SELECT reference, role, tag, note, latest_digest,
                  extract(epoch FROM checked_at)::bigint AS checked_at,
                  extract(epoch FROM scanned_at)::bigint AS scanned_at,
                  critical, high, medium, low, top
           FROM image_scans WHERE reference = $1"#,
        reference
    )
    .fetch_optional(pool)
    .await
}

/// Asks for a scan as soon as possible. `false` when there is no such image.
///
/// # Errors
/// Database errors.
pub async fn request_scan(pool: &PgPool, reference: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE image_scans SET scan_requested = true, scan_started_at = NULL WHERE reference = $1",
        reference
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

/// The image to scan next: one asked for, else the one never scanned or scanned longest ago
/// (more than `older_than_secs` ago), skipping one sent within the last 40 minutes.
///
/// # Errors
/// Database errors.
pub async fn next_to_scan(
    pool: &PgPool,
    older_than_secs: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT reference FROM image_scans
           WHERE (scan_started_at IS NULL OR scan_started_at < now() - interval '40 minutes')
             AND (scan_requested OR scanned_at IS NULL
                  OR scanned_at < now() - make_interval(secs => $1::int))
           ORDER BY scan_requested DESC, scanned_at NULLS FIRST, reference
           LIMIT 1"#,
        i32::try_from(older_than_secs).unwrap_or(i32::MAX)
    )
    .fetch_optional(pool)
    .await
}

/// A scan of the image was sent.
///
/// # Errors
/// Database errors.
pub async fn scan_started(pool: &PgPool, reference: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE image_scans SET scan_started_at = now() WHERE reference = $1",
        reference
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The answer of a scan.
///
/// # Errors
/// Database errors.
pub async fn store_scan(
    pool: &PgPool,
    reference: &str,
    counts: [i32; 4],
    top: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE image_scans
         SET scanned_at = now(), critical = $2, high = $3, medium = $4, low = $5, top = $6,
             scan_requested = false, scan_started_at = NULL
         WHERE reference = $1",
        reference,
        counts[0],
        counts[1],
        counts[2],
        counts[3],
        top
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// What a registry said a `name:tag` is now: kept for the rows of that name and tag.
///
/// # Errors
/// Database errors.
pub async fn store_digest(
    pool: &PgPool,
    name: &str,
    tag: &str,
    digest: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE image_scans SET latest_digest = $3, checked_at = now()
         WHERE tag = $2 AND split_part(reference, '@', 1) = $1",
        name,
        tag,
        digest
    )
    .execute(pool)
    .await?;
    Ok(())
}
