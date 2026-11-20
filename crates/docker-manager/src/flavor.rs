//! The names this node gives its Docker objects.
//!
//! The panel was called DHPanel before it was Built Panel. A node installed then has its
//! containers, volumes and networks named `dh-…` and labelled `dhpanel.…`, and Docker cannot
//! rename a volume. So such a node keeps those names ("legacy" names), and a node installed
//! since uses `bp-…` and `builtpanel.…`.
//!
//! The code always writes the current names (`bp-db`, [`MANAGED_LABEL`](crate::MANAGED_LABEL),
//! …). They are turned into this node's names only where they meet Docker: [`Name::parse`]
//! (every container, volume and network name goes through it) and the labels sent to and read
//! from Docker in this crate. The agent switches to legacy names at start-up, before it touches
//! Docker, when the upgrade from DHPanel left a marker in its state folder.
//!
//! [`Name::parse`]: crate::Name::parse

use std::sync::atomic::{AtomicBool, Ordering};

static LEGACY: AtomicBool = AtomicBool::new(false);

/// The prefix of names on a node installed as Built Panel.
pub const PREFIX: &str = "bp-";
/// The prefix of names on a node installed as DHPanel.
pub const LEGACY_PREFIX: &str = "dh-";
const LABELS: &str = "builtpanel.";
const LEGACY_LABELS: &str = "dhpanel.";

/// Use the names of a node installed as DHPanel (`dh-…`, `dhpanel.…`) from now on.
pub fn use_legacy_names() {
    LEGACY.store(true, Ordering::SeqCst);
}

/// Whether this node uses the DHPanel names.
#[must_use]
pub fn legacy() -> bool {
    LEGACY.load(Ordering::SeqCst)
}

/// The prefix of this node's container, volume and network names.
#[must_use]
pub fn prefix() -> &'static str {
    if legacy() { LEGACY_PREFIX } else { PREFIX }
}

/// The product's short name as this node's objects carry it (`builtpanel`, or `dhpanel` on a
/// node installed as DHPanel): e.g. the administrator of the node's SFTP server.
#[must_use]
pub fn slug() -> &'static str {
    if legacy() { "dhpanel" } else { "builtpanel" }
}

/// A name or label key written with the current names, as this node has it.
#[must_use]
pub fn wire(key: &str) -> String {
    if !legacy() {
        return key.to_owned();
    }
    if let Some(rest) = key.strip_prefix(LABELS) {
        format!("{LEGACY_LABELS}{rest}")
    } else if let Some(rest) = key.strip_prefix(PREFIX) {
        format!("{LEGACY_PREFIX}{rest}")
    } else {
        key.to_owned()
    }
}

/// A name or label key as Docker reports it, written with the current names.
#[must_use]
pub fn local(key: &str) -> String {
    if let Some(rest) = key.strip_prefix(LEGACY_LABELS) {
        format!("{LABELS}{rest}")
    } else if let Some(rest) = key.strip_prefix(LEGACY_PREFIX) {
        format!("{PREFIX}{rest}")
    } else {
        key.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_translated_both_ways() {
        // The current names stay as they are until a node is switched (tests never switch,
        // since the switch is for the whole process).
        assert_eq!(wire("bp-db"), "bp-db");
        assert_eq!(wire("builtpanel.managed"), "builtpanel.managed");
        assert_eq!(local("dhpanel.tenant"), "builtpanel.tenant");
        assert_eq!(local("dh-db-public"), "bp-db-public");
        assert_eq!(local("bp-db"), "bp-db");
        assert_eq!(local("other.label"), "other.label");
        assert_eq!(prefix(), "bp-");
        assert_eq!(slug(), "builtpanel");
    }
}
