//! Names and access levels of databases and database users (T07.02).
//!
//! A database or user belongs to one account, and its name starts with the
//! account's system name and an underscore (`alice_shop`), so two accounts never
//! meet in a name and a person can tell whose a database is. The prefix can be
//! switched off by the administrator (a panel that migrates names from elsewhere
//! needs that); every other rule stays: lower-case letters, digits and `_`, at
//! most 64 characters, and nothing that belongs to the database server itself.

use crate::TenantUsername;

/// Longest database or user name.
pub const MAX_NAME_LEN: usize = 64;

/// Names that belong to the database server (or that people would take for them).
const RESERVED: &[&str] = &[
    "mysql",
    "information_schema",
    "performance_schema",
    "sys",
    "test",
    "root",
    "mariadb",
    "mariadb_sys",
    "admin",
    "administrator",
    "dhpanel",
    "pma",
    "phpmyadmin",
    "debian_sys_maint",
    "healthcheck",
];

/// Why a name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DbNameError {
    /// Empty, or something other than `a-z`, `0-9` and `_`.
    #[error("a name is lower-case letters, digits and underscores")]
    Invalid,
    /// Longer than [`MAX_NAME_LEN`] with the prefix.
    #[error("the whole name, with the account prefix, is at most 64 characters")]
    TooLong,
    /// Belongs to the database server.
    #[error("this name is reserved")]
    Reserved,
}

impl DbNameError {
    /// Stable machine code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid_db_name",
            Self::TooLong => "db_name_too_long",
            Self::Reserved => "reserved_db_name",
        }
    }
}

fn plain(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The whole name of a database or user from what a person typed: with the
/// account's prefix (`alice_` + `shop`) when `with_prefix`, as typed otherwise.
///
/// A person may type the name with the prefix already (`alice_shop`): it is not
/// put on twice.
///
/// # Errors
/// [`DbNameError`].
pub fn full_name(
    tenant: &TenantUsername,
    typed: &str,
    with_prefix: bool,
) -> Result<String, DbNameError> {
    let typed = typed.trim().to_ascii_lowercase();
    let prefix = format!("{}_", tenant.as_str());
    let name = if with_prefix && !typed.starts_with(&prefix) {
        format!("{prefix}{typed}")
    } else {
        typed.clone()
    };
    // The part after the prefix has to be there and plain.
    let own = name.strip_prefix(&prefix).unwrap_or(&name);
    if !plain(&name) || !plain(own) {
        return Err(DbNameError::Invalid);
    }
    if name.len() > MAX_NAME_LEN {
        return Err(DbNameError::TooLong);
    }
    if RESERVED.contains(&name.as_str()) || RESERVED.contains(&own) {
        return Err(DbNameError::Reserved);
    }
    Ok(name)
}

/// What a user may do in a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read only.
    ReadOnly,
    /// Read and change data.
    ReadWrite,
    /// Everything inside that database (tables, indexes, routines, ...).
    All,
}

impl Access {
    /// The name used in the API and the ledger.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
            Self::All => "all",
        }
    }

    /// Reads a name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [Self::ReadOnly, Self::ReadWrite, Self::All]
            .into_iter()
            .find(|a| a.as_str() == text)
    }

    /// The privileges it stands for, as the server names them. A fixed list: no
    /// request ever supplies one, and none of them is `GRANT OPTION`, `FILE`,
    /// `SUPER` or anything outside the one database.
    #[must_use]
    pub fn privileges(self) -> &'static [&'static str] {
        match self {
            Self::ReadOnly => &["SELECT", "SHOW VIEW"],
            Self::ReadWrite => &["SELECT", "INSERT", "UPDATE", "DELETE", "SHOW VIEW"],
            Self::All => &[
                "SELECT",
                "INSERT",
                "UPDATE",
                "DELETE",
                "CREATE",
                "DROP",
                "ALTER",
                "INDEX",
                "REFERENCES",
                "CREATE TEMPORARY TABLES",
                "LOCK TABLES",
                "CREATE VIEW",
                "SHOW VIEW",
                "CREATE ROUTINE",
                "ALTER ROUTINE",
                "EXECUTE",
                "TRIGGER",
                "EVENT",
            ],
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn alice() -> TenantUsername {
        TenantUsername::parse("alice").unwrap()
    }

    #[test]
    fn names_get_the_accounts_prefix_once() {
        let a = alice();
        assert_eq!(full_name(&a, "shop", true).unwrap(), "alice_shop");
        assert_eq!(full_name(&a, "alice_shop", true).unwrap(), "alice_shop");
        assert_eq!(full_name(&a, "  Shop_2 ", true).unwrap(), "alice_shop_2");
        // Without the prefix rule the name is as typed (still checked).
        assert_eq!(full_name(&a, "shop", false).unwrap(), "shop");
        assert_eq!(full_name(&a, "alice_shop", false).unwrap(), "alice_shop");
    }

    #[test]
    fn what_cannot_be_an_identifier_or_belongs_to_the_server_is_refused() {
        let a = alice();
        for (typed, err) in [
            ("", DbNameError::Invalid),
            ("   ", DbNameError::Invalid),
            ("sh op", DbNameError::Invalid),
            ("shop-2", DbNameError::Invalid),
            ("shop;drop", DbNameError::Invalid),
            ("a`b", DbNameError::Invalid),
            ("sh'op", DbNameError::Invalid),
            ("sh\nop", DbNameError::Invalid),
            ("ünï", DbNameError::Invalid),
            ("alice_", DbNameError::Invalid),
            ("mysql", DbNameError::Reserved),
            ("information_schema", DbNameError::Reserved),
            ("alice_mysql", DbNameError::Reserved),
            ("root", DbNameError::Reserved),
        ] {
            assert_eq!(full_name(&a, typed, true).unwrap_err(), err, "{typed:?}");
        }
        // Without the prefix a server name is still reserved.
        for typed in ["mysql", "sys", "performance_schema", "root"] {
            assert_eq!(
                full_name(&a, typed, false).unwrap_err(),
                DbNameError::Reserved
            );
        }
        // The whole name, prefix included, has a limit.
        let long = "x".repeat(MAX_NAME_LEN - "alice_".len());
        assert!(full_name(&a, &long, true).is_ok());
        assert_eq!(
            full_name(&a, &format!("{long}x"), true).unwrap_err(),
            DbNameError::TooLong
        );
        assert_eq!(DbNameError::Reserved.code(), "reserved_db_name");
    }

    #[test]
    fn access_levels_name_fixed_privileges() {
        for a in [Access::ReadOnly, Access::ReadWrite, Access::All] {
            assert_eq!(Access::parse(a.as_str()), Some(a));
            for p in a.privileges() {
                assert!(
                    p.bytes().all(|b| b.is_ascii_uppercase() || b == b' '),
                    "{p}"
                );
                for never in ["GRANT", "FILE", "SUPER", "PROCESS", "SHUTDOWN", "RELOAD"] {
                    assert!(!p.contains(never), "{p}");
                }
            }
        }
        assert_eq!(Access::parse("root"), None);
        assert!(Access::ReadOnly.privileges().len() < Access::ReadWrite.privileges().len());
        assert!(Access::ReadWrite.privileges().len() < Access::All.privileges().len());
    }
}
