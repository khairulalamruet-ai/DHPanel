//! Which node has a live session in this process.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::Notify;
use uuid::Uuid;

struct Entry {
    session: u64,
    replaced: Arc<Notify>,
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
    pub(crate) fn register(&self, node: Uuid) -> (u64, Arc<Notify>) {
        let session = self.next.fetch_add(1, Ordering::Relaxed);
        let replaced = Arc::new(Notify::new());
        let previous = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                node,
                Entry {
                    session,
                    replaced: replaced.clone(),
                },
            );
        if let Some(previous) = previous {
            previous.replaced.notify_one();
        }
        (session, replaced)
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
        let (first, first_signal) = registry.register(node);
        let (second, _) = registry.register(node);
        assert_ne!(first, second);
        // The first session was told to close (the permit is stored, so this returns at once).
        tokio::time::timeout(std::time::Duration::from_secs(1), first_signal.notified())
            .await
            .expect("replaced signal");
        assert!(
            !registry.release(node, first),
            "stale session must not clear the node"
        );
        assert!(registry.contains(node));
        assert!(registry.release(node, second));
        assert!(!registry.contains(node));
    }
}
