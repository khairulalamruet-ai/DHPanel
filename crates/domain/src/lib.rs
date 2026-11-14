//! Shared domain entities and value objects.
//!
//! No web, database or async dependencies: these types encode business rules
//! that every layer (HTTP validation, CLI, repositories) must agree on.

pub mod account;
pub mod dns;
pub mod domains;
pub mod error_pages;
pub mod php_ini;
pub mod redirects;
pub mod tenant;
pub mod vhost;

pub use account::{AccountKind, Email, Username, ValidationError};
pub use dns::check_conflicts as check_dns_conflicts;
pub use dns::{Record as DnsRecord, RecordError as DnsRecordError, RecordType as DnsRecordType};
pub use dns::{
    TemplateRecord as DnsTemplateRecord, Variables as DnsVariables,
    check_template as check_dns_template, render as render_dns_template,
};
pub use domains::{Blocklist, Docroot, DomainError, DomainKind, DomainStatus, Fqdn};
pub use redirects::{Place, RedirectCode, RedirectError, SourcePath, Target};
pub use tenant::{Tenant, TenantError, TenantStatus, TenantUsername, Uid, UidRange};
