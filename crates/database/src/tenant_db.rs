//! Where a tenant's databases of each kind live (T07.13): the node's shared server or the
//! tenant's own. Plain queries, so this table needs no offline cache.

use sqlx::{PgPool, types::Uuid};

/// One tenant's server of one kind, as a candidate for a move.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// The tenant.
    pub tenant_id: Uuid,
    /// Its node.
    pub node_id: Uuid,
    /// Its system name.
    pub username: String,
    /// Its plan.
    pub plan_id: Uuid,
    /// `mysql` or `postgres`.
    pub engine: String,
    /// `shared`, `dedicated`, `moving_to_dedicated` or `moving_to_shared`.
    pub state: String,
    /// The move that is running.
    pub operation_id: Option<Uuid>,
    /// Seconds since the row last changed.
    pub age_secs: i64,
}

/// Every active tenant with each kind of server (no row is the shared server).
///
/// # Errors
/// Database errors.
pub async fn candidates(pool: &PgPool) -> Result<Vec<Candidate>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Uuid, String, String, Option<Uuid>, i64)>(
        "SELECT t.id, t.node_id, t.username, t.plan_id, e.engine,
                COALESCE(s.state, 'shared'), s.operation_id,
                COALESCE(extract(epoch FROM now() - s.updated_at)::bigint, 1000000000)
         FROM tenants t
         CROSS JOIN (VALUES ('mysql'), ('postgres')) AS e (engine)
         LEFT JOIN tenant_db_servers s ON s.tenant_id = t.id AND s.engine = e.engine
         WHERE t.status = 'active'
         ORDER BY t.id, e.engine",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(tenant_id, node_id, username, plan_id, engine, state, operation_id, age_secs)| {
                Candidate {
                    tenant_id,
                    node_id,
                    username,
                    plan_id,
                    engine,
                    state,
                    operation_id,
                    age_secs,
                }
            },
        )
        .collect())
}

/// Where the tenant's databases of this kind live now: `shared`, `dedicated`, or a move in
/// progress.
///
/// # Errors
/// Database errors.
pub async fn state(pool: &PgPool, tenant_id: Uuid, engine: &str) -> Result<String, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT state FROM tenant_db_servers WHERE tenant_id = $1 AND engine = $2",
    )
    .bind(tenant_id)
    .bind(engine)
    .fetch_optional(pool)
    .await?
    .unwrap_or_else(|| "shared".to_owned()))
}

/// Sets where the databases live (and which operation is moving them).
///
/// # Errors
/// Database errors.
pub async fn set_state(
    pool: &PgPool,
    tenant_id: Uuid,
    engine: &str,
    state: &str,
    operation_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tenant_db_servers (tenant_id, engine, state, operation_id) VALUES ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, engine) DO UPDATE
         SET state = EXCLUDED.state, operation_id = EXCLUDED.operation_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(engine)
    .bind(state)
    .bind(operation_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// What a move carries.
#[derive(Debug, Clone, Default)]
pub struct MoveSet {
    /// Ready databases.
    pub databases: Vec<String>,
    /// Ready users.
    pub users: Vec<String>,
    /// `(database, user, access)`.
    pub grants: Vec<(String, String, String)>,
    /// `(user, cidrs)`.
    pub remote: Vec<(String, Vec<String>)>,
}

/// The ready databases, users, grants and remote addresses of a tenant, of one kind.
///
/// # Errors
/// Database errors.
pub async fn move_set(
    pool: &PgPool,
    tenant_id: Uuid,
    engine: &str,
) -> Result<MoveSet, sqlx::Error> {
    let databases = sqlx::query_scalar::<_, String>(
        "SELECT name FROM databases WHERE tenant_id = $1 AND engine = $2 AND status = 'ready' ORDER BY name",
    )
    .bind(tenant_id)
    .bind(engine)
    .fetch_all(pool)
    .await?;
    let users = sqlx::query_scalar::<_, String>(
        "SELECT name FROM db_users WHERE tenant_id = $1 AND engine = $2 AND status = 'ready' ORDER BY name",
    )
    .bind(tenant_id)
    .bind(engine)
    .fetch_all(pool)
    .await?;
    let grants = sqlx::query_as::<_, (String, String, String)>(
        "SELECT d.name, u.name, g.access
         FROM db_grants g
         JOIN databases d ON d.id = g.database_id
         JOIN db_users u ON u.id = g.user_id
         WHERE g.tenant_id = $1 AND d.engine = $2 AND u.engine = $2
           AND d.status = 'ready' AND u.status = 'ready'
         ORDER BY d.name, u.name",
    )
    .bind(tenant_id)
    .bind(engine)
    .fetch_all(pool)
    .await?;
    let hosts = sqlx::query_as::<_, (String, String)>(
        "SELECT u.name, h.cidr FROM db_remote_hosts h JOIN db_users u ON u.id = h.user_id
         WHERE u.tenant_id = $1 AND u.engine = $2 AND u.status = 'ready' ORDER BY u.name, h.cidr",
    )
    .bind(tenant_id)
    .bind(engine)
    .fetch_all(pool)
    .await?;
    let mut remote: Vec<(String, Vec<String>)> = Vec::new();
    for (user, cidr) in hosts {
        match remote.last_mut() {
            Some((u, list)) if *u == user => list.push(cidr),
            _ => remote.push((user, vec![cidr])),
        }
    }
    Ok(MoveSet {
        databases,
        users,
        grants,
        remote,
    })
}

/// The system name to put in an operation when the tenant's databases of this kind are on its
/// own server (`None`: on the shared one).
///
/// # Errors
/// [`Moving`] while a move is running; database errors.
pub async fn dedicated_for(
    pool: &PgPool,
    tenant_id: Uuid,
    username: &str,
    engine: &str,
) -> Result<Result<Option<String>, Moving>, sqlx::Error> {
    Ok(match state(pool, tenant_id, engine).await?.as_str() {
        "dedicated" => Ok(Some(username.to_owned())),
        "shared" => Ok(None),
        _ => Err(Moving),
    })
}

/// The tenant's databases of this kind are being moved between servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the databases are being moved to another server")]
pub struct Moving;

/// Whether a plan lists the `db_dedicated` feature by name. A plan that has every feature
/// (`all_features`) does not get dedicated servers by that: they cost a container each, so a
/// plan has to ask for them.
///
/// # Errors
/// Database errors.
pub async fn plan_asks_for_dedicated(pool: &PgPool, plan_id: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT 'db_dedicated'::feature_key = ANY (f.features)
         FROM plans p JOIN feature_sets f ON f.id = p.feature_set_id WHERE p.id = $1",
    )
    .bind(plan_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or(false))
}
