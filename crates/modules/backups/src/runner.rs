//! The backup runner (T09.07): makes the backups that schedules ask for, keeps no more of
//! them than retention allows, and has the node remove the rest.
//!
//! It is a loop of [`tick`]s and keeps no state of its own: what is due, queued or being
//! removed is read from the database every time, every operation it queues is idempotent
//! (a key made of the backup's id), and the node's answers reach the `backups` rows through
//! the ledger (see the migration of T09.07). A crash between two steps is repaired by the
//! next tick.

use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_backup::{
    crypto::{key_hex, new_key},
    schedule::{Cron, expired},
};
use dhpanel_database::{
    backups as store,
    operations::{self, NewOperation},
    sqlx::PgPool,
    tenants,
};
use dhpanel_node_protocol::{BackupTarget, Operation};
use uuid::Uuid;

/// Most backups of one node that run at once.
pub const PER_NODE: i64 = 2;
/// How long a queued backup waits for its node before it is given up on (seconds).
const BACKUP_TTL_SECS: i32 = 24 * 3600;
const DELETE_TTL_SECS: i32 = 6 * 3600;
/// Most things done for one kind of work in one tick.
const BATCH: i64 = 50;

/// What went wrong in a tick (the loop logs it and goes on).
#[derive(Debug, thiserror::Error)]
pub enum TickError {
    /// The database.
    #[error(transparent)]
    Database(#[from] dhpanel_database::sqlx::Error),
    /// The ledger refused an operation.
    #[error("cannot queue an operation: {0}")]
    Queue(String),
    /// A key could not be made or sealed.
    #[error("cannot make or seal a key")]
    Key,
}

fn target(d: &store::Destination) -> BackupTarget {
    BackupTarget {
        kind: d.kind.clone(),
        settings: d.settings.clone(),
    }
}

async fn queue(
    db: &PgPool,
    node_id: Uuid,
    tenant_id: Option<Uuid>,
    op: &Operation,
    key: &str,
    ttl_secs: i32,
) -> Result<(), TickError> {
    let payload = serde_json::to_value(op).map_err(|e| TickError::Queue(e.to_string()))?;
    operations::enqueue(
        db,
        &NewOperation {
            node_id,
            kind: op.type_name(),
            tenant_id,
            payload: &payload,
            idempotency_key: key,
            created_by: None,
            ttl_secs,
            max_attempts: 3,
        },
    )
    .await
    .map_err(|e| TickError::Queue(e.to_string()))?;
    Ok(())
}

/// One round of work, `now` in Unix seconds.
///
/// # Errors
/// [`TickError`]; what was done before it stays done.
pub async fn tick(db: &PgPool, master_key: &MasterKey, now: i64) -> Result<(), TickError> {
    start_due(db, master_key, now).await?;
    send_queued(db).await?;
    apply_retention(db).await?;
    send_removals(db).await?;
    Ok(())
}

/// Schedules whose time has come get a backup (queued), unless the node is busy with
/// [`PER_NODE`] already: then the schedule stays due and is looked at again next time.
async fn start_due(db: &PgPool, master_key: &MasterKey, now: i64) -> Result<(), TickError> {
    for schedule in store::due_schedules(db, BATCH).await? {
        let next = Cron::parse(&schedule.cron)
            .ok()
            .and_then(|c| c.next_after(now));
        let Some(tenant_id) = schedule.tenant_id else {
            // The panel's own backup is made elsewhere (T09.08).
            store::advance_schedule(db, schedule.id, next).await?;
            continue;
        };
        let Some(tenant) = tenants::get(db, tenant_id).await? else {
            store::advance_schedule(db, schedule.id, next).await?;
            continue;
        };
        if store::active_on_node(db, tenant.node_id).await? >= PER_NODE {
            continue;
        }
        let destination = match schedule.destination_id {
            Some(d) => Some(d),
            None => store::default_destination(db).await?,
        };
        if let Some(destination) = destination
            && tenant.status == dhpanel_domain::TenantStatus::Active
        {
            let id = store::new_backup(
                db,
                tenant.id,
                &tenant.username,
                "schedule",
                destination,
                None,
            )
            .await?;
            let key = new_key().map_err(|_| TickError::Key)?;
            let sealed = master_key
                .seal(&store::key_context(id), &key)
                .map_err(|_| TickError::Key)?;
            store::keep_key(db, id, &sealed).await?;
        }
        store::advance_schedule(db, schedule.id, next).await?;
    }
    Ok(())
}

/// Queued backups get their operation (the same one again is the same row).
async fn send_queued(db: &PgPool) -> Result<(), TickError> {
    for backup in store::queued(db, BATCH).await? {
        let (Some(tenant_id), Some(destination)) = (
            backup.tenant_id,
            store::destination(db, backup.destination_id).await?,
        ) else {
            continue;
        };
        let Some(tenant) = tenants::get(db, tenant_id).await? else {
            continue;
        };
        let databases: Vec<String> = dhpanel_database::databases::list(db, tenant.id)
            .await?
            .into_iter()
            .filter(|d| d.status == "ready")
            .map(|d| d.name)
            .collect();
        let op = Operation::CreateBackup {
            tenant_id: tenant.id,
            uid: u32::try_from(tenant.uid).unwrap_or(0),
            backup_id: backup.id,
            account: tenant.username.clone(),
            target: target(&destination),
            credentials: None,
            key: None,
            databases,
            // TODO(T09.07): the zone records of the account's domains.
            dns: serde_json::json!([]),
            meta: serde_json::json!({
                "username": tenant.username,
                "plan_id": tenant.plan_id,
                "tenant_created_at": tenant.created_at,
            }),
            created_at: backup.created_at,
        };
        queue(
            db,
            tenant.node_id,
            Some(tenant.id),
            &op,
            &format!("backup:{}", backup.id),
            BACKUP_TTL_SECS,
        )
        .await?;
    }
    Ok(())
}

/// What retention lets go is marked, and the removal follows.
async fn apply_retention(db: &PgPool) -> Result<(), TickError> {
    for rule in store::retentions(db).await? {
        let ready = store::ready_scheduled(db, rule.tenant_id).await?;
        let gone = expired(
            &ready,
            u32::try_from(rule.keep_daily).unwrap_or(0),
            u32::try_from(rule.keep_weekly).unwrap_or(0),
        );
        for id in gone {
            store::mark_deleting(db, id).await?;
        }
    }
    Ok(())
}

/// Backups that are being removed get their operation.
async fn send_removals(db: &PgPool) -> Result<(), TickError> {
    for backup in store::deleting(db, BATCH).await? {
        let Some(destination) = store::destination(db, backup.destination_id).await? else {
            continue;
        };
        let node_id = match backup.tenant_id {
            Some(t) => tenants::get(db, t).await?.map(|t| t.node_id),
            None => None,
        };
        let Some(node_id) = node_id else {
            continue;
        };
        let op = Operation::DeleteBackup {
            backup_id: backup.id,
            target: target(&destination),
            credentials: None,
        };
        // The stamp makes a new key when the removal is tried again after a failure.
        queue(
            db,
            node_id,
            backup.tenant_id,
            &op,
            &format!("backup-delete:{}:{}", backup.id, backup.stamp),
            DELETE_TTL_SECS,
        )
        .await?;
    }
    Ok(())
}

/// The node-bound operations of backups, with their secrets put in just before sending:
/// the key of the backup and the credentials of its destination.
///
/// # Errors
/// A message for the ledger when something does not open.
pub async fn fill(db: &PgPool, master_key: &MasterKey, op: Operation) -> Result<Operation, String> {
    let open_credentials = |destination: Uuid| async move {
        let sealed = store::sealed_credentials(db, destination)
            .await
            .map_err(|_| "could not read the destination's credentials".to_owned())?;
        sealed
            .map(|s| {
                master_key
                    .open(&store::credentials_context(destination), &s)
                    .map_err(|_| "the destination's credentials do not open".to_owned())
                    .and_then(|b| {
                        String::from_utf8(b)
                            .map_err(|_| "the destination's credentials are not text".to_owned())
                    })
                    .map(dhpanel_node_protocol::DbPassword)
            })
            .transpose()
    };
    let open_key = |backup: Uuid| async move {
        let sealed = store::sealed_key_of(db, backup)
            .await
            .map_err(|_| "could not read a backup key".to_owned())?
            .ok_or_else(|| "this backup has no key".to_owned())?;
        let bytes = master_key
            .open(&store::key_context(backup), &sealed)
            .map_err(|_| "a backup key does not open".to_owned())?;
        let key: [u8; dhpanel_backup::KEY_LEN] = bytes
            .try_into()
            .map_err(|_| "a backup key is not the right size".to_owned())?;
        Ok::<_, String>(dhpanel_node_protocol::DbPassword(key_hex(&key)))
    };
    let destination_of = |backup: Uuid| async move {
        store::destination_of(db, backup)
            .await
            .map_err(|_| "could not read a backup".to_owned())?
            .ok_or_else(|| "this backup is gone".to_owned())
    };
    match op {
        Operation::CreateBackup {
            tenant_id,
            uid,
            backup_id,
            account,
            target,
            key: None,
            databases,
            dns,
            meta,
            created_at,
            ..
        } => Ok(Operation::CreateBackup {
            tenant_id,
            uid,
            backup_id,
            account,
            target,
            credentials: open_credentials(destination_of(backup_id).await?).await?,
            key: Some(open_key(backup_id).await?),
            databases,
            dns,
            meta,
            created_at,
        }),
        Operation::DeleteBackup {
            backup_id, target, ..
        } => Ok(Operation::DeleteBackup {
            backup_id,
            target,
            credentials: open_credentials(destination_of(backup_id).await?).await?,
        }),
        other => Ok(other),
    }
}
