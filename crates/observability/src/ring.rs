//! The last log events of this process, kept in memory (T10.07), for an administrator to read
//! through the API: a `tracing` layer that copies every event the filter lets through into a
//! bounded ring (the newest 2000).

use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::{Event, Level, Subscriber, field::Visit};
use tracing_subscriber::{Layer, layer::Context};

const CAPACITY: usize = 2000;
const MAX_LINE: usize = 2000;

/// One log event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// When it happened (Unix milliseconds).
    pub at_ms: i64,
    /// `ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`.
    pub level: &'static str,
    /// The module that wrote it.
    pub target: String,
    /// The message and its fields (`message key=value ...`).
    pub text: String,
}

static RING: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

/// The layer that fills the ring.
#[derive(Debug, Default, Clone, Copy)]
pub struct RingLayer;

struct Text(String);

impl Visit for Text {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut text = Text(String::new());
        event.record(&mut text);
        let mut line = text.0;
        if line.len() > MAX_LINE {
            line.truncate(line.floor_char_boundary(MAX_LINE));
        }
        let meta = event.metadata();
        let level = match *meta.level() {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };
        let at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let mut ring = RING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if ring.len() >= CAPACITY {
            ring.pop_front();
        }
        ring.push_back(Entry {
            at_ms,
            level,
            target: meta.target().to_owned(),
            text: line,
        });
    }
}

/// The ring's entries, oldest first, from `since_ms` on, at `min_level` or worse, whose text
/// or target contains `search` (case-insensitive); the newest `limit`.
#[must_use]
pub fn recent(since_ms: i64, min_level: &str, search: Option<&str>, limit: usize) -> Vec<Entry> {
    let rank = |l: &str| match l {
        "ERROR" => 0,
        "WARN" => 1,
        "INFO" => 2,
        "DEBUG" => 3,
        _ => 4,
    };
    let wanted = rank(min_level);
    let needle = search.map(str::to_lowercase).filter(|s| !s.is_empty());
    let ring = RING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut found: Vec<Entry> = ring
        .iter()
        .filter(|e| e.at_ms > since_ms && rank(e.level) <= wanted)
        .filter(|e| {
            needle.as_ref().is_none_or(|n| {
                e.text.to_lowercase().contains(n) || e.target.to_lowercase().contains(n)
            })
        })
        .cloned()
        .collect();
    if found.len() > limit {
        found.drain(..found.len() - limit);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt as _;

    #[test]
    fn events_are_kept_and_found_by_level_text_and_time() {
        let subscriber = tracing_subscriber::registry().with(RingLayer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(user = "alice", "signed in");
            tracing::warn!("disk is filling");
            tracing::error!(code = 7, "backup failed");
            tracing::debug!("noise");
        });
        let all = recent(0, "DEBUG", Some("ring"), 100);
        assert_eq!(all.len(), 4, "{all:?}");
        let bad = recent(0, "WARN", Some("ring"), 100);
        assert_eq!(
            bad.iter().map(|e| e.level).collect::<Vec<_>>(),
            ["WARN", "ERROR"]
        );
        assert!(
            bad[1].text.contains("backup failed") && bad[1].text.contains("code=7"),
            "{bad:?}"
        );
        let found = recent(0, "INFO", Some("SIGNED"), 100);
        assert!(
            found.len() == 1 && found[0].text.contains("user=\"alice\""),
            "{found:?}"
        );
        assert_eq!(recent(0, "DEBUG", Some("ring"), 2).len(), 2);
        assert!(recent(i64::MAX - 1, "DEBUG", None, 10).is_empty());
    }
}
