//! The `git_deploys` table (T12.09): sites and applications deployed from a git repository.

use sqlx::{PgConnection, PgPool, types::Uuid};

/// One deploy of an account.
#[derive(Debug, Clone)]
pub struct GitDeploy {
    /// Id.
    pub id: Uuid,
    /// The account.
    pub tenant_id: Uuid,
    /// Short name.
    pub name: String,
    /// The repository.
    pub repo_url: String,
    /// The branch.
    pub branch: String,
    /// The folder below the web directory that links to the release in use.
    pub dest: String,
    /// The build steps (each a line of words).
    pub build_steps: Vec<String>,
    /// `node` or `python` when there are build steps.
    pub build_kind: Option<String>,
    /// The runtime version for the build.
    pub build_version: Option<String>,
    /// The application restarted afterwards.
    pub app_id: Option<Uuid>,
    /// The sealed token for a private repository.
    pub sealed_token: Option<Vec<u8>>,
    /// The sealed webhook secret.
    pub sealed_webhook: Vec<u8>,
    /// `idle`, `deploying`, `ok`, `failed` or `removing`.
    pub status: String,
    /// The operation awaited.
    pub op_id: Option<Uuid>,
    /// The commit that is live.
    pub last_commit: Option<String>,
    /// The end of the last deploy's log.
    pub last_log: Option<String>,
    /// Why the last deploy failed.
    pub last_error: Option<String>,
    /// When the last deploy went live (Unix seconds).
    pub last_deployed_at: Option<i64>,
}

/// What a sealed token is sealed to.
#[must_use]
pub fn token_context(id: Uuid) -> Vec<u8> {
    format!("git-deploy-token:{id}").into_bytes()
}

/// What a sealed webhook secret is sealed to.
#[must_use]
pub fn webhook_context(id: Uuid) -> Vec<u8> {
    format!("git-deploy-webhook:{id}").into_bytes()
}

struct Row {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    repo_url: String,
    branch: String,
    dest: String,
    build_steps: Vec<String>,
    build_kind: Option<String>,
    build_version: Option<String>,
    app_id: Option<Uuid>,
    sealed_token: Option<Vec<u8>>,
    sealed_webhook: Vec<u8>,
    status: String,
    op_id: Option<Uuid>,
    last_commit: Option<String>,
    last_log: Option<String>,
    last_error: Option<String>,
    last_deployed_at: Option<i64>,
}

impl From<Row> for GitDeploy {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            repo_url: r.repo_url,
            branch: r.branch,
            dest: r.dest,
            build_steps: r.build_steps,
            build_kind: r.build_kind,
            build_version: r.build_version,
            app_id: r.app_id,
            sealed_token: r.sealed_token,
            sealed_webhook: r.sealed_webhook,
            status: r.status,
            op_id: r.op_id,
            last_commit: r.last_commit,
            last_log: r.last_log,
            last_error: r.last_error,
            last_deployed_at: r.last_deployed_at,
        }
    }
}

/// An account's deploys, by name.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<GitDeploy>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, repo_url, branch, dest, build_steps, build_kind,
                  build_version, app_id, sealed_token, sealed_webhook, status, op_id,
                  last_commit, last_log, last_error,
                  extract(epoch FROM last_deployed_at)::bigint AS last_deployed_at
           FROM git_deploys WHERE tenant_id = $1 ORDER BY name"#,
        tenant_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One deploy of an account.
///
/// # Errors
/// Database errors.
pub async fn get_in(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    id: Uuid,
) -> Result<Option<GitDeploy>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, repo_url, branch, dest, build_steps, build_kind,
                  build_version, app_id, sealed_token, sealed_webhook, status, op_id,
                  last_commit, last_log, last_error,
                  extract(epoch FROM last_deployed_at)::bigint AS last_deployed_at
           FROM git_deploys WHERE tenant_id = $1 AND id = $2 FOR UPDATE"#,
        tenant_id,
        id
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(Into::into))
}

/// One deploy by id alone (for the webhook, which knows only the id), locked.
///
/// # Errors
/// Database errors.
pub async fn by_id_in(conn: &mut PgConnection, id: Uuid) -> Result<Option<GitDeploy>, sqlx::Error> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, repo_url, branch, dest, build_steps, build_kind,
                  build_version, app_id, sealed_token, sealed_webhook, status, op_id,
                  last_commit, last_log, last_error,
                  extract(epoch FROM last_deployed_at)::bigint AS last_deployed_at
           FROM git_deploys WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(Into::into))
}

/// The webhook secret, sealed, of a deploy (no lock).
///
/// # Errors
/// Database errors.
pub async fn sealed_webhook(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar!("SELECT sealed_webhook FROM git_deploys WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

/// The token, sealed, of a deploy (no lock).
///
/// # Errors
/// Database errors.
pub async fn sealed_token(pool: &PgPool, id: Uuid) -> Result<Option<Vec<u8>>, sqlx::Error> {
    Ok(
        sqlx::query_scalar!("SELECT sealed_token FROM git_deploys WHERE id = $1", id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// How many deploys an account has.
///
/// # Errors
/// Database errors.
pub async fn count(conn: &mut PgConnection, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM git_deploys WHERE tenant_id = $1 AND status <> 'removing'"#,
        tenant_id
    )
    .fetch_one(conn)
    .await
}

/// What to store.
#[derive(Debug, Clone)]
pub struct Write<'a> {
    /// Short name.
    pub name: &'a str,
    /// Repository.
    pub repo_url: &'a str,
    /// Branch.
    pub branch: &'a str,
    /// Destination folder.
    pub dest: &'a str,
    /// Build steps.
    pub build_steps: &'a [String],
    /// Build runtime.
    pub build_kind: Option<&'a str>,
    /// Build runtime version.
    pub build_version: Option<&'a str>,
    /// Application to restart.
    pub app_id: Option<Uuid>,
}

/// Why a write failed.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The name or the folder is taken.
    #[error("a deploy with this name or folder exists")]
    Taken,
    /// The application is not the account's.
    #[error("the application is not one of the account's")]
    UnknownApp,
    /// Anything else.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn classify(e: sqlx::Error) -> WriteError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => WriteError::Taken,
        _ => WriteError::Database(e),
    }
}

async fn check_app(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    app_id: Option<Uuid>,
) -> Result<(), WriteError> {
    if let Some(app) = app_id {
        let own = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM runtime_apps WHERE id = $1 AND tenant_id = $2) AS "own!""#,
            app,
            tenant_id
        )
        .fetch_one(conn)
        .await?;
        if !own {
            return Err(WriteError::UnknownApp);
        }
    }
    Ok(())
}

/// Makes a deploy (idle). Returns its id.
///
/// # Errors
/// [`WriteError`].
pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    tenant_id: Uuid,
    w: &Write<'_>,
    sealed_webhook: &[u8],
) -> Result<Uuid, WriteError> {
    check_app(conn, tenant_id, w.app_id).await?;
    sqlx::query_scalar!(
        "INSERT INTO git_deploys (id, tenant_id, name, repo_url, branch, dest, build_steps,
                                  build_kind, build_version, app_id, sealed_webhook)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
        id,
        tenant_id,
        w.name,
        w.repo_url,
        w.branch,
        w.dest,
        w.build_steps,
        w.build_kind,
        w.build_version,
        w.app_id,
        sealed_webhook,
    )
    .fetch_one(conn)
    .await
    .map_err(classify)
}

/// Stores the sealed token (`None` removes it).
///
/// # Errors
/// Database errors.
pub async fn set_token(
    conn: &mut PgConnection,
    id: Uuid,
    sealed: Option<&[u8]>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE git_deploys SET sealed_token = $2 WHERE id = $1",
        id,
        sealed
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Notes the operation a deploy waits for and its status.
///
/// # Errors
/// Database errors.
pub async fn waiting_for(
    conn: &mut PgConnection,
    id: Uuid,
    op_id: Uuid,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE git_deploys SET op_id = $2, status = $3, last_error = NULL, updated_at = now() WHERE id = $1",
        id,
        op_id,
        status
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deploys whose answer is awaited.
///
/// # Errors
/// Database errors.
pub async fn awaiting(pool: &PgPool) -> Result<Vec<GitDeploy>, sqlx::Error> {
    let rows = sqlx::query_as!(
        Row,
        r#"SELECT id, tenant_id, name, repo_url, branch, dest, build_steps, build_kind,
                  build_version, app_id, sealed_token, sealed_webhook, status, op_id,
                  last_commit, last_log, last_error,
                  extract(epoch FROM last_deployed_at)::bigint AS last_deployed_at
           FROM git_deploys WHERE op_id IS NOT NULL AND status IN ('deploying', 'removing')"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The release is live.
///
/// # Errors
/// Database errors.
pub async fn deployed(pool: &PgPool, id: Uuid, commit: &str, log: &str) -> Result<(), sqlx::Error> {
    let log: String = log
        .chars()
        .rev()
        .take(8000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    sqlx::query!(
        "UPDATE git_deploys
            SET status = 'ok', op_id = NULL, last_commit = $2, last_log = $3, last_error = NULL,
                last_deployed_at = now(), updated_at = now()
          WHERE id = $1",
        id,
        commit,
        log
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The deploy failed.
///
/// # Errors
/// Database errors.
pub async fn failed(pool: &PgPool, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(500)
        .collect();
    sqlx::query!(
        "UPDATE git_deploys SET status = 'failed', op_id = NULL, last_error = $2, updated_at = now() WHERE id = $1",
        id,
        error
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// The node removed what it kept: the row goes.
///
/// # Errors
/// Database errors.
pub async fn forget(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM git_deploys WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}
