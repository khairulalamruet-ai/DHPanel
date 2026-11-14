//! Shared domain entities and value objects.
//!
//! No web, database or async dependencies: these types encode business rules
//! that every layer (HTTP validation, CLI, repositories) must agree on.

pub mod account;
pub mod domains;
pub mod redirects;
pub mod tenant;

pub use account::{AccountKind, Email, Username, ValidationError};
pub use domains::{Blocklist, Docroot, DomainError, DomainKind, DomainStatus, Fqdn};
pub use redirects::{Place, RedirectCode, RedirectError, SourcePath, Target};
pub use tenant::{Tenant, TenantError, TenantStatus, TenantUsername, Uid, UidRange};
