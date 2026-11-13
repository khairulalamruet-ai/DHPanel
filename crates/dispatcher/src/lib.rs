//! The dispatcher (T02.11): moves operations from the ledger to nodes and back.
//!
//! Each pass ([`Dispatcher::tick`]):
//! 1. fails operations that expired before they were sent;
//! 2. times out operations that were sent but not answered (the node vanished,
//!    restarted or lost the frame): they are retried after a backoff while
//!    attempts and time remain;
//! 3. claims due operations of the nodes that are connected *right now* and
//!    writes them to the node's session. A frame that cannot be queued puts the
//!    operation back without using an attempt.
//!
//! Results arrive from the gateway ([`Dispatcher::handle_result`]). A result is
//! only accepted from the node the operation was sent to: a node cannot finish
//! another node's operation. Every status change is published on Redis
//! (`dh:ops` and `dh:ops:<op_id>`, a small JSON event without output) so that
//! the panel can update live; a failing publish never affects the ledger.

use std::time::Duration;

use dhpanel_cache::RedisHandle;
use dhpanel_node_gateway::{Gateway, NodeResult, SendError};
use dhpanel_node_protocol::{
    AgentError, ErrorCode, OperationResult, ResultStatus, encode_operation,
};
use dhpanel_operations::{Operations, Row, Status, envelope};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

/// Redis channel that carries every operation status change.
pub const EVENT_CHANNEL: &str = "dh:ops";

/// Timing and limits.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Time between passes.
    pub poll: Duration,
    /// Most operations claimed per pass.
    pub batch: i64,
    /// How long a sent operation may stay unanswered.
    pub operation_timeout: Duration,
    /// Wait before the first retry; doubles per attempt.
    pub retry_base: Duration,
    /// Longest wait between retries.
    pub retry_cap: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(250),
            batch: 50,
            operation_timeout: Duration::from_secs(60),
            retry_base: Duration::from_secs(2),
            retry_cap: Duration::from_secs(60),
        }
    }
}

impl Settings {
    /// Wait before delivery number `attempts + 1`, after `attempts` deliveries.
    #[must_use]
    pub fn backoff(&self, attempts: i32) -> Duration {
        let doublings = u32::try_from(attempts.saturating_sub(1).clamp(0, 16)).unwrap_or(0);
        self.retry_base
            .saturating_mul(1_u32 << doublings)
            .min(self.retry_cap)
    }
}

/// What one pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickStats {
    /// Operations written to a node.
    pub sent: usize,
    /// Operations put back because the node's queue was full or gone.
    pub released: usize,
    /// Operations that timed out waiting for an answer.
    pub timed_out: usize,
    /// Operations that expired unsent.
    pub expired: u64,
}

/// Sends operations and records results.
#[derive(Clone)]
pub struct Dispatcher {
    ops: Operations,
    gateway: Gateway,
    redis: Option<RedisHandle>,
    settings: Settings,
}

impl Dispatcher {
    /// A dispatcher. `redis` is optional: without it no events are published.
    #[must_use]
    pub fn new(
        ops: Operations,
        gateway: Gateway,
        redis: Option<RedisHandle>,
        settings: Settings,
    ) -> Self {
        Self {
            ops,
            gateway,
            redis,
            settings,
        }
    }

    /// Runs until `shutdown` turns `true`.
    pub async fn run(
        self,
        mut results: mpsc::Receiver<NodeResult>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(self.settings.poll);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = stopped(&mut shutdown) => return,
                _ = tick.tick() => { self.tick().await; }
                Some(report) = results.recv() => self.handle_result(report).await,
            }
        }
    }

    /// One pass; see the module documentation.
    pub async fn tick(&self) -> TickStats {
        let mut stats = TickStats::default();
        match self.ops.expire_overdue().await {
            Ok(n) => stats.expired = n,
            Err(err) => tracing::warn!(error = %err, "could not expire operations"),
        }
        let retry_in = self.settings.backoff(1);
        match self
            .ops
            .time_out_dispatched(self.settings.operation_timeout, retry_in)
            .await
        {
            Ok(timed_out) => {
                stats.timed_out = timed_out.len();
                for (id, status) in timed_out {
                    tracing::warn!(op_id = %id, ?status, "operation timed out waiting for the node");
                    self.publish(id, None, status).await;
                }
            }
            Err(err) => tracing::warn!(error = %err, "could not time out operations"),
        }

        let nodes = self.gateway.connected_nodes();
        if nodes.is_empty() {
            return stats;
        }
        let due = match self.ops.claim_due(&nodes, self.settings.batch).await {
            Ok(due) => due,
            Err(err) => {
                tracing::warn!(error = %err, "could not claim operations");
                return stats;
            }
        };
        let now = unix_now();
        for op in due {
            match self.deliver(&op, now).await {
                Delivery::Sent => stats.sent += 1,
                Delivery::Released => stats.released += 1,
                Delivery::Rejected => {}
            }
        }
        stats
    }

    async fn deliver(&self, op: &Row, now: i64) -> Delivery {
        let frame = match envelope(op, now)
            .map_err(|e| e.to_string())
            .and_then(|env| encode_operation(&env).map_err(|e| e.to_string()))
        {
            Ok(frame) => frame,
            Err(reason) => {
                // A ledger row this build cannot express (for example written by a newer
                // control plane): trying again cannot help.
                tracing::error!(op_id = %op.id, %reason, "operation cannot be encoded");
                let error = serde_json::json!({
                    "code": "internal", "message": "the operation could not be encoded", "retryable": false
                });
                self.finish_failed(op.id, op.node_id, &error, None).await;
                return Delivery::Rejected;
            }
        };
        match self.gateway.send(op.node_id, frame) {
            Ok(()) => {
                self.publish(op.id, Some(op.node_id), Status::Dispatched)
                    .await;
                Delivery::Sent
            }
            Err(err @ (SendError::NotConnected | SendError::Busy)) => {
                tracing::debug!(op_id = %op.id, %err, "operation put back");
                if let Err(db) = self.ops.release_undelivered(op.id).await {
                    tracing::warn!(op_id = %op.id, error = %db, "could not put the operation back");
                }
                Delivery::Released
            }
        }
    }

    /// Records a result reported by `report.node_id`.
    pub async fn handle_result(&self, report: NodeResult) {
        let NodeResult { node_id, result } = report;
        let OperationResult {
            op_id,
            status,
            output,
            error,
        } = result;
        let row = match self.ops.get(op_id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                tracing::warn!(%node_id, %op_id, "result for an unknown operation ignored");
                return;
            }
            Err(err) => {
                tracing::warn!(%node_id, %op_id, error = %err, "could not look up an operation");
                return;
            }
        };
        if row.node_id != node_id {
            tracing::warn!(%node_id, %op_id, owner = %row.node_id, "result from the wrong node ignored");
            return;
        }
        let error_json = || {
            let error = error.clone().unwrap_or_else(|| {
                AgentError::new(
                    ErrorCode::Internal,
                    "the node reported a failure without detail",
                )
            });
            serde_json::to_value(error).unwrap_or(serde_json::Value::Null)
        };
        match status {
            ResultStatus::Succeeded => {
                let output = output
                    .and_then(|o| serde_json::to_value(o).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                match self.ops.succeed(op_id, &output).await {
                    Ok(true) => self.publish(op_id, Some(node_id), Status::Succeeded).await,
                    Ok(false) => tracing::debug!(%op_id, "late or repeated result ignored"),
                    Err(err) => tracing::warn!(%op_id, error = %err, "could not record a result"),
                }
            }
            ResultStatus::Failed => {
                self.finish_failed(op_id, node_id, &error_json(), None)
                    .await
            }
            ResultStatus::Retryable => {
                let wait = self.settings.backoff(row.attempts);
                self.finish_failed(op_id, node_id, &error_json(), Some(wait))
                    .await;
            }
        }
    }

    async fn finish_failed(
        &self,
        id: Uuid,
        node_id: Uuid,
        error: &serde_json::Value,
        retry_in: Option<Duration>,
    ) {
        match self.ops.fail(id, error, retry_in).await {
            Ok(Some(status)) => self.publish(id, Some(node_id), status).await,
            Ok(None) => tracing::debug!(op_id = %id, "late or repeated failure ignored"),
            Err(err) => tracing::warn!(op_id = %id, error = %err, "could not record a failure"),
        }
    }

    async fn publish(&self, id: Uuid, node_id: Option<Uuid>, status: Status) {
        let Some(redis) = &self.redis else { return };
        let event = serde_json::json!({
            "op_id": id,
            "node_id": node_id,
            "status": status.as_str(),
        })
        .to_string();
        for channel in [EVENT_CHANNEL.to_owned(), format!("{EVENT_CHANNEL}:{id}")] {
            if let Err(err) = redis.publish(&channel, &event).await {
                tracing::debug!(error = %err, "could not publish an operation event");
                return;
            }
        }
    }
}

async fn stopped(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow_and_update() {
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}

enum Delivery {
    Sent,
    Released,
    Rejected,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn retry_waits_double_up_to_the_cap() {
        let s = Settings::default();
        let waits: Vec<u64> = (0..8).map(|a| s.backoff(a).as_secs()).collect();
        assert_eq!(waits, [2, 2, 4, 8, 16, 32, 60, 60]);
        assert_eq!(s.backoff(i32::MAX), s.retry_cap);
        assert_eq!(s.backoff(-5), s.retry_base);
    }
}
