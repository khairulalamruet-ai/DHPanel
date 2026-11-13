//! The `modules` table: which modules this installation knows, and which are
//! switched on.

use serde_json::Value;
use sqlx::PgPool;

/// A module of the running build, as written to the table.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleRecord<'a> {
    /// Module name.
    pub name: &'a str,
    /// `MAJOR.MINOR.PATCH`.
    pub version: &'a str,
    /// Category name (`core`, `web`, ...).
    pub category: &'a str,
    /// Cannot be switched off.
    pub core: bool,
    /// The manifest as JSON.
    pub manifest: &'a Value,
}

/// A row of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleRow {
    /// Module name.
    pub name: String,
    /// Version of the build that was last seen.
    pub version: String,
    /// Category name.
    pub category: String,
    /// Cannot be switched off.
    pub core: bool,
    /// Switched on by the administrator (meaningless while `missing`).
    pub enabled: bool,
    /// Whether the running build contains the module.
    pub present: bool,
    /// The manifest as JSON.
    pub manifest: Value,
}

/// What [`sync`] changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Modules seen for the first time.
    pub added: Vec<String>,
    /// Modules that were `missing` and are part of the build again.
    pub restored: Vec<String>,
    /// Modules whose version, category, core flag or manifest changed.
    pub updated: Vec<String>,
    /// Modules that are no longer part of the build.
    pub missing: Vec<String>,
}

impl SyncReport {
    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.restored.is_empty()
            && self.updated.is_empty()
            && self.missing.is_empty()
    }
}

/// Advisory lock that serialises module syncs (two control planes starting at once).
const SYNC_LOCK: i64 = 7_303_001;

/// Makes the table match the build: new modules are added (switched on),
/// known ones are refreshed, and modules that are gone are marked `missing`.
/// An administrator's on/off choice is never changed, except that a module
/// which became core is switched on. All in one transaction.
///
/// # Errors
/// Database errors.
pub async fn sync(pool: &PgPool, modules: &[ModuleRecord<'_>]) -> Result<SyncReport, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", SYNC_LOCK)
        .execute(&mut *tx)
        .await?;
    let mut report = SyncReport::default();
    for module in modules {
        let before = sqlx::query!(
            r#"SELECT version, category, core, status, manifest FROM modules WHERE name = $1 FOR UPDATE"#,
            module.name,
        )
        .fetch_optional(&mut *tx)
        .await?;
        match before {
            None => {
                sqlx::query!(
                    "INSERT INTO modules (name, version, category, core, manifest) VALUES ($1, $2, $3, $4, $5)",
                    module.name,
                    module.version,
                    module.category,
                    module.core,
                    module.manifest,
                )
                .execute(&mut *tx)
                .await?;
                report.added.push(module.name.to_owned());
            }
            Some(row) => {
                let changed = row.version != module.version
                    || row.category != module.category
                    || row.core != module.core
                    || row.manifest != *module.manifest;
                let was_missing = row.status == "missing";
                if changed || was_missing {
                    sqlx::query!(
                        r#"UPDATE modules
                           SET version = $2, category = $3, core = $4, manifest = $5,
                               status = 'present', last_seen_at = now(),
                               enabled = enabled OR $4
                           WHERE name = $1"#,
                        module.name,
                        module.version,
                        module.category,
                        module.core,
                        module.manifest,
                    )
                    .execute(&mut *tx)
                    .await?;
                    if was_missing {
                        report.restored.push(module.name.to_owned());
                    } else {
                        report.updated.push(module.name.to_owned());
                    }
                } else {
                    sqlx::query!(
                        "UPDATE modules SET last_seen_at = now() WHERE name = $1",
                        module.name
                    )
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }
    let names: Vec<String> = modules.iter().map(|m| m.name.to_owned()).collect();
    report.missing = sqlx::query_scalar!(
        r#"UPDATE modules SET status = 'missing'
           WHERE status = 'present' AND name <> ALL($1)
           RETURNING name"#,
        &names,
    )
    .fetch_all(&mut *tx)
    .await?;
    report.missing.sort();
    tx.commit().await?;
    Ok(report)
}

/// All rows, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<ModuleRow>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT name, version, category, core, enabled, status = 'present' AS "present!", manifest
           FROM modules ORDER BY name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ModuleRow {
            name: r.name,
            version: r.version,
            category: r.category,
            core: r.core,
            enabled: r.enabled,
            present: r.present,
            manifest: r.manifest,
        })
        .collect())
}

/// Names of the modules that are part of the build and switched on.
///
/// # Errors
/// Database errors.
pub async fn enabled_names(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT name FROM modules WHERE status = 'present' AND enabled ORDER BY name"
    )
    .fetch_all(pool)
    .await
}
