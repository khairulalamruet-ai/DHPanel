//! Bringing an account over from another hosting panel (T15.04, T15.06).
//!
//! | Method | Path | Needs |
//! |---|---|---|
//! | GET, POST | `/api/v1/admin/migration/transfers` | `admin.migration.read`, `admin.migration.manage` |
//! | GET, DELETE | `/api/v1/admin/migration/transfers/{id}` | read, manage |
//! | POST | `/api/v1/admin/migration/transfers/{id}/retry` | manage |
//! | POST | `/api/v1/admin/migration/transfers/{id}/cancel` | manage |
//!
//! A transfer reads a backup file that waits in the panel's import folder
//! (`<imports.spool_dir>/migration/<file>`), works out what importing it would do
//! ([`dhpanel_import::plan`], a pure function: nothing is written) and keeps the answer, the
//! steps and a log. A dry run ends there. Fetching the backup from another server (T15.05) and
//! the real import (the executor) come on top of this record.

use std::{fs::File, io::BufReader, path::PathBuf};

pub mod exec;

use axum::{
    Json, Router,
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::{get, post},
};
use dhpanel_audit::{AuditEntry, Outcome};
use dhpanel_database::{
    migration_transfers::{self as store, NewTransfer, Progress, Transfer},
    sqlx::PgPool,
};
use dhpanel_http_kit::{AppError, FieldError, ProblemDetails};
use dhpanel_import::{Existing, Include, Limits, OnConflict, Options, SourceKind};
use dhpanel_rbac::{
    Principal, Require,
    permissions::{MigrationManage, MigrationRead},
};
use dhpanel_session::{SessionStore, rfc3339};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// The module's manifest.
pub const MANIFEST: &str = include_str!("../../../../modules/migration/module.toml");

/// Most transfers listed.
const LIST_LIMIT: i64 = 200;

/// What the module's handlers share.
#[derive(Clone)]
pub struct MigrationState {
    /// Database pool.
    pub db: PgPool,
    /// Session store.
    pub sessions: SessionStore,
    /// The import folder's parent (`imports.spool_dir`); backups wait in its `migration` folder.
    pub spool_dir: PathBuf,
    /// How the executor uses the panel's own API.
    pub loopback: exec::Loopback,
}

impl FromRef<MigrationState> for PgPool {
    fn from_ref(s: &MigrationState) -> Self {
        s.db.clone()
    }
}

impl FromRef<MigrationState> for SessionStore {
    fn from_ref(s: &MigrationState) -> Self {
        s.sessions.clone()
    }
}

/// The administrator's routes.
pub fn admin_router() -> Router<MigrationState> {
    Router::new()
        .route("/api/v1/admin/migration/transfers", get(list).post(create))
        .route(
            "/api/v1/admin/migration/transfers/{id}",
            get(show).delete(remove),
        )
        .route("/api/v1/admin/migration/transfers/{id}/retry", post(retry))
        .route(
            "/api/v1/admin/migration/transfers/{id}/cancel",
            post(cancel),
        )
}

fn internal(err: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>) -> AppError {
    AppError::internal(err)
}

fn audit(action: &'static str, who: &Principal, id: Uuid) -> AuditEntry {
    AuditEntry::new(action, Outcome::Success)
        .actor(who.account_id())
        .target("migration_transfer", id)
}

// ------------------------------------------------------------------------- views

/// One step of a transfer.
#[derive(Debug, Serialize, ToSchema)]
pub struct StepView {
    /// Id, `step-<n>`.
    pub id: String,
    /// What it does.
    pub name: String,
    /// `pending`, `running`, `completed` or `failed`.
    pub status: String,
    /// What it did or why it failed.
    pub details: Option<String>,
}

/// A transfer.
#[derive(Debug, Serialize, ToSchema)]
pub struct TransferView {
    /// Id.
    pub id: Uuid,
    /// `cpanel`, `cyberpanel`, `openpanel` or `dhpanel_archive`.
    pub source_type: String,
    /// The server it was fetched from.
    pub source_host: Option<String>,
    /// The backup's file name.
    pub backup_file: String,
    /// The account's name here.
    pub tenant_username: String,
    /// The main domain here.
    pub target_domain: String,
    /// Whether it only plans.
    pub dry_run: bool,
    /// `pending`, `running`, `completed`, `failed` or `cancelled`.
    pub status: String,
    /// 0 to 100.
    pub progress_percent: i32,
    /// The step being done.
    pub current_step: String,
    /// The steps, in order.
    pub steps: Vec<StepView>,
    /// The log (the last 2000 lines).
    pub logs: Vec<String>,
    /// The backup's size, in bytes.
    pub size_bytes: Option<i64>,
    /// What the import does and what stands in its way (the dry run's answer).
    pub plan: Option<Value>,
    /// Why it failed.
    pub error_message: Option<String>,
    /// RFC 3339, UTC.
    #[schema(format = DateTime)]
    pub created_at: String,
    /// RFC 3339, UTC.
    #[schema(format = DateTime)]
    pub updated_at: String,
}

fn steps_of(v: &Value) -> Vec<StepView> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    Some(StepView {
                        id: s.get("id")?.as_str()?.to_owned(),
                        name: s.get("name")?.as_str()?.to_owned(),
                        status: s.get("status")?.as_str()?.to_owned(),
                        details: s.get("details").and_then(Value::as_str).map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn view(t: Transfer) -> TransferView {
    TransferView {
        id: t.id,
        source_type: t.source_type,
        source_host: t.source_host,
        backup_file: t.backup_file,
        tenant_username: t.tenant_username,
        target_domain: t.target_domain,
        dry_run: t.dry_run,
        status: t.status,
        progress_percent: t.progress_percent,
        current_step: t.current_step,
        steps: steps_of(&t.steps),
        logs: t
            .logs
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        size_bytes: t.size_bytes,
        plan: t.plan,
        error_message: t.error_message,
        created_at: rfc3339(t.created_at),
        updated_at: rfc3339(t.updated_at),
    }
}

// --------------------------------------------------------------------- the dry run

/// What a dry run found.
struct Planned {
    plan: dhpanel_import::Plan,
    size: i64,
}

/// A backup file name that stays in the import folder.
fn valid_file_name(name: &str) -> bool {
    (1..=200).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn conflict_policy(text: &str) -> OnConflict {
    match text {
        "rename" => OnConflict::Rename,
        "skip" => OnConflict::Skip,
        _ => OnConflict::Fail,
    }
}

fn kind_name(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Cpanel => "cpanel",
        SourceKind::Cyberpanel => "cyberpanel",
    }
}

/// Reads the backup and plans the import. Reading is blocking work.
async fn dry_run(state: &MigrationState, t: &Transfer) -> Result<Planned, String> {
    let path = state.spool_dir.join("migration").join(&t.backup_file);
    let taken = store::taken(&state.db)
        .await
        .map_err(|e| format!("the panel's names could not be read: {e}"))?;
    let options = Options {
        username: Some(t.tenant_username.clone()),
        main_domain: Some(t.target_domain.clone()),
        on_conflict: conflict_policy(&t.on_conflict),
        include: Include::default(),
        mail_supported: false,
    };
    let source_type = t.source_type.clone();
    tokio::task::spawn_blocking(move || {
        let file = File::open(&path)
            .map_err(|_| "the backup file is not in the import folder".to_owned())?;
        let size = file
            .metadata()
            .map_or(0, |m| i64::try_from(m.len()).unwrap_or(i64::MAX));
        let model = dhpanel_import::read_backup(BufReader::new(file), Limits::default())
            .map_err(|e| e.to_string())?;
        let found = kind_name(model.source.kind);
        if found != source_type {
            return Err(format!("the backup is from {found}, not {source_type}"));
        }
        let existing = Existing {
            usernames: taken.usernames.into_iter().collect(),
            domains: taken.domains.into_iter().collect(),
            db_names: taken.db_names.into_iter().collect(),
        };
        Ok(Planned {
            plan: dhpanel_import::plan(&model, &existing, &options),
            size,
        })
    })
    .await
    .map_err(|e| format!("reading the backup stopped: {e}"))?
}

fn describe(step: &dhpanel_import::Step) -> String {
    use dhpanel_import::Action::{
        AddCronJob, AddDomain, CreateAccount, CreateDatabase, CreateDbUser, CreateDnsZone,
        CreateForwarder, CreateMailbox, ImportDump, RestoreFiles,
    };
    let what = match step.action {
        CreateAccount => "Create the account",
        RestoreFiles => "Restore the files of",
        AddDomain => "Add the domain",
        CreateDatabase => "Create the database",
        ImportDump => "Load the dump into",
        CreateDbUser => "Create the database user",
        CreateDnsZone => "Create the DNS zone",
        AddCronJob => "Add the scheduled job",
        CreateMailbox => "Create the mailbox",
        CreateForwarder => "Create the forwarder",
    };
    match &step.renamed_from {
        Some(old) => format!("{what} {} (was {old})", step.subject),
        None => format!("{what} {}", step.subject),
    }
}

/// Plans a transfer and records the outcome. A dry run ends there; a real import goes on
/// (`true`) when nothing stands in its way.
async fn plan_transfer(state: &MigrationState, t: &Transfer) -> Result<bool, AppError> {
    let started = vec!["Reading the backup...".to_owned()];
    store::append_log(&state.db, t.id, &started)
        .await
        .map_err(internal)?;
    match dry_run(state, t).await {
        Ok(Planned { plan, size }) => {
            let steps: Vec<Value> = plan
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    json!({"id": format!("step-{}", i + 1), "name": describe(s),
                           "status": if t.dry_run { "completed" } else { "pending" }})
                })
                .collect();
            let mut log = vec![format!(
                "{} step(s), {} byte(s) of files, {} byte(s) of dumps.",
                plan.steps.len(),
                plan.file_bytes,
                plan.dump_bytes
            )];
            for c in &plan.conflicts {
                log.push(format!(
                    "Conflict: {} `{}` ({:?}).",
                    c.kind, c.name, c.resolution
                ));
            }
            for s in &plan.skipped {
                log.push(format!("Left out: {} `{}`: {}.", s.kind, s.name, s.reason));
            }
            for w in &plan.warnings {
                log.push(format!("Warning: {w:?}"));
            }
            for b in &plan.blockers {
                log.push(format!("Blocked: {b}."));
            }
            log.push(match (plan.ok, t.dry_run) {
                (true, true) => "Dry run finished: nothing was changed.".to_owned(),
                (true, false) => "The plan is fine: the import starts.".to_owned(),
                (false, _) => {
                    "The import cannot run until the problems above are settled.".to_owned()
                }
            });
            store::append_log(&state.db, t.id, &log)
                .await
                .map_err(internal)?;
            let plan_json = serde_json::to_value(&plan).map_err(internal)?;
            let steps_json = Value::Array(steps);
            let go = plan.ok && !t.dry_run;
            let (status, percent, current) = match (plan.ok, t.dry_run) {
                (true, true) => ("completed", 100, "Dry run finished"),
                (true, false) => ("running", 0, "Starting"),
                (false, _) => ("failed", 0, "The plan has problems"),
            };
            store::set_progress(
                &state.db,
                t.id,
                &Progress {
                    status,
                    progress_percent: percent,
                    current_step: current,
                    size_bytes: Some(size),
                    steps: &steps_json,
                    plan: Some(&plan_json),
                    error_message: (!plan.ok)
                        .then_some("The plan has problems that stop the import."),
                },
            )
            .await
            .map_err(internal)?;
            Ok(go)
        }
        Err(why) => {
            store::append_log(&state.db, t.id, &[format!("Failed: {why}")])
                .await
                .map_err(internal)?;
            let none = json!([]);
            store::set_progress(
                &state.db,
                t.id,
                &Progress {
                    status: "failed",
                    progress_percent: 0,
                    current_step: "Reading the backup",
                    size_bytes: None,
                    steps: &none,
                    plan: None,
                    error_message: Some(&why.chars().take(900).collect::<String>()),
                },
            )
            .await
            .map_err(internal)?;
            Ok(false)
        }
    }
}

/// Plans, and for a real import starts the executor in the background.
async fn start(state: &MigrationState, t: &Transfer) -> Result<(), AppError> {
    if plan_transfer(state, t).await? {
        tokio::spawn(exec::run(state.clone(), t.id));
    }
    Ok(())
}

// ---------------------------------------------------------------------- handlers

/// The transfers, newest first.
#[utoipa::path(
    get, path = "/api/v1/admin/migration/transfers", tag = "migration",
    operation_id = "listMigrationTransfers", security(("session" = [])),
    responses(
        (status = 200, description = "The transfers", body = Vec<TransferView>),
        (status = 403, description = "`permission_denied`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn list(
    State(state): State<MigrationState>,
    Require(_who, _): Require<MigrationRead>,
) -> Result<Json<Vec<TransferView>>, AppError> {
    let rows = store::list(&state.db, LIST_LIMIT).await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(view).collect()))
}

/// One transfer, with its steps and log.
#[utoipa::path(
    get, path = "/api/v1/admin/migration/transfers/{id}", tag = "migration",
    operation_id = "getMigrationTransfer", security(("session" = [])),
    params(("id" = Uuid, Path, description = "Transfer id")),
    responses(
        (status = 200, description = "The transfer", body = TransferView),
        (status = 404, description = "No such transfer", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn show(
    State(state): State<MigrationState>,
    Require(_who, _): Require<MigrationRead>,
    Path(id): Path<Uuid>,
) -> Result<Json<TransferView>, AppError> {
    let t = store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    Ok(Json(view(t)))
}

/// A transfer to start.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTransfer {
    /// `cpanel` or `cyberpanel` (`openpanel` and `dhpanel_archive` are not readable yet).
    pub source_type: String,
    /// The backup's file name in the import folder.
    pub backup_file: String,
    /// The account's name here.
    pub tenant_username: String,
    /// The main domain here.
    pub target_domain: String,
    /// Only plan (default `true`). With `false` the plan is carried out when nothing stands in
    /// its way.
    #[serde(default = "yes")]
    pub dry_run: bool,
    /// `fail` (default), `rename` or `skip`.
    #[serde(default)]
    pub on_conflict: Option<String>,
    /// Fetching the backup from another server is not available yet (T15.05): must be absent.
    #[serde(default)]
    pub remote_host: Option<String>,
}

fn yes() -> bool {
    true
}

/// Start a transfer: read the backup and plan the import.
#[utoipa::path(
    post, path = "/api/v1/admin/migration/transfers", tag = "migration",
    operation_id = "createMigrationTransfer", security(("session" = [])),
    request_body = CreateTransfer,
    responses(
        (status = 201, description = "Created and, for a dry run, finished", body = TransferView),
        (status = 409, description = "`import_unavailable`: this panel cannot run imports", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid input", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn create(
    State(state): State<MigrationState>,
    Require(who, _): Require<MigrationManage>,
    Json(req): Json<CreateTransfer>,
) -> Result<(StatusCode, Json<TransferView>), AppError> {
    let mut errors = Vec::new();
    if !matches!(req.source_type.as_str(), "cpanel" | "cyberpanel") {
        errors.push(FieldError::new(
            "source_type",
            "unsupported_source",
            "Only cpanel and cyberpanel backups can be read.",
        ));
    }
    if !valid_file_name(&req.backup_file) {
        errors.push(FieldError::new(
            "backup_file",
            "invalid_file_name",
            "Use the file's name in the import folder (letters, digits, . _ -).",
        ));
    }
    if dhpanel_domain::TenantUsername::parse(&req.tenant_username.to_ascii_lowercase()).is_err() {
        errors.push(FieldError::new(
            "tenant_username",
            "invalid_username",
            "Use 3 to 32 lower-case letters and digits, starting with a letter.",
        ));
    }
    if dhpanel_domain::Fqdn::parse(&req.target_domain.to_ascii_lowercase()).is_err() {
        errors.push(FieldError::new(
            "target_domain",
            "invalid_domain",
            "Not a valid domain name.",
        ));
    }
    let policy = req.on_conflict.as_deref().unwrap_or("fail");
    if !matches!(policy, "fail" | "rename" | "skip") {
        errors.push(FieldError::new(
            "on_conflict",
            "invalid_policy",
            "Use fail, rename or skip.",
        ));
    }
    if req.remote_host.is_some() {
        errors.push(FieldError::new(
            "remote_host",
            "remote_fetch_unavailable",
            "Fetching a backup from another server is not available yet; put the file in the import folder.",
        ));
    }
    if !errors.is_empty() {
        return Err(AppError::validation(errors));
    }
    if !req.dry_run && state.loopback.0.get().is_none() {
        return Err(AppError::conflict(
            "import_unavailable",
            "This panel cannot run imports.",
        ));
    }
    let t = store::insert(
        &state.db,
        &NewTransfer {
            source_type: &req.source_type,
            source_host: None,
            backup_file: &req.backup_file,
            tenant_username: &req.tenant_username.to_ascii_lowercase(),
            target_domain: &req.target_domain.to_ascii_lowercase(),
            dry_run: req.dry_run,
            on_conflict: policy,
            created_by: who.account_id(),
        },
    )
    .await
    .map_err(internal)?;
    dhpanel_audit::insert(&state.db, &audit("migration.transfer_started", &who, t.id))
        .await
        .map_err(internal)?;
    start(&state, &t).await?;
    let done = store::get(&state.db, t.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    Ok((StatusCode::CREATED, Json(view(done))))
}

/// Run a failed or cancelled transfer again from its first step.
#[utoipa::path(
    post, path = "/api/v1/admin/migration/transfers/{id}/retry", tag = "migration",
    operation_id = "retryMigrationTransfer", security(("session" = [])),
    params(("id" = Uuid, Path, description = "Transfer id")),
    responses(
        (status = 200, description = "Run again", body = TransferView),
        (status = 404, description = "No such transfer", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`not_retryable`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn retry(
    State(state): State<MigrationState>,
    Require(who, _): Require<MigrationManage>,
    Path(id): Path<Uuid>,
) -> Result<Json<TransferView>, AppError> {
    let existing = store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    // A real import that got as far as its steps goes on from the step that stopped it.
    let resumed = !existing.dry_run
        && state.loopback.0.get().is_some()
        && store::resume(&state.db, id).await.map_err(internal)?;
    if resumed {
        dhpanel_audit::insert(&state.db, &audit("migration.transfer_retried", &who, id))
            .await
            .map_err(internal)?;
        store::append_log(
            &state.db,
            id,
            &["Going on from the step that stopped it.".to_owned()],
        )
        .await
        .map_err(internal)?;
        tokio::spawn(exec::run(state.clone(), id));
    } else {
        if !store::requeue(&state.db, id).await.map_err(internal)? {
            return Err(AppError::conflict(
                "not_retryable",
                "Only a transfer that failed or was cancelled can be run again.",
            ));
        }
        let t = store::get(&state.db, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| AppError::not_found("transfer"))?;
        dhpanel_audit::insert(&state.db, &audit("migration.transfer_retried", &who, id))
            .await
            .map_err(internal)?;
        store::append_log(
            &state.db,
            id,
            &["Running again from the first step.".to_owned()],
        )
        .await
        .map_err(internal)?;
        start(&state, &t).await?;
    }
    let done = store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    Ok(Json(view(done)))
}

/// Stop a transfer that has not ended.
#[utoipa::path(
    post, path = "/api/v1/admin/migration/transfers/{id}/cancel", tag = "migration",
    operation_id = "cancelMigrationTransfer", security(("session" = [])),
    params(("id" = Uuid, Path, description = "Transfer id")),
    responses(
        (status = 200, description = "Cancelled", body = TransferView),
        (status = 404, description = "No such transfer", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`not_cancellable`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn cancel(
    State(state): State<MigrationState>,
    Require(who, _): Require<MigrationManage>,
    Path(id): Path<Uuid>,
) -> Result<Json<TransferView>, AppError> {
    store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    if !store::cancel(&state.db, id).await.map_err(internal)? {
        return Err(AppError::conflict(
            "not_cancellable",
            "This transfer has already ended.",
        ));
    }
    dhpanel_audit::insert(&state.db, &audit("migration.transfer_cancelled", &who, id))
        .await
        .map_err(internal)?;
    let t = store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    Ok(Json(view(t)))
}

/// Delete a transfer that has ended.
#[utoipa::path(
    delete, path = "/api/v1/admin/migration/transfers/{id}", tag = "migration",
    operation_id = "deleteMigrationTransfer", security(("session" = [])),
    params(("id" = Uuid, Path, description = "Transfer id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No such transfer", body = ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "`still_running`", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn remove(
    State(state): State<MigrationState>,
    Require(who, _): Require<MigrationManage>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    store::get(&state.db, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| AppError::not_found("transfer"))?;
    if !store::delete(&state.db, id).await.map_err(internal)? {
        return Err(AppError::conflict(
            "still_running",
            "Cancel the transfer before deleting it.",
        ));
    }
    dhpanel_audit::insert(&state.db, &audit("migration.transfer_deleted", &who, id))
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI description of the module.
#[derive(OpenApi)]
#[openapi(
    paths(list, show, create, retry, cancel, remove),
    components(schemas(TransferView, StepView, CreateTransfer)),
    tags((name = "migration", description = "Bring an account over from another hosting panel"))
)]
pub struct MigrationApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_stay_in_the_folder() {
        assert!(valid_file_name("cpmove-alice.tar.gz"));
        assert!(!valid_file_name("../etc/passwd"));
        assert!(!valid_file_name("a/b.tar"));
        assert!(!valid_file_name(".hidden"));
        assert!(!valid_file_name("a..b"));
        assert!(!valid_file_name(""));
    }
}
