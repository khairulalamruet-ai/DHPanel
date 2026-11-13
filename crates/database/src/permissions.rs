//! The permission catalogue (`permissions`): which keys exist and which module
//! declares each.

use sqlx::PgPool;

/// What [`sync`] changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Keys seen for the first time.
    pub added: Vec<String>,
    /// Keys that were missing and are declared again.
    pub restored: Vec<String>,
    /// Keys now declared by another module.
    pub moved: Vec<String>,
    /// Keys the running build no longer declares.
    pub missing: Vec<String>,
}

impl SyncReport {
    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.restored.is_empty()
            && self.moved.is_empty()
            && self.missing.is_empty()
    }
}

/// A catalogue entry with its module's display data.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key.
    pub key: String,
    /// The declaring module.
    pub module: String,
    /// The module's title by language, when the module is known.
    pub module_title: Option<serde_json::Value>,
    /// Declared by the running build.
    pub present: bool,
}

/// Advisory lock shared with the module sync: both run at start.
const SYNC_LOCK: i64 = 7_303_002;

/// Makes the catalogue match the build. `keys` are `(key, declaring module)`.
/// One transaction; a key never disappears, it is marked `missing`.
///
/// # Errors
/// Database errors.
pub async fn sync(pool: &PgPool, keys: &[(&str, &str)]) -> Result<SyncReport, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", SYNC_LOCK)
        .execute(&mut *tx)
        .await?;
    let mut report = SyncReport::default();
    for (key, module) in keys {
        let before = sqlx::query!(
            "SELECT module, status FROM permissions WHERE key = $1 FOR UPDATE",
            key,
        )
        .fetch_optional(&mut *tx)
        .await?;
        match before {
            None => {
                sqlx::query!(
                    "INSERT INTO permissions (key, module) VALUES ($1, $2)",
                    key,
                    module
                )
                .execute(&mut *tx)
                .await?;
                report.added.push((*key).to_owned());
            }
            Some(row) if row.module != *module || row.status == "missing" => {
                sqlx::query!(
                    "UPDATE permissions SET module = $2, status = 'present' WHERE key = $1",
                    key,
                    module,
                )
                .execute(&mut *tx)
                .await?;
                if row.status == "missing" {
                    report.restored.push((*key).to_owned());
                } else {
                    report.moved.push((*key).to_owned());
                }
            }
            Some(_) => {}
        }
    }
    let declared: Vec<String> = keys.iter().map(|(k, _)| (*k).to_owned()).collect();
    report.missing = sqlx::query_scalar!(
        r#"UPDATE permissions SET status = 'missing'
           WHERE status = 'present' AND key <> ALL($1)
           RETURNING key"#,
        &declared,
    )
    .fetch_all(&mut *tx)
    .await?;
    report.missing.sort();
    tx.commit().await?;
    Ok(report)
}

/// The whole catalogue, by module then key, with each module's title.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Entry>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT p.key, p.module, p.status = 'present' AS "present!",
                  m.manifest #> '{module,title}' AS module_title
           FROM permissions p LEFT JOIN modules m ON m.name = p.module
           ORDER BY p.module, p.key"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Entry {
            key: r.key,
            module: r.module,
            module_title: r.module_title,
            present: r.present,
        })
        .collect())
}
