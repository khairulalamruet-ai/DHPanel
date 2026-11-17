//! Resellers, their packages, their clients and their plans (T14.01 - T14.07).

use serde_json::Value;
use sqlx::{PgConnection, PgPool, types::Uuid};

const GIB: i64 = 1024 * 1024 * 1024;

/// What an administrator gives a reseller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Id.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Most hosting accounts.
    pub max_accounts: i32,
    /// Disk space across all accounts, in GiB.
    pub max_disk_gb: i32,
    /// Monthly transfer across all accounts, in GiB.
    pub max_bandwidth_gb: i32,
    /// Whether the sums of the plans may pass the two caps above.
    pub allow_overselling: bool,
    /// Plans of the administrator the reseller may use as they are.
    pub allowed_plans: Vec<Uuid>,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

/// What to store for a package.
#[derive(Debug, Clone)]
pub struct PackageWrite<'a> {
    /// Name.
    pub name: &'a str,
    /// Most hosting accounts.
    pub max_accounts: i32,
    /// Disk cap in GiB.
    pub max_disk_gb: i32,
    /// Transfer cap in GiB per month.
    pub max_bandwidth_gb: i32,
    /// Overselling.
    pub allow_overselling: bool,
    /// Administrator plans the reseller may use.
    pub allowed_plans: &'a [Uuid],
}

/// Why writing a package failed.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// The name is taken.
    #[error("a package with this name exists")]
    NameTaken,
    /// An allowed plan does not exist or belongs to a reseller.
    #[error("an allowed plan is not one of the administrator's plans")]
    UnknownPlan,
    /// Anything else.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct PackageRow {
    id: Uuid,
    name: String,
    max_accounts: i32,
    max_disk_gb: i32,
    max_bandwidth_gb: i32,
    allow_overselling: bool,
    allowed_plans: Vec<Uuid>,
    created_at: i64,
}

impl From<PackageRow> for Package {
    fn from(r: PackageRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            max_accounts: r.max_accounts,
            max_disk_gb: r.max_disk_gb,
            max_bandwidth_gb: r.max_bandwidth_gb,
            allow_overselling: r.allow_overselling,
            allowed_plans: r.allowed_plans,
            created_at: r.created_at,
        }
    }
}

/// All packages, by name.
///
/// # Errors
/// Database errors.
pub async fn packages(pool: &PgPool) -> Result<Vec<Package>, sqlx::Error> {
    let rows = sqlx::query_as!(
        PackageRow,
        r#"SELECT id, name, max_accounts, max_disk_gb, max_bandwidth_gb, allow_overselling,
                  allowed_plans, extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM reseller_packages ORDER BY lower(name)"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One package.
///
/// # Errors
/// Database errors.
pub async fn package(pool: &PgPool, id: Uuid) -> Result<Option<Package>, sqlx::Error> {
    let row = sqlx::query_as!(
        PackageRow,
        r#"SELECT id, name, max_accounts, max_disk_gb, max_bandwidth_gb, allow_overselling,
                  allowed_plans, extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM reseller_packages WHERE id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Creates a package.
///
/// # Errors
/// [`PackageError`].
pub async fn insert_package(
    conn: &mut PgConnection,
    p: &PackageWrite<'_>,
) -> Result<Uuid, PackageError> {
    let known = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM plans WHERE id = ANY($1) AND reseller_id IS NULL"#,
        p.allowed_plans
    )
    .fetch_one(&mut *conn)
    .await?;
    if usize::try_from(known).ok() != Some(p.allowed_plans.len()) {
        return Err(PackageError::UnknownPlan);
    }
    sqlx::query_scalar!(
        r#"INSERT INTO reseller_packages
               (name, max_accounts, max_disk_gb, max_bandwidth_gb, allow_overselling, allowed_plans)
           VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"#,
        p.name,
        p.max_accounts,
        p.max_disk_gb,
        p.max_bandwidth_gb,
        p.allow_overselling,
        p.allowed_plans,
    )
    .fetch_one(conn)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => PackageError::NameTaken,
        _ => PackageError::Database(e),
    })
}

/// What [`delete_package`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageDelete {
    /// Gone.
    Deleted,
    /// No such package.
    Unknown,
    /// Resellers use it.
    InUse,
}

/// Deletes a package no reseller uses.
///
/// # Errors
/// Database errors.
pub async fn delete_package(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<PackageDelete, sqlx::Error> {
    let used = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM resellers WHERE package_id = $1) AS "used!""#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;
    if used {
        return Ok(PackageDelete::InUse);
    }
    let done = sqlx::query!("DELETE FROM reseller_packages WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(if done.rows_affected() == 0 {
        PackageDelete::Unknown
    } else {
        PackageDelete::Deleted
    })
}

/// A reseller with what is needed to show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reseller {
    /// The account id (also the reseller's id everywhere in the API).
    pub id: Uuid,
    /// Login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// Company name.
    pub company_name: String,
    /// Package.
    pub package_id: Uuid,
    /// Package name.
    pub package_name: String,
    /// `active` or `suspended`.
    pub status: String,
    /// Hosting accounts of its clients (not counting deleted ones).
    pub accounts: i64,
    /// Disk the clients use.
    pub disk_used_bytes: i64,
    /// Data the clients sent out this month.
    pub bandwidth_used_bytes: i64,
    /// Branding.
    pub branding: Value,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

struct ResellerRow {
    id: Uuid,
    username: String,
    email: String,
    company_name: String,
    package_id: Uuid,
    package_name: String,
    status: String,
    accounts: i64,
    disk_used_bytes: i64,
    bandwidth_used_bytes: i64,
    branding: Value,
    created_at: i64,
}

impl From<ResellerRow> for Reseller {
    fn from(r: ResellerRow) -> Self {
        Self {
            id: r.id,
            username: r.username,
            email: r.email,
            company_name: r.company_name,
            package_id: r.package_id,
            package_name: r.package_name,
            status: r.status,
            accounts: r.accounts,
            disk_used_bytes: r.disk_used_bytes,
            bandwidth_used_bytes: r.bandwidth_used_bytes,
            branding: r.branding,
            created_at: r.created_at,
        }
    }
}

/// All resellers, newest first.
///
/// # Errors
/// Database errors.
pub async fn list(pool: &PgPool) -> Result<Vec<Reseller>, sqlx::Error> {
    query(pool, None).await
}

async fn query(pool: &PgPool, only: Option<Uuid>) -> Result<Vec<Reseller>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ResellerRow,
        r#"SELECT r.account_id AS id, a.username, a.email, r.company_name, r.package_id,
                  k.name AS package_name, r.status, r.branding,
                  extract(epoch FROM r.created_at)::bigint AS "created_at!",
                  (SELECT count(*) FROM tenants t
                    WHERE t.reseller_id = r.account_id AND t.status <> 'deleting') AS "accounts!",
                  (SELECT COALESCE(sum(u.total_bytes), 0)::bigint FROM tenants t
                     JOIN tenant_disk_usage u ON u.tenant_id = t.id
                    WHERE t.reseller_id = r.account_id AND t.status <> 'deleting') AS "disk_used_bytes!",
                  (SELECT COALESCE(sum(h.net_tx_bytes), 0)::bigint FROM tenants t
                     JOIN usage_hour h ON h.tenant_id = t.id
                    WHERE t.reseller_id = r.account_id
                      AND h.at >= date_trunc('month', now())) AS "bandwidth_used_bytes!"
           FROM resellers r
           JOIN accounts a ON a.id = r.account_id
           JOIN reseller_packages k ON k.id = r.package_id
           WHERE $1::uuid IS NULL OR r.account_id = $1
           ORDER BY r.created_at DESC, r.account_id DESC"#,
        only
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// One reseller.
///
/// # Errors
/// Database errors.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<Reseller>, sqlx::Error> {
    Ok(query(pool, Some(id)).await?.into_iter().next())
}

/// Makes an existing `reseller` account a reseller.
///
/// # Errors
/// [`sqlx::Error`]; a foreign-key violation when the package does not exist.
pub async fn insert(
    conn: &mut PgConnection,
    account_id: Uuid,
    company_name: &str,
    package_id: Uuid,
    branding: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO resellers (account_id, company_name, package_id, branding)
         VALUES ($1, $2, $3, $4)",
        account_id,
        company_name,
        package_id,
        branding,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Changes the status; returns whether the reseller exists.
///
/// # Errors
/// Database errors.
pub async fn set_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE resellers SET status = $2 WHERE account_id = $1",
        id,
        status
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Replaces the branding; returns whether the reseller exists.
///
/// # Errors
/// Database errors.
pub async fn set_branding(
    conn: &mut PgConnection,
    id: Uuid,
    branding: &Value,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE resellers SET branding = $2 WHERE account_id = $1",
        id,
        branding
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// What a reseller's package and standing allow right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// `active` or `suspended`.
    pub status: String,
    /// Package.
    pub package_id: Uuid,
    /// Most accounts.
    pub max_accounts: i32,
    /// Disk cap in bytes.
    pub max_disk_bytes: i64,
    /// Transfer cap in bytes per month.
    pub max_bandwidth_bytes: i64,
    /// Overselling.
    pub allow_overselling: bool,
    /// Administrator plans allowed.
    pub allowed_plans: Vec<Uuid>,
}

/// The standing of a reseller, locking its row so that two requests cannot both
/// use the last free account.
///
/// # Errors
/// Database errors.
pub async fn standing_locked(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<Standing>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT r.status, r.package_id, k.max_accounts, k.max_disk_gb, k.max_bandwidth_gb,
                  k.allow_overselling, k.allowed_plans
           FROM resellers r JOIN reseller_packages k ON k.id = r.package_id
           WHERE r.account_id = $1 FOR UPDATE OF r"#,
        id
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| Standing {
        status: r.status,
        package_id: r.package_id,
        max_accounts: r.max_accounts,
        max_disk_bytes: i64::from(r.max_disk_gb) * GIB,
        max_bandwidth_bytes: i64::from(r.max_bandwidth_gb) * GIB,
        allow_overselling: r.allow_overselling,
        allowed_plans: r.allowed_plans,
    }))
}

/// What the reseller's clients already hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocated {
    /// Hosting accounts (not counting deleted ones).
    pub accounts: i64,
    /// Sum of the plans' disk space; `None` when a plan has no cap.
    pub disk_bytes: Option<i64>,
    /// Sum of the plans' transfer; `None` when a plan has no cap.
    pub bandwidth_bytes: Option<i64>,
}

/// The accounts and the plan sums of a reseller's clients.
///
/// # Errors
/// Database errors.
pub async fn allocated(conn: &mut PgConnection, id: Uuid) -> Result<Allocated, sqlx::Error> {
    let r = sqlx::query!(
        r#"SELECT count(*) AS "accounts!",
                  CASE WHEN bool_or(p.disk_bytes IS NULL) THEN NULL
                       ELSE COALESCE(sum(p.disk_bytes), 0)::bigint END AS disk_bytes,
                  CASE WHEN bool_or(p.bandwidth_bytes IS NULL) THEN NULL
                       ELSE COALESCE(sum(p.bandwidth_bytes), 0)::bigint END AS bandwidth_bytes
           FROM tenants t JOIN plans p ON p.id = t.plan_id
           WHERE t.reseller_id = $1 AND t.status <> 'deleting'"#,
        id
    )
    .fetch_one(conn)
    .await?;
    Ok(Allocated {
        accounts: r.accounts,
        disk_bytes: r.disk_bytes,
        bandwidth_bytes: r.bandwidth_bytes,
    })
}

/// A plan's reseller and the figures needed to count it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFacts {
    /// Its reseller; `None` for the administrator's plans.
    pub reseller_id: Option<Uuid>,
    /// Disk space.
    pub disk_bytes: Option<i64>,
    /// Transfer.
    pub bandwidth_bytes: Option<i64>,
}

/// Who owns a plan and what it allows.
///
/// # Errors
/// Database errors.
pub async fn plan_facts(
    conn: &mut PgConnection,
    plan_id: Uuid,
) -> Result<Option<PlanFacts>, sqlx::Error> {
    let r = sqlx::query!(
        "SELECT reseller_id, disk_bytes, bandwidth_bytes FROM plans WHERE id = $1",
        plan_id
    )
    .fetch_optional(conn)
    .await?;
    Ok(r.map(|r| PlanFacts {
        reseller_id: r.reseller_id,
        disk_bytes: r.disk_bytes,
        bandwidth_bytes: r.bandwidth_bytes,
    }))
}

/// A plan a reseller made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResellerPlan {
    /// Id.
    pub id: Uuid,
    /// The name the reseller gave it.
    pub name: String,
    /// Disk space in bytes.
    pub disk_bytes: i64,
    /// Transfer in bytes per month.
    pub bandwidth_bytes: i64,
    /// Module limits.
    pub limits: Value,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

struct PlanRow {
    id: Uuid,
    name: Option<String>,
    disk_bytes: Option<i64>,
    bandwidth_bytes: Option<i64>,
    limits: Value,
    created_at: i64,
}

/// The plans of one reseller, by name.
///
/// # Errors
/// Database errors.
pub async fn plans_of(pool: &PgPool, reseller_id: Uuid) -> Result<Vec<ResellerPlan>, sqlx::Error> {
    let rows = sqlx::query_as!(
        PlanRow,
        r#"SELECT id, display_name AS name, disk_bytes, bandwidth_bytes, limits,
                  extract(epoch FROM created_at)::bigint AS "created_at!"
           FROM plans WHERE reseller_id = $1 ORDER BY lower(display_name), id"#,
        reseller_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ResellerPlan {
            id: r.id,
            name: r.name.unwrap_or_default(),
            disk_bytes: r.disk_bytes.unwrap_or(0),
            bandwidth_bytes: r.bandwidth_bytes.unwrap_or(0),
            limits: r.limits,
            created_at: r.created_at,
        })
        .collect())
}

/// Creates a plan for a reseller, on the feature set and the container resources of the
/// administrator's default plan. Returns its id.
///
/// # Errors
/// Database errors; [`sqlx::Error::RowNotFound`] when there is no default plan.
pub async fn insert_plan(
    conn: &mut PgConnection,
    reseller_id: Uuid,
    display_name: &str,
    disk_bytes: i64,
    bandwidth_bytes: i64,
    limits: &Value,
) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();
    let name = format!("rp_{}", &tag[tag.len() - 12..]);
    sqlx::query_scalar!(
        r#"INSERT INTO plans (name, display_name, description, feature_set_id, cpu_millis,
                              memory_bytes, pids, disk_bytes, inodes, bandwidth_bytes, limits,
                              isolation, reseller_id)
           SELECT $1, $2, '', d.feature_set_id, d.cpu_millis, d.memory_bytes, d.pids, $3,
                  d.inodes, $4, $5, d.isolation, $6
           FROM plans d WHERE d.is_default
           RETURNING id"#,
        name,
        display_name,
        disk_bytes,
        bandwidth_bytes,
        limits,
        reseller_id,
    )
    .fetch_one(conn)
    .await
}

/// What [`delete_plan`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanDelete {
    /// Gone.
    Deleted,
    /// Not one of this reseller's plans.
    Unknown,
    /// Accounts are on it.
    InUse,
}

/// Deletes a plan of the reseller that no account uses.
///
/// # Errors
/// Database errors.
pub async fn delete_plan(
    conn: &mut PgConnection,
    reseller_id: Uuid,
    plan_id: Uuid,
) -> Result<PlanDelete, sqlx::Error> {
    let own = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM plans WHERE id = $1 AND reseller_id = $2 FOR UPDATE) AS "own!""#,
        plan_id,
        reseller_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if !own {
        return Ok(PlanDelete::Unknown);
    }
    let used = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM tenants WHERE plan_id = $1) AS "used!""#,
        plan_id
    )
    .fetch_one(&mut *conn)
    .await?;
    if used {
        return Ok(PlanDelete::InUse);
    }
    sqlx::query!("DELETE FROM plans WHERE id = $1", plan_id)
        .execute(conn)
        .await?;
    Ok(PlanDelete::Deleted)
}

/// A client of a reseller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Account id.
    pub id: Uuid,
    /// Login name.
    pub username: String,
    /// Email address.
    pub email: String,
    /// First domain of the account, empty when it has none.
    pub domain: String,
    /// Plan name as the reseller sees it.
    pub plan_name: String,
    /// Disk in use.
    pub disk_used_bytes: i64,
    /// `active` or `suspended`.
    pub status: String,
    /// Creation time (Unix seconds).
    pub created_at: i64,
}

/// The clients of a reseller, newest first (accounts being deleted are left out).
///
/// # Errors
/// Database errors.
pub async fn clients(pool: &PgPool, reseller_id: Uuid) -> Result<Vec<Client>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT a.id, a.username, a.email,
                  COALESCE((SELECT min(d.fqdn) FROM domains d WHERE d.tenant_id = t.id), '') AS "domain!",
                  COALESCE(p.display_name, p.name) AS "plan_name!",
                  COALESCE(u.total_bytes, 0) AS "disk_used_bytes!",
                  CASE WHEN a.status::text = 'active' AND t.status = 'active' THEN 'active'
                       ELSE 'suspended' END AS "status!",
                  extract(epoch FROM a.created_at)::bigint AS "created_at!"
           FROM tenants t
           JOIN accounts a ON a.id = t.owner_account_id
           JOIN plans p ON p.id = t.plan_id
           LEFT JOIN tenant_disk_usage u ON u.tenant_id = t.id
           WHERE t.reseller_id = $1 AND t.status <> 'deleting'
           ORDER BY a.created_at DESC, a.id DESC"#,
        reseller_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Client {
            id: r.id,
            username: r.username,
            email: r.email,
            domain: r.domain,
            plan_name: r.plan_name,
            disk_used_bytes: r.disk_used_bytes,
            status: r.status,
            created_at: r.created_at,
        })
        .collect())
}

/// Whether the account is a client of the reseller (and not being deleted).
///
/// # Errors
/// Database errors.
pub async fn owns_client(
    pool: &PgPool,
    reseller_id: Uuid,
    account_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM tenants
                           WHERE owner_account_id = $1 AND reseller_id = $2) AS "own!""#,
        account_id,
        reseller_id
    )
    .fetch_one(pool)
    .await
}

/// Accounts of the reseller's clients, for ending their sessions.
///
/// # Errors
/// Database errors.
pub async fn client_ids(pool: &PgPool, reseller_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT owner_account_id FROM tenants WHERE reseller_id = $1",
        reseller_id
    )
    .fetch_all(pool)
    .await
}

/// Sets which reseller a tenant belongs to.
///
/// # Errors
/// Database errors.
pub async fn assign_tenant(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    reseller_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE tenants SET reseller_id = $2 WHERE id = $1",
        tenant_id,
        reseller_id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes a reseller that has no clients and no plans in use, with its plans.
/// Returns `false` while it still has hosting accounts.
///
/// # Errors
/// Database errors.
pub async fn delete(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let has = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM tenants WHERE reseller_id = $1) AS "has!""#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;
    if has {
        return Ok(false);
    }
    sqlx::query!("DELETE FROM plans WHERE reseller_id = $1", id)
        .execute(&mut *conn)
        .await?;
    sqlx::query!("DELETE FROM resellers WHERE account_id = $1", id)
        .execute(conn)
        .await?;
    Ok(true)
}
