//! Usage over time (T10.09): what the nodes' `collect_metrics` readings become.
//!
//! [`record`] turns one reading (counters) into rows of rates and levels per account and for
//! the node. [`maintain`] rolls minutes up into hours, makes the months of the hour table,
//! and drops what is too old. [`series`] reads an account's usage back as buckets.

use std::collections::HashMap;

use sqlx::{PgPool, types::Uuid};

/// One container's counters and levels, as read.
#[derive(Debug, Clone)]
pub struct Container {
    /// `dh-<account>-<role>`.
    pub name: String,
    /// Memory in use.
    pub memory_bytes: u64,
    /// CPU time since it started, in nanoseconds.
    pub cpu_nanos: u64,
    /// Processes.
    pub pids: u64,
    /// Network bytes received since it started.
    pub net_rx: u64,
    /// Network bytes sent.
    pub net_tx: u64,
    /// Block bytes read.
    pub io_read: u64,
    /// Block bytes written.
    pub io_write: u64,
}

/// The node's counters and levels, as read.
#[derive(Debug, Clone, Copy)]
pub struct Node {
    /// Logical CPUs.
    pub cpus: u32,
    /// Jiffies the CPUs were busy since boot.
    pub busy_jiffies: u64,
    /// Jiffies since boot.
    pub total_jiffies: u64,
    /// Memory in use.
    pub memory_used: u64,
    /// Memory in all.
    pub memory_total: u64,
    /// Swap in use.
    pub swap_used: u64,
    /// One-minute load times 100.
    pub load1_x100: u32,
    /// Disk bytes in use.
    pub disk_used: u64,
    /// Disk bytes in all.
    pub disk_total: u64,
}

fn big(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// The account a container belongs to: the longest account name `dh-<name>-` begins with.
fn owner<'a>(name: &str, accounts: &'a [(String, Uuid)]) -> Option<&'a Uuid> {
    accounts
        .iter()
        .filter(|(user, _)| {
            name.strip_prefix("dh-").is_some_and(|r| {
                r.strip_prefix(user.as_str())
                    .is_some_and(|rest| rest.starts_with('-'))
            })
        })
        .max_by_key(|(user, _)| user.len())
        .map(|(_, id)| id)
}

#[derive(Default, Clone, Copy)]
struct Sum {
    cpu_millis: i64,
    memory: i64,
    pids: i64,
    rx: i64,
    tx: i64,
    read: i64,
    write: i64,
}

/// Stores one reading of a node taken at `at` (Unix seconds).
///
/// # Errors
/// Database errors (nothing is stored then).
pub async fn record(
    pool: &PgPool,
    node_id: Uuid,
    at: i64,
    node: &Node,
    containers: &[Container],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let accounts: Vec<(String, Uuid)> = sqlx::query!(
        "SELECT username, id FROM tenants WHERE node_id = $1",
        node_id
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| (r.username, r.id))
    .collect();
    let previous: HashMap<String, (i64, i64, i64, i64, i64, i64)> = sqlx::query!(
        r#"SELECT container, extract(epoch FROM at)::bigint AS "at!", cpu_nanos, net_rx, net_tx,
                  io_read, io_write
           FROM usage_counters WHERE node_id = $1"#,
        node_id
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| {
        (
            r.container,
            (r.at, r.cpu_nanos, r.net_rx, r.net_tx, r.io_read, r.io_write),
        )
    })
    .collect();
    let mut per_account: HashMap<Uuid, Sum> = HashMap::new();
    for c in containers {
        let Some(account) = owner(&c.name, &accounts) else {
            continue;
        };
        let sum = per_account.entry(*account).or_default();
        sum.memory += big(c.memory_bytes);
        sum.pids += big(c.pids);
        // A rate needs an earlier reading of the same run: counters that went down mean the
        // container restarted, and that reading is only a new beginning.
        if let Some(&(was_at, cpu, rx, tx_, read, write)) = previous.get(&c.name)
            && at > was_at
            && big(c.cpu_nanos) >= cpu
        {
            let seconds = at - was_at;
            sum.cpu_millis += (big(c.cpu_nanos) - cpu) / 1_000_000 * 1000 / (seconds * 1000).max(1);
            sum.rx += (big(c.net_rx) - rx).max(0);
            sum.tx += (big(c.net_tx) - tx_).max(0);
            sum.read += (big(c.io_read) - read).max(0);
            sum.write += (big(c.io_write) - write).max(0);
        }
    }
    for (account, s) in &per_account {
        sqlx::query!(
            "INSERT INTO usage_minute
                 (tenant_id, at, cpu_millis, memory_bytes, pids, net_rx_bytes, net_tx_bytes,
                  io_read_bytes, io_write_bytes)
             VALUES ($1, date_trunc('minute', to_timestamp($2::bigint)), $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (tenant_id, at) DO UPDATE
             SET cpu_millis = EXCLUDED.cpu_millis, memory_bytes = EXCLUDED.memory_bytes,
                 pids = EXCLUDED.pids, net_rx_bytes = usage_minute.net_rx_bytes + EXCLUDED.net_rx_bytes,
                 net_tx_bytes = usage_minute.net_tx_bytes + EXCLUDED.net_tx_bytes,
                 io_read_bytes = usage_minute.io_read_bytes + EXCLUDED.io_read_bytes,
                 io_write_bytes = usage_minute.io_write_bytes + EXCLUDED.io_write_bytes",
            account,
            at,
            i32::try_from(s.cpu_millis).unwrap_or(i32::MAX),
            s.memory,
            i32::try_from(s.pids).unwrap_or(i32::MAX),
            s.rx,
            s.tx,
            s.read,
            s.write,
        )
        .execute(&mut *tx)
        .await?;
    }
    // The counters for the next difference: these containers, and none that is gone.
    sqlx::query!("DELETE FROM usage_counters WHERE node_id = $1", node_id)
        .execute(&mut *tx)
        .await?;
    for c in containers {
        sqlx::query!(
            "INSERT INTO usage_counters (node_id, container, at, cpu_nanos, net_rx, net_tx, io_read, io_write)
             VALUES ($1, $2, to_timestamp($3::bigint), $4, $5, $6, $7, $8)",
            node_id,
            c.name,
            at,
            big(c.cpu_nanos),
            big(c.net_rx),
            big(c.net_tx),
            big(c.io_read),
            big(c.io_write),
        )
        .execute(&mut *tx)
        .await?;
    }
    // The node: how much of its CPU was busy since the last reading.
    let before = sqlx::query!(
        "SELECT cpu_busy_jiffies, cpu_total_jiffies FROM node_usage_minute
         WHERE node_id = $1 ORDER BY at DESC LIMIT 1",
        node_id
    )
    .fetch_optional(&mut *tx)
    .await?;
    let busy_millis = before
        .filter(|b| big(node.total_jiffies) > b.cpu_total_jiffies)
        .map_or(0, |b| {
            let busy = (big(node.busy_jiffies) - b.cpu_busy_jiffies).max(0);
            let total = big(node.total_jiffies) - b.cpu_total_jiffies;
            (busy * 1000 / total).clamp(0, 1000)
        });
    sqlx::query!(
        "INSERT INTO node_usage_minute
             (node_id, at, cpus, cpu_busy_millis, memory_used_bytes, memory_total_bytes,
              swap_used_bytes, load1_x100, disk_used_bytes, disk_total_bytes, containers,
              cpu_busy_jiffies, cpu_total_jiffies)
         VALUES ($1, date_trunc('minute', to_timestamp($2::bigint)), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (node_id, at) DO UPDATE
         SET cpus = EXCLUDED.cpus, cpu_busy_millis = EXCLUDED.cpu_busy_millis,
             memory_used_bytes = EXCLUDED.memory_used_bytes, memory_total_bytes = EXCLUDED.memory_total_bytes,
             swap_used_bytes = EXCLUDED.swap_used_bytes, load1_x100 = EXCLUDED.load1_x100,
             disk_used_bytes = EXCLUDED.disk_used_bytes, disk_total_bytes = EXCLUDED.disk_total_bytes,
             containers = EXCLUDED.containers, cpu_busy_jiffies = EXCLUDED.cpu_busy_jiffies,
             cpu_total_jiffies = EXCLUDED.cpu_total_jiffies",
        node_id,
        at,
        i32::try_from(node.cpus).unwrap_or(i32::MAX),
        i32::try_from(busy_millis).unwrap_or(0),
        big(node.memory_used),
        big(node.memory_total),
        big(node.swap_used),
        i32::try_from(node.load1_x100).unwrap_or(i32::MAX),
        big(node.disk_used),
        big(node.disk_total),
        i32::try_from(containers.len()).unwrap_or(i32::MAX),
        big(node.busy_jiffies),
        big(node.total_jiffies),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Minutes are kept this many hours.
pub const KEEP_MINUTES_HOURS: i32 = 48;
/// The node's minutes are kept this many days.
pub const KEEP_NODE_DAYS: i32 = 30;
/// Hours are kept this many days (whole months older than this are dropped).
pub const KEEP_HOURS_DAYS: i32 = 400;

/// What [`maintain`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Maintained {
    /// Hour rows made or refreshed from minutes.
    pub rolled_up: u64,
    /// Minute rows removed.
    pub minutes_removed: u64,
    /// Node minute rows removed.
    pub node_rows_removed: u64,
    /// Month partitions made.
    pub partitions_made: u32,
    /// Month partitions dropped.
    pub partitions_dropped: u32,
}

/// Rolls up the hours that are over, removes what is too old, and keeps the month partitions
/// of the hour table: this month, the next two, and none older than [`KEEP_HOURS_DAYS`] days.
///
/// # Errors
/// Database errors.
pub async fn maintain(pool: &PgPool) -> Result<Maintained, sqlx::Error> {
    let mut done = Maintained::default();
    // Partitions first, so that the roll-up has a place to go.
    for ahead in 0..3 {
        let name: String = sqlx::query_scalar!(
            r#"SELECT 'usage_hour_' || to_char(date_trunc('month', now()) + make_interval(months => $1), 'YYYY_MM') AS "n!""#,
            ahead
        )
        .fetch_one(pool)
        .await?;
        let exists: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM pg_class WHERE relname = $1) AS "e!""#,
            name
        )
        .fetch_one(pool)
        .await?;
        if !exists {
            // The name is made from digits above; the bounds are dates the database made.
            let bounds = sqlx::query!(
                r#"SELECT to_char(date_trunc('month', now()) + make_interval(months => $1), 'YYYY-MM-DD') AS "from!",
                          to_char(date_trunc('month', now()) + make_interval(months => $1 + 1), 'YYYY-MM-DD') AS "to!""#,
                ahead
            )
            .fetch_one(pool)
            .await?;
            // Audited: the name is `usage_hour_` and digits, the bounds are dates, both made by
            // the database above.
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TABLE IF NOT EXISTS {name} PARTITION OF usage_hour FOR VALUES FROM ('{}') TO ('{}')",
                bounds.from, bounds.to
            )))
            .execute(pool)
            .await?;
            done.partitions_made += 1;
        }
    }
    done.rolled_up = sqlx::query!(
        "INSERT INTO usage_hour
             (tenant_id, at, cpu_millis, memory_bytes, pids, net_rx_bytes, net_tx_bytes,
              io_read_bytes, io_write_bytes)
         SELECT tenant_id, date_trunc('hour', at),
                avg(cpu_millis)::int, avg(memory_bytes)::bigint, max(pids),
                sum(net_rx_bytes), sum(net_tx_bytes), sum(io_read_bytes), sum(io_write_bytes)
         FROM usage_minute
         WHERE at < date_trunc('hour', now())
         GROUP BY tenant_id, date_trunc('hour', at)
         ON CONFLICT (tenant_id, at) DO UPDATE
         SET cpu_millis = EXCLUDED.cpu_millis, memory_bytes = EXCLUDED.memory_bytes,
             pids = EXCLUDED.pids, net_rx_bytes = EXCLUDED.net_rx_bytes,
             net_tx_bytes = EXCLUDED.net_tx_bytes, io_read_bytes = EXCLUDED.io_read_bytes,
             io_write_bytes = EXCLUDED.io_write_bytes"
    )
    .execute(pool)
    .await?
    .rows_affected();
    // Minutes are removed only once their hour is in the hour table (it is, just above).
    done.minutes_removed = sqlx::query!(
        "DELETE FROM usage_minute
         WHERE at < now() - make_interval(hours => $1) AND at < date_trunc('hour', now())",
        KEEP_MINUTES_HOURS
    )
    .execute(pool)
    .await?
    .rows_affected();
    done.node_rows_removed = sqlx::query!(
        "DELETE FROM node_usage_minute WHERE at < now() - make_interval(days => $1)",
        KEEP_NODE_DAYS
    )
    .execute(pool)
    .await?
    .rows_affected();
    // Months that end before the oldest hour kept are dropped whole.
    let old: Vec<String> = sqlx::query_scalar!(
        r#"SELECT c.relname AS "name!"
           FROM pg_inherits i
           JOIN pg_class c ON c.oid = i.inhrelid
           JOIN pg_class p ON p.oid = i.inhparent
           WHERE p.relname = 'usage_hour' AND c.relname ~ '^usage_hour_[0-9]{4}_[0-9]{2}$'
             AND to_date(substr(c.relname, 12), 'YYYY_MM') + interval '1 month'
                 <= now() - make_interval(days => $1)"#,
        KEEP_HOURS_DAYS
    )
    .fetch_all(pool)
    .await?;
    for name in old {
        // Audited: the name matched `^usage_hour_[0-9]{4}_[0-9]{2}$` in the query above.
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {name}")))
            .execute(pool)
            .await?;
        done.partitions_dropped += 1;
    }
    Ok(done)
}

/// One bucket of a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    /// Start of the bucket (Unix seconds).
    pub at: i64,
    /// Average thousandths of a CPU.
    pub cpu_millis: i32,
    /// Average memory in use.
    pub memory_bytes: i64,
    /// Most processes seen.
    pub pids: i32,
    /// Bytes received in the bucket.
    pub net_rx_bytes: i64,
    /// Bytes sent in the bucket.
    pub net_tx_bytes: i64,
    /// Bytes read from disk in the bucket.
    pub io_read_bytes: i64,
    /// Bytes written to disk in the bucket.
    pub io_write_bytes: i64,
}

/// An account's usage from `from` to `to` (Unix seconds) in buckets of `bucket_seconds` (at
/// least a minute): from the minutes when the whole range is within [`KEEP_MINUTES_HOURS`],
/// from the hours otherwise.
///
/// # Errors
/// Database errors.
pub async fn series(
    pool: &PgPool,
    tenant_id: Uuid,
    from: i64,
    to: i64,
    bucket_seconds: i64,
) -> Result<Vec<Point>, sqlx::Error> {
    let bucket = bucket_seconds.max(60);
    let recent = to - from <= i64::from(KEEP_MINUTES_HOURS) * 3600;
    let rows = if recent {
        sqlx::query!(
            r#"SELECT extract(epoch FROM date_bin(make_interval(secs => $4::int), at, 'epoch'))::bigint AS "at!",
                      avg(cpu_millis)::int AS "cpu_millis!", avg(memory_bytes)::bigint AS "memory_bytes!",
                      max(pids) AS "pids!", sum(net_rx_bytes)::bigint AS "rx!", sum(net_tx_bytes)::bigint AS "tx!",
                      sum(io_read_bytes)::bigint AS "read!", sum(io_write_bytes)::bigint AS "write!"
               FROM usage_minute
               WHERE tenant_id = $1 AND at >= to_timestamp($2::bigint) AND at < to_timestamp($3::bigint)
               GROUP BY 1 ORDER BY 1"#,
            tenant_id,
            from,
            to,
            i32::try_from(bucket).unwrap_or(3600),
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.at, r.cpu_millis, r.memory_bytes, r.pids, r.rx, r.tx, r.read, r.write))
        .collect::<Vec<_>>()
    } else {
        sqlx::query!(
            r#"SELECT extract(epoch FROM date_bin(make_interval(secs => $4::int), at, 'epoch'))::bigint AS "at!",
                      avg(cpu_millis)::int AS "cpu_millis!", avg(memory_bytes)::bigint AS "memory_bytes!",
                      max(pids) AS "pids!", sum(net_rx_bytes)::bigint AS "rx!", sum(net_tx_bytes)::bigint AS "tx!",
                      sum(io_read_bytes)::bigint AS "read!", sum(io_write_bytes)::bigint AS "write!"
               FROM usage_hour
               WHERE tenant_id = $1 AND at >= to_timestamp($2::bigint) AND at < to_timestamp($3::bigint)
               GROUP BY 1 ORDER BY 1"#,
            tenant_id,
            from,
            to,
            i32::try_from(bucket.max(3600)).unwrap_or(3600),
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.at, r.cpu_millis, r.memory_bytes, r.pids, r.rx, r.tx, r.read, r.write))
        .collect::<Vec<_>>()
    };
    Ok(rows
        .into_iter()
        .map(
            |(at, cpu_millis, memory_bytes, pids, rx, tx, read, write)| Point {
                at,
                cpu_millis,
                memory_bytes,
                pids,
                net_rx_bytes: rx,
                net_tx_bytes: tx,
                io_read_bytes: read,
                io_write_bytes: write,
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_belongs_to_the_longest_account_name_in_front_of_its_role() {
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let accounts = vec![("alice".to_owned(), a), ("alice-shop".to_owned(), b)];
        assert_eq!(owner("dh-alice-web", &accounts), Some(&a));
        assert_eq!(owner("dh-alice-php-8-3", &accounts), Some(&a));
        assert_eq!(owner("dh-alice-shop-php", &accounts), Some(&b));
        assert_eq!(owner("dh-alicex-web", &accounts), None);
        assert_eq!(owner("dh-db", &accounts), None);
        assert_eq!(owner("alice-web", &accounts), None);
    }
}
