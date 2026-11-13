//! Role-based access control.
//!
//! * A **permission** is a dotted key, e.g. `admin.roles.manage`.
//! * A **grant** (stored per role) is an exact key, a subtree `admin.*`, or
//!   `*` for everything. [`PermissionSet`] answers whether a set of grants
//!   allows a key.
//! * Handlers declare what they need in their signature with
//!   [`Require<P>`]; a permission type is declared with [`permission!`]:
//!
//! ```
//! dhpanel_rbac::permission!(
//!     /// Create, change and remove roles.
//!     pub RolesManage = "admin.roles.manage"
//! );
//! // async fn create_role(Require(who, ..): Require<RolesManage>, ...) { ... }
//! ```
//!
//! [`Require`] does everything [`dhpanel_session::Authenticated`] does
//! (session, CSRF on state-changing methods), then loads the account's roles
//! and answers:
//! * 401 when the account no longer exists or is not active (and ends the session),
//! * 403 `mfa_setup_required` when the account must use two-factor
//!   authentication and has not enrolled yet (it can still use its own
//!   security endpoints to do so),
//! * 403 `permission_denied` when no role grants the permission.
//!
//! [`RequireRecent`] additionally demands sudo mode (a recent login or
//! re-authentication) and answers 403 `reauth_required` otherwise; use it for
//! changes to other people's access.
//!
//! Permissions are read from PostgreSQL on every request, so a role change
//! takes effect immediately.

use std::marker::PhantomData;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use dhpanel_database::{
    accounts::{DbAccountKind, DbAccountStatus},
    rbac::load_access,
    sqlx::PgPool,
};
use dhpanel_http_kit::AppError;
use dhpanel_session::{Authenticated, RevokeReason, SessionData, SessionStore, cookie_value};
use uuid::Uuid;

/// Core permission keys that exist before any feature module is loaded.
pub mod permissions {
    crate::permission!(
        /// See the list of administrators.
        pub AdministratorsRead = "admin.administrators.read"
    );
    crate::permission!(
        /// Create, change and remove administrators and their roles.
        pub AdministratorsManage = "admin.administrators.manage"
    );
    crate::permission!(
        /// See roles and their permissions.
        pub RolesRead = "admin.roles.read"
    );
    crate::permission!(
        /// Create, change and remove roles.
        pub RolesManage = "admin.roles.manage"
    );
    crate::permission!(
        /// Read the audit log.
        pub AuditRead = "admin.audit.read"
    );
    crate::permission!(
        /// See nodes and their enrollment tokens.
        pub NodesRead = "admin.nodes.read"
    );
    crate::permission!(
        /// Add, change and remove nodes (create and revoke enrollment tokens).
        pub NodesManage = "admin.nodes.manage"
    );
    crate::permission!(
        /// See which modules exist and which are switched on.
        pub ModulesRead = "admin.modules.read"
    );
    crate::permission!(
        /// Switch modules on and off.
        pub ModulesManage = "admin.modules.manage"
    );
    crate::permission!(
        /// See the panel's settings.
        pub SettingsRead = "admin.settings.read"
    );
    crate::permission!(
        /// Change the panel's settings.
        pub SettingsManage = "admin.settings.manage"
    );
}

/// Name of the built-in role that grants everything.
pub const SUPER_ADMIN_ROLE: &str = "super_admin";

/// A permission known at compile time. Implement with [`permission!`].
pub trait PermissionKey: Send + Sync + 'static {
    /// The dotted key, e.g. `admin.roles.manage`.
    const KEY: &'static str;
}

/// Declares a permission type: `permission!(pub Name = "dotted.key");`
/// The key is checked at compile time (see [`is_valid_key`]).
#[macro_export]
macro_rules! permission {
    ($(#[$meta:meta])* $vis:vis $name:ident = $key:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis struct $name;

        impl $crate::PermissionKey for $name {
            const KEY: &'static str = {
                assert!($crate::is_valid_key($key), "invalid permission key");
                $key
            };
        }
    };
}

const fn is_segment_start(b: u8) -> bool {
    b.is_ascii_lowercase()
}

const fn is_segment_char(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// Whether `key` is a well-formed permission key: at least two dot-separated
/// segments of `[a-z][a-z0-9_]*` (the same rule as the database CHECK).
#[must_use]
pub const fn is_valid_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut i = 0;
    let mut segments = 0;
    let mut at_start = true;
    while i < bytes.len() {
        let b = bytes[i];
        if at_start {
            if !is_segment_start(b) {
                return false;
            }
            at_start = false;
            segments += 1;
        } else if b == b'.' {
            at_start = true;
        } else if !is_segment_char(b) {
            return false;
        }
        i += 1;
    }
    !at_start && segments >= 2
}

/// Whether `grant` is storable for a role: a valid key, a subtree wildcard
/// (`admin.*`, `admin.roles.*`) or `*`.
#[must_use]
pub fn is_valid_grant(grant: &str) -> bool {
    if grant == "*" {
        return true;
    }
    match grant.strip_suffix(".*") {
        // "admin.*": the prefix is one or more segments.
        Some(prefix) => {
            !prefix.is_empty()
                && prefix.split('.').all(|seg| {
                    let mut bytes = seg.bytes();
                    bytes.next().is_some_and(is_segment_start) && bytes.all(is_segment_char)
                })
        }
        None => is_valid_key(grant),
    }
}

/// The grants an account holds through its roles.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet {
    grants: Vec<String>,
}

impl PermissionSet {
    /// From stored grants. Malformed entries are ignored, so a bad row can
    /// only ever remove access.
    #[must_use]
    pub fn new(grants: impl IntoIterator<Item = String>) -> Self {
        let mut grants: Vec<String> = grants.into_iter().filter(|g| is_valid_grant(g)).collect();
        grants.sort();
        grants.dedup();
        Self { grants }
    }

    /// Whether `key` is allowed. Only well-formed keys can be allowed; a
    /// subtree grant `a.b.*` covers `a.b.c` and deeper, but not `a.b` itself
    /// and not `a.bc.d`.
    #[must_use]
    pub fn allows(&self, key: &str) -> bool {
        is_valid_key(key)
            && self.grants.iter().any(|grant| {
                grant == "*"
                    || grant == key
                    || grant
                        .strip_suffix('*')
                        .is_some_and(|prefix| prefix.ends_with('.') && key.starts_with(prefix))
            })
    }

    /// The raw grants, sorted (for showing to the client).
    #[must_use]
    pub fn grants(&self) -> &[String] {
        &self.grants
    }

    /// Whether everything is allowed.
    #[must_use]
    pub fn is_unrestricted(&self) -> bool {
        self.grants.iter().any(|g| g == "*")
    }
}

/// The authenticated, authorised caller.
#[derive(Debug, Clone)]
pub struct Principal {
    /// The session.
    pub session: SessionData,
    /// Account kind.
    pub kind: DbAccountKind,
    /// Role names, sorted.
    pub roles: Vec<String>,
    /// What the roles grant.
    pub permissions: PermissionSet,
}

impl Principal {
    /// The account id.
    #[must_use]
    pub fn account_id(&self) -> Uuid {
        self.session.account_id
    }

    /// Whether the caller has the given role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Checks a permission chosen at run time (e.g. depending on the request
    /// body); prefer [`Require`] when it is known statically.
    ///
    /// # Errors
    /// 403 `permission_denied`.
    pub fn ensure(&self, key: &str) -> Result<(), AppError> {
        if self.permissions.allows(key) {
            Ok(())
        } else {
            tracing::debug!(account_id = %self.account_id(), permission = key, "permission denied");
            Err(permission_denied())
        }
    }
}

fn permission_denied() -> AppError {
    AppError::forbidden(
        "permission_denied",
        "You do not have permission to do this.",
    )
}

/// Loads the caller's roles; ends the session when the account is gone or
/// no longer active.
async fn load_principal<S>(parts: &mut Parts, state: &S) -> Result<Principal, AppError>
where
    SessionStore: FromRef<S>,
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    let Authenticated(session) = Authenticated::from_request_parts(parts, state).await?;
    let pool = PgPool::from_ref(state);
    let access = load_access(&pool, session.account_id)
        .await
        .map_err(AppError::internal)?
        .filter(|a| a.status == DbAccountStatus::Active);
    let Some(access) = access else {
        if let Some(value) = cookie_value(&parts.headers) {
            let _ = SessionStore::from_ref(state)
                .revoke(value, RevokeReason::Security)
                .await;
        }
        return Err(AppError::Unauthorized);
    };
    if access.mfa_required && !access.mfa_enabled {
        return Err(AppError::forbidden(
            "mfa_setup_required",
            "Set up two-factor authentication to continue.",
        ));
    }
    Ok(Principal {
        session,
        kind: access.kind,
        roles: access.roles,
        permissions: PermissionSet::new(access.permissions),
    })
}

impl<S> FromRequestParts<S> for Principal
where
    SessionStore: FromRef<S>,
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        load_principal(parts, state).await
    }
}

/// Extractor that admits only callers holding permission `P`.
/// Destructure as `Require(principal, ..)`.
#[derive(Debug, Clone)]
pub struct Require<P: PermissionKey>(pub Principal, pub PhantomData<P>);

impl<S, P> FromRequestParts<S> for Require<P>
where
    SessionStore: FromRef<S>,
    PgPool: FromRef<S>,
    S: Send + Sync,
    P: PermissionKey,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let principal = load_principal(parts, state).await?;
        principal.ensure(P::KEY)?;
        Ok(Self(principal, PhantomData))
    }
}

/// Like [`Require`], and the caller must also be in sudo mode. The permission
/// is checked first, so callers without it learn nothing about sudo mode.
#[derive(Debug, Clone)]
pub struct RequireRecent<P: PermissionKey>(pub Principal, pub PhantomData<P>);

impl<S, P> FromRequestParts<S> for RequireRecent<P>
where
    SessionStore: FromRef<S>,
    PgPool: FromRef<S>,
    S: Send + Sync,
    P: PermissionKey,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let principal = load_principal(parts, state).await?;
        principal.ensure(P::KEY)?;
        let store = SessionStore::from_ref(state);
        if principal
            .session
            .sudo_remaining(store.now(), store.config().sudo_window)
            .is_none()
        {
            return Err(AppError::forbidden(
                "reauth_required",
                "Confirm your identity to continue.",
            ));
        }
        Ok(Self(principal, PhantomData))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn set(grants: &[&str]) -> PermissionSet {
        PermissionSet::new(grants.iter().map(|g| (*g).to_owned()))
    }

    #[test]
    fn keys_need_two_wellformed_segments() {
        for ok in [
            "admin.roles",
            "admin.roles.manage",
            "a.b",
            "dns_zones.records2.read",
        ] {
            assert!(is_valid_key(ok), "{ok}");
        }
        for bad in [
            "",
            "admin",
            "admin.",
            ".roles",
            "admin..roles",
            "Admin.roles",
            "admin.Roles",
            "admin.1roles",
            "admin.ro-les",
            "admin.roles.*",
            "*",
            "admin roles.x",
            "admin.rolés",
        ] {
            assert!(!is_valid_key(bad), "{bad}");
        }
    }

    #[test]
    fn grants_may_be_keys_or_wildcards() {
        for ok in ["*", "admin.*", "admin.roles.*", "admin.roles.manage"] {
            assert!(is_valid_grant(ok), "{ok}");
        }
        for bad in [
            "",
            "admin",
            ".*",
            "*.*",
            "admin.*.read",
            "admin.**",
            "admin*",
            "Admin.*",
            "admin..*",
        ] {
            assert!(!is_valid_grant(bad), "{bad}");
        }
    }

    #[test]
    fn exact_grants_allow_only_that_key() {
        let s = set(&["admin.roles.read"]);
        assert!(s.allows("admin.roles.read"));
        for denied in [
            "admin.roles.manage",
            "admin.roles",
            "admin.roles.read.all",
            "admin.audit.read",
        ] {
            assert!(!s.allows(denied), "{denied}");
        }
    }

    #[test]
    fn subtree_grants_cover_descendants_but_not_siblings_or_the_node_itself() {
        let s = set(&["admin.roles.*"]);
        assert!(s.allows("admin.roles.read"));
        assert!(s.allows("admin.roles.members.add"));
        for denied in [
            "admin.roles",
            "admin.rolesx.read",
            "admin.audit.read",
            "client.roles.read",
        ] {
            assert!(!s.allows(denied), "{denied}");
        }
        assert!(set(&["admin.*"]).allows("admin.audit.read"));
        assert!(!set(&["admin.*"]).allows("administrators.read"));
    }

    #[test]
    fn star_allows_every_wellformed_key_and_nothing_malformed() {
        let s = set(&["*"]);
        assert!(s.is_unrestricted());
        assert!(s.allows("anything.at_all"));
        for malformed in ["", "*", "admin", "admin.*", "Admin.x"] {
            assert!(!s.allows(malformed), "{malformed}");
        }
    }

    #[test]
    fn empty_and_malformed_grants_allow_nothing() {
        assert!(!PermissionSet::default().allows("admin.roles.read"));
        let s = set(&["admin", "admin*", "**", "admin.roles.read ", ""]);
        assert!(s.grants().is_empty());
        assert!(!s.allows("admin.roles.read"));
        assert!(!s.is_unrestricted());
    }

    #[test]
    fn grants_are_sorted_and_deduplicated() {
        let s = set(&["b.c", "a.b", "b.c"]);
        assert_eq!(s.grants(), ["a.b", "b.c"]);
    }

    #[test]
    fn core_permission_keys_are_declared() {
        assert_eq!(permissions::RolesManage::KEY, "admin.roles.manage");
        assert_eq!(permissions::AuditRead::KEY, "admin.audit.read");
        assert!(is_valid_key(permissions::AdministratorsManage::KEY));
        assert!(is_valid_key(permissions::AdministratorsRead::KEY));
        assert!(is_valid_key(permissions::RolesRead::KEY));
        assert_eq!(permissions::NodesRead::KEY, "admin.nodes.read");
        assert_eq!(permissions::NodesManage::KEY, "admin.nodes.manage");
    }
}
