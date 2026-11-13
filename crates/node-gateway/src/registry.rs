//! Which node has a live session in this process.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

/// "Close this session now", with the reason.
#[derive(Default)]
pub(crate) struct Stop {
    notify: Notify,
    revoked: AtomicBool,
}

impl Stop {
    /// Waits for the signal.
    pub(crate) async fn requested(&self) {
        self.notify.notified().await;
    }

    /// Whether the node was revoked (otherwise the session was replaced).
    pub(crate) fn was_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }
}

struct Entry {
    session: u64,
    stop: Arc<Stop>,
    outbound: mpsc::Sender<String>,
}

/// Frames waiting to be written to one session.
pub(crate) const OUTBOUND_QUEUE: usize = 64;

/// Why a frame could not be handed to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    /// The node has no live session on this control plane.
    #[error("the node is not connected")]
    NotConnected,
    /// The session's queue is full; try again shortly.
    #[error("the node's connection is busy")]
    Busy,
}

/// One session per node. A newer connection replaces the older one, because the
/// older one is almost always a half-dead connection the agent has given up on.
#[derive(Default)]
pub(crate) struct Registry {
    sessions: Mutex<HashMap<Uuid, Entry>>,
    next: AtomicU64,
}

impl Registry {
    /// Registers a session and tells the previous one (if any) to close.
    /// Returns the session number and the signal that says "you were replaced".
    pub(crate) fn register(&self, node: Uuid) -> (u64, Arc<Stop>, mpsc::Receiver<String>) {
        let (outbound, queue) = mpsc::channel(OUTBOUND_QUEUE);
        let session = self.next.fetch_add(1, Ordering::Relaxed);
        let stop = Arc::new(Stop::default());
        let previous = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                node,
                Entry {
                    session,
                    stop: stop.clone(),
                    outbound,
                },
            );
        if let Some(previous) = previous {
            previous.stop.notify.notify_one();
        }
        (session, stop, queue)
    }

    /// Queues a text frame for the node's live session.
    pub(crate) fn send(&self, node: Uuid, frame: String) -> Result<(), SendError> {
        let sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = sessions.get(&node).ok_or(SendError::NotConnected)?;
        entry.outbound.try_send(frame).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => SendError::Busy,
            mpsc::error::TrySendError::Closed(_) => SendError::NotConnected,
        })
    }

    /// Nodes with a live session.
    pub(crate) fn connected(&self) -> Vec<Uuid> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .copied()
            .collect()
    }

    /// Tells the node's live session, if any, to close because the node was
    /// revoked. Returns whether there was one.
    pub(crate) fn revoke(&self, node: Uuid) -> bool {
        let sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        sessions.get(&node).is_some_and(|entry| {
            entry.stop.revoked.store(true, Ordering::Release);
            entry.stop.notify.notify_one();
            true
        })
    }

    /// Removes the session if it is still the current one. `true` means the
    /// node has no session left, so the caller should mark it offline.
    pub(crate) fn release(&self, node: Uuid, session: u64) -> bool {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        if sessions.get(&node).is_some_and(|e| e.session == session) {
            sessions.remove(&node);
            true
        } else {
            false
        }
    }

    pub(crate) fn contains(&self, node: Uuid) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&node)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_new_session_replaces_the_old_one_and_only_the_current_one_releases() {
        let registry = Registry::default();
        let node = Uuid::now_v7();
        let (first, first_signal, _first_queue) = registry.register(node);
        let (second, _, mut second_queue) = registry.register(node);
        assert_ne!(first, second);
        // The first session was told to close (the permit is stored, so this returns at once).
        tokio::time::timeout(std::time::Duration::from_secs(1), first_signal.requested())
            .await
            .expect("replaced signal");
        assert!(
            !registry.release(node, first),
            "stale session must not clear the node"
        );
        assert!(registry.contains(node));
        assert!(!first_signal.was_revoked());
        assert!(registry.revoke(node));
        assert!(!registry.revoke(Uuid::now_v7()));
        // Frames reach the current session only.
        registry.send(node, "frame".into()).unwrap();
        assert_eq!(second_queue.try_recv().unwrap(), "frame");
        assert_eq!(registry.connected(), vec![node]);
        assert_eq!(
            registry.send(Uuid::now_v7(), "x".into()),
            Err(SendError::NotConnected)
        );
        for _ in 0..OUTBOUND_QUEUE {
            registry.send(node, "fill".into()).unwrap();
        }
        assert_eq!(registry.send(node, "more".into()), Err(SendError::Busy));
        assert!(registry.release(node, second));
        assert!(!registry.contains(node));
    }
}
