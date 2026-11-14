//! Wire types shared by the control plane and the node agent.
//!
//! See `docs/adr/0004-node-agent-protocol.md`. In short: one WebSocket per
//! node over mutual TLS, one JSON object per text frame, at most
//! [`MAX_FRAME_BYTES`]. Both ends are Rust and use these very types, so a
//! mismatch is a compile error, not a runtime surprise.
//!
//! ## Compatibility rules
//! * Changes within one [`PROTOCOL_VERSION`] are **additive**: new operations,
//!   new outputs, new optional fields (give them `#[serde(default)]`).
//!   Unknown JSON fields are ignored, never rejected.
//! * An operation an older agent does not know does not break the connection:
//!   [`parse_operation_frame`] returns [`Parsed::Unsupported`] and the agent
//!   answers that `op_id` with an `unsupported_operation` error.
//! * A breaking change bumps [`PROTOCOL_VERSION`]; the control plane keeps
//!   accepting the previous one ([`MIN_SUPPORTED_VERSION`]).
//!
//! No operation here carries a secret; if one ever must, its type gets a
//! redacting `Debug` and is left out of logs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version spoken by this build.
pub const PROTOCOL_VERSION: u16 = 1;
/// Oldest version this build still understands.
pub const MIN_SUPPORTED_VERSION: u16 = 1;
/// Largest frame accepted or produced, in bytes. Bulk data uses other channels.
pub const MAX_FRAME_BYTES: usize = 1 << 20;
/// Longest time an envelope may stay valid, in seconds. Bounds the replay window.
pub const MAX_ENVELOPE_LIFETIME_SECS: i64 = 3600;
/// How far in the future `issued_at` may be before the agent distrusts it.
pub const MAX_CLOCK_SKEW_SECS: i64 = 300;

/// How often the control plane pings an agent, in seconds. The agent answers
/// with a pong; a ping also tells the agent the control plane is alive.
pub const HEARTBEAT_INTERVAL_SECS: u64 = 15;
/// A session that has sent nothing (no pong, no message) for this long (three
/// missed pings) is dead: the control plane closes it and the node shows as
/// offline. The check runs on each ping tick, so a node that vanishes without a
/// goodbye is offline within this plus one interval: at most 60 seconds.
pub const LIVENESS_TIMEOUT_SECS: u64 = 45;
/// An agent that has heard nothing from the control plane for this long
/// (three missed pings) drops the connection and reconnects.
pub const AGENT_SILENCE_TIMEOUT_SECS: u64 = 45;
/// Path of the agent port's WebSocket endpoint (the only one it serves).
pub const STREAM_PATH: &str = "/agent/v1/stream";

/// The protocol version to speak with a peer announcing `peer`, or `None`
/// when the peer is too old.
#[must_use]
pub fn negotiate_version(peer: u16) -> Option<u16> {
    (peer >= MIN_SUPPORTED_VERSION).then_some(peer.min(PROTOCOL_VERSION))
}

// ------------------------------------------------------------------ errors

/// Machine-readable failure reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The agent does not know this operation (older agent, newer control plane).
    UnsupportedOperation,
    /// The operation is known but its arguments are invalid.
    InvalidArguments,
    /// The envelope's validity window has passed (or is not plausible).
    Expired,
    /// The envelope's protocol version is not supported.
    UnsupportedVersion,
    /// The agent is running as many operations as it allows; try again shortly.
    Busy,
    /// The operation did not finish in the time it had.
    Timeout,
    /// Something went wrong inside the agent; see the message.
    Internal,
}

/// A failure reported by the agent. The message is safe to show to administrators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AgentError {
    /// What kind of failure.
    pub code: ErrorCode,
    /// Human-readable detail (English; never contains secrets).
    pub message: String,
    /// Whether trying the same operation again may succeed.
    #[serde(default)]
    pub retryable: bool,
}

impl AgentError {
    /// A non-retryable error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    /// Marks the error as worth retrying.
    #[must_use]
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

// ------------------------------------------------------------------- routes

/// One redirect of a site: the exact path it applies to and where it sends the
/// visitor. Both were checked by the control plane and are checked again by the
/// agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRedirect {
    /// The exact path, starting with `/`.
    pub path: String,
    /// A path of the same site or an http(s) address.
    pub target: String,
    /// 301, 302, 307 or 308.
    pub code: u16,
}

/// How the web server answers for a group of names: the same site under every
/// name in `hosts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteRoute {
    /// The names (lower-case ASCII), at least one.
    pub hosts: Vec<String>,
    /// Where the site's web container listens, `host:port` (for example
    /// `dh-alice-web:8080`), or `None` for a suspended site: visitors then get
    /// the "suspended" page and nothing is served from the tenant's files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Redirects, answered before the site is asked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirects: Vec<RouteRedirect>,
    /// Visitors who come over plain HTTP are sent to the same address over HTTPS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force_https: bool,
    /// Answers over HTTPS carry `Strict-Transport-Security` (one year). Only with
    /// `force_https`; the agent refuses it without.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hsts: bool,
}

/// A certificate a person uploaded for one of their names, for the web server to use.
///
/// The private key is not part of what the control plane records in its ledger:
/// there it is `None`, and the dispatcher fills it in at the moment the message is
/// sent (see `dhpanel_dispatcher::SecretFiller`). `Debug` never shows it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCertificate {
    /// The name it is for (lower-case ASCII).
    pub host: String,
    /// The certificates, PEM: the site's first, then the ones that vouch for it.
    pub certificate: String,
    /// The private key, PKCS#8 PEM. Present only on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}

impl std::fmt::Debug for CustomCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomCertificate")
            .field("host", &self.host)
            .field("has_key", &self.private_key.is_some())
            .finish_non_exhaustive()
    }
}

/// Longest php.ini file the control plane sends.
pub const MAX_PHP_INI_BYTES: usize = 4096;

/// Most PHP versions besides the account's own that one tenant may run.
pub const MAX_EXTRA_PHP_VERSIONS: usize = 3;

/// Longest web server configuration of one domain.
pub const MAX_VHOST_CONFIG_BYTES: usize = 32 * 1024;

/// Most uploaded certificates one tenant may have on a node.
pub const MAX_CUSTOM_CERTIFICATES_PER_TENANT: usize = 100;

/// A certificate the web server holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CertificateInfo {
    /// The name it is for (lower-case ASCII).
    pub host: String,
    /// Who issued it (the issuer's common name).
    pub issuer: String,
    /// Valid from (Unix seconds).
    pub not_before: i64,
    /// Valid until (Unix seconds).
    pub not_after: i64,
}

/// Most certificates one report holds.
pub const MAX_CERTIFICATES_PER_REPORT: usize = 4000;

/// Most sites one tenant may have on a node.
pub const MAX_SITES_PER_TENANT: usize = 200;
/// Most names in all of a tenant's sites.
pub const MAX_HOSTS_PER_TENANT: usize = 500;
/// Most redirects in all of a tenant's sites.
pub const MAX_REDIRECTS_PER_TENANT: usize = 500;

// ------------------------------------------------------------ skeleton files

/// One default file of a new tenant: text, at a plain relative path below its
/// docroot. At most [`SKELETON_MAX_FILES`] files, [`SKELETON_MAX_FILE_BYTES`]
/// each and [`SKELETON_MAX_TOTAL_BYTES`] together: they travel inside one
/// operation, which is a single bounded message end to end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkeletonFile {
    /// Path with `/` between names, for example `css/site.css`.
    pub path: String,
    /// The file's text.
    pub content: String,
}

/// Most skeleton files.
pub const SKELETON_MAX_FILES: usize = 20;
/// Largest skeleton file, in bytes.
pub const SKELETON_MAX_FILE_BYTES: usize = 16 * 1024;
/// Most bytes in all skeleton files together.
pub const SKELETON_MAX_TOTAL_BYTES: usize = 24 * 1024;
/// Deepest nesting of a skeleton file.
pub const SKELETON_MAX_DEPTH: usize = 6;
/// Longest single name.
pub const SKELETON_MAX_NAME: usize = 64;

/// Checks one skeleton path: relative, plain names (letters, digits, `.`, `_`,
/// `-`), no `.` or `..`, limited depth.
///
/// # Errors
/// What is wrong, for the person who wrote it.
pub fn check_skeleton_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() || path.len() > SKELETON_MAX_DEPTH * (SKELETON_MAX_NAME + 1) {
        return Err("the path must be 1 to 450 characters");
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > SKELETON_MAX_DEPTH {
        return Err("the path is nested too deeply (at most 6 levels)");
    }
    for part in parts {
        let plain = !part.is_empty()
            && part.len() <= SKELETON_MAX_NAME
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !plain {
            return Err(
                "names may hold letters, digits, dot, dash and underscore only (no empty names, `.` or `..`)",
            );
        }
    }
    Ok(())
}

/// Checks a whole list the way the node will. Within these limits the JSON of a
/// set-up operation stays under 56 KiB, inside the 64 KiB of the operations
/// ledger and of the privileged helper's request line.
///
/// # Errors
/// What is wrong, for the person who wrote it.
pub fn check_skeleton(files: &[SkeletonFile]) -> Result<(), &'static str> {
    if files.len() > SKELETON_MAX_FILES {
        return Err("too many files (at most 20)");
    }
    let mut total = 0usize;
    for (i, file) in files.iter().enumerate() {
        check_skeleton_path(&file.path)?;
        if file.content.len() > SKELETON_MAX_FILE_BYTES {
            return Err("a file is larger than 16 KiB");
        }
        // Control characters other than line breaks and tabs can grow six-fold
        // when written as JSON, which would break the size promise below.
        if file
            .content
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("a file holds control characters (only line breaks and tabs are allowed)");
        }
        total += file.content.len();
        if total > SKELETON_MAX_TOTAL_BYTES {
            return Err("the files are larger than 24 KiB together");
        }
        for other in &files[..i] {
            if other.path == file.path {
                return Err("a path is listed twice");
            }
            let (a, b) = (other.path.as_str(), file.path.as_str());
            if (b.len() > a.len() && b.starts_with(a) && b.as_bytes()[a.len()] == b'/')
                || (a.len() > b.len() && a.starts_with(b) && a.as_bytes()[b.len()] == b'/')
            {
                return Err("a path is both a file and a directory of another file");
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------- operations

/// What the control plane asks a node to do. Every variant is a narrow,
/// typed action; there is no variant that runs a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    /// Liveness check; the agent answers [`Output::Pong`] with the same nonce.
    Ping {
        /// Echoed back.
        nonce: u64,
    },
    /// Ask the node to describe itself; answered with [`Output::Capabilities`].
    ReportCapabilities,
    /// Prepare a node for a new tenant: its directories (owned by its id), its
    /// private network and its files volume. Safe to repeat. Answered with
    /// [`Output::TenantCreated`].
    CreateTenant {
        /// The tenant's id (also its directory name).
        tenant_id: Uuid,
        /// The tenant's system name (`a-z`, `0-9`, 3-32 characters).
        username: String,
        /// The tenant's numeric user id (10000..59999) on this node.
        uid: u32,
    },
    /// Limit what a tenant may store: bytes and number of files. `None` is
    /// unlimited. The node must support project quotas
    /// ([`Capabilities::storage_quota`]), else the operation fails as unsupported.
    SetStorageQuota {
        /// The tenant's id.
        tenant_id: Uuid,
        /// The tenant's numeric user id.
        uid: u32,
        /// Most bytes.
        disk_bytes: Option<u64>,
        /// Most files and directories.
        inodes: Option<u64>,
    },
    /// Create and start a tenant's containers (web server + PHP) from the
    /// stack template, and wait until they run. Needs [`Operation::CreateTenant`]
    /// first. Safe to repeat: containers that exist are only started. Answered
    /// with [`Output::StackProvisioned`].
    ProvisionTenantStack {
        /// The tenant's id.
        tenant_id: Uuid,
        /// The tenant's system name.
        username: String,
        /// The tenant's numeric user id on this node.
        uid: u32,
        /// CPU the plan gives the stack, in thousandths of a core (`None` = unlimited).
        cpu_millis: Option<u32>,
        /// Memory the plan gives the stack, in bytes (`None` = unlimited).
        memory_bytes: Option<u64>,
        /// Processes the plan gives the stack (`None` = unlimited).
        pids: Option<u32>,
    },
    /// Make the tenant's PHP containers match: the account's version in its main
    /// PHP container, one more container for each extra version, none for any
    /// other. A container that runs another version than asked is replaced (the
    /// files stay), new ones are started and waited for, and the plan's PHP share
    /// is divided between them. The web server is not touched (its configuration
    /// says which container each site uses). Safe to repeat. Answered with
    /// [`Output::PhpVersionsSet`].
    SetPhpVersions {
        /// The tenant's id.
        tenant_id: Uuid,
        /// The tenant's system name.
        username: String,
        /// The tenant's numeric user id on this node.
        uid: u32,
        /// CPU the plan gives the stack, in thousandths of a core (`None` = unlimited).
        cpu_millis: Option<u32>,
        /// Memory the plan gives the stack, in bytes (`None` = unlimited).
        memory_bytes: Option<u64>,
        /// Processes the plan gives the stack (`None` = unlimited).
        pids: Option<u32>,
        /// The version of the main PHP container, `8.4`.
        php_version: String,
        /// More versions to run next to it (at most [`MAX_EXTRA_PHP_VERSIONS`]).
        extra_php: Vec<String>,
    },
    /// Restart one of the tenant's own services: its web server (`web`) or all its
    /// PHP containers (`php`): stopped with a short grace period, started, and
    /// waited for. The files stay. Answered with [`Output::ServiceRestarted`].
    RestartService {
        /// The tenant's system name.
        username: String,
        /// `web` or `php`.
        service: String,
    },
    /// Put the tenant's php.ini options (a file of `option = value` lines made by
    /// the control plane) where its PHP containers read them and have the running
    /// ones load it. The file is checked against the list of options that may be
    /// set. Safe to repeat. Answered with [`Output::PhpIniSet`].
    SetPhpIni {
        /// The tenant's system name.
        username: String,
        /// The file (at most [`MAX_PHP_INI_BYTES`]; empty removes the options).
        ini: String,
    },
    /// Everything a new tenant needs on its node, in one operation: directories,
    /// network and volume ([`Operation::CreateTenant`]), the storage limits
    /// ([`Operation::SetStorageQuota`], skipped when the plan has none or the node
    /// cannot enforce them) and the containers
    /// ([`Operation::ProvisionTenantStack`]). Safe to repeat. Answered with
    /// [`Output::TenantReady`].
    SetUpTenant {
        /// The tenant's id.
        tenant_id: Uuid,
        /// The tenant's system name.
        username: String,
        /// The tenant's numeric user id on this node.
        uid: u32,
        /// Most bytes of storage (`None` = unlimited).
        disk_bytes: Option<u64>,
        /// Most files (`None` = unlimited).
        inodes: Option<u64>,
        /// CPU in thousandths of a core (`None` = unlimited).
        cpu_millis: Option<u32>,
        /// Memory in bytes (`None` = unlimited).
        memory_bytes: Option<u64>,
        /// Processes (`None` = unlimited).
        pids: Option<u32>,
        /// Default files for the tenant's docroot, written when its directories
        /// are first made (see [`SkeletonFile`] for the limits).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skeleton: Vec<SkeletonFile>,
    },
    /// Set the certificates one tenant uploaded: the web server uses them for those
    /// names instead of getting its own. Replaces what the tenant had; an empty list
    /// removes them. Kept by the agent (the keys in a file only it can read) and put
    /// back whenever the web server loses them. Answered with
    /// [`Output::CustomCertificatesSet`].
    SetCustomCertificates {
        /// The tenant's system name.
        username: String,
        /// The certificates, each with its key.
        items: Vec<CustomCertificate>,
    },
    /// Put one domain's web server configuration (rendered by the control plane)
    /// into the tenant's web container and have it reloaded. The file is checked
    /// by the web server itself before it is used; when it is refused the
    /// previous file is put back and the operation fails. Safe to repeat.
    /// Answered with [`Output::DomainAttached`].
    AttachDomain {
        /// The tenant's system name.
        username: String,
        /// The domain, lower-case ASCII; the file is named for it.
        domain: String,
        /// The configuration (at most [`MAX_VHOST_CONFIG_BYTES`]).
        config: String,
    },
    /// Remove one domain's web server configuration and have it reloaded. Safe
    /// to repeat. Answered with [`Output::DomainDetached`].
    DetachDomain {
        /// The tenant's system name.
        username: String,
        /// The domain.
        domain: String,
    },
    /// Remove the web server configuration of every domain of the tenant that is
    /// not in `domains`, and reload. What the control plane sends after it has
    /// attached the tenant's current domains, so a domain that was deleted does not
    /// stay served. Safe to repeat. Answered with [`Output::DomainsKept`].
    KeepDomains {
        /// The tenant's system name.
        username: String,
        /// The domains whose configuration stays (at most [`MAX_HOSTS_PER_TENANT`]).
        domains: Vec<String>,
    },
    /// List the certificates the node's web server holds (name, issuer, validity).
    /// Answered with [`Output::Certificates`].
    ReportCertificates,
    /// Set the web server's routes for one tenant: which names it answers for and
    /// what it does for each (proxy to the tenant's web container, show the
    /// "suspended" page, redirect paths). Replaces what the tenant had; an empty
    /// list removes the tenant's routes. The agent keeps them and puts them back
    /// whenever the web server loses them. Answered with [`Output::SiteRoutesSet`].
    SetSiteRoutes {
        /// The tenant's system name.
        username: String,
        /// The tenant's sites.
        routes: Vec<SiteRoute>,
    },
    /// Stop a tenant's containers; its files stay. Safe to repeat. Answered with
    /// [`Output::TenantSuspended`].
    SuspendTenant {
        /// The tenant's system name.
        username: String,
    },
    /// Start a suspended tenant's containers again and wait until they run.
    /// Safe to repeat. Answered with [`Output::TenantResumed`].
    ResumeTenant {
        /// The tenant's system name.
        username: String,
    },
    /// Remove a tenant from the node: containers, network, volume and, when
    /// `purge_data` is set, its files and directories (without it the files stay
    /// for a later purge). Safe to repeat. Answered with [`Output::TenantDeleted`].
    DeleteTenant {
        /// The tenant's id.
        tenant_id: Uuid,
        /// The tenant's system name.
        username: String,
        /// The tenant's numeric user id.
        uid: u32,
        /// Delete the site files too. Irreversible.
        purge_data: bool,
    },
    /// Change the limits of a tenant's running containers to a plan's, without
    /// restarting them. Answered with [`Output::ResourceLimitsSet`].
    SetResourceLimits {
        /// The tenant's system name.
        username: String,
        /// CPU in thousandths of a core (`None` = unlimited).
        cpu_millis: Option<u32>,
        /// Memory in bytes (`None` = unlimited).
        memory_bytes: Option<u64>,
        /// Processes (`None` = unlimited).
        pids: Option<u32>,
    },
}

impl Operation {
    /// Wire names of every operation this build knows, in declaration order.
    pub const TYPE_NAMES: &'static [&'static str] = &[
        "ping",
        "report_capabilities",
        "create_tenant",
        "set_storage_quota",
        "provision_tenant_stack",
        "set_resource_limits",
        "set_php_versions",
        "set_php_ini",
        "restart_service",
        "set_up_tenant",
        "set_site_routes",
        "report_certificates",
        "set_custom_certificates",
        "attach_domain",
        "detach_domain",
        "keep_domains",
        "suspend_tenant",
        "resume_tenant",
        "delete_tenant",
    ];

    /// The operation's wire name.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Ping { .. } => "ping",
            Self::ReportCapabilities => "report_capabilities",
            Self::CreateTenant { .. } => "create_tenant",
            Self::SetStorageQuota { .. } => "set_storage_quota",
            Self::ProvisionTenantStack { .. } => "provision_tenant_stack",
            Self::SetResourceLimits { .. } => "set_resource_limits",
            Self::SetPhpVersions { .. } => "set_php_versions",
            Self::SetPhpIni { .. } => "set_php_ini",
            Self::RestartService { .. } => "restart_service",
            Self::SetUpTenant { .. } => "set_up_tenant",
            Self::SetSiteRoutes { .. } => "set_site_routes",
            Self::ReportCertificates => "report_certificates",
            Self::SetCustomCertificates { .. } => "set_custom_certificates",
            Self::AttachDomain { .. } => "attach_domain",
            Self::DetachDomain { .. } => "detach_domain",
            Self::KeepDomains { .. } => "keep_domains",
            Self::SuspendTenant { .. } => "suspend_tenant",
            Self::ResumeTenant { .. } => "resume_tenant",
            Self::DeleteTenant { .. } => "delete_tenant",
        }
    }
}

/// One operation with the metadata needed to run it exactly once, in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEnvelope {
    /// The operations-ledger id; results refer to it.
    pub op_id: Uuid,
    /// Same key, same effect: lets the control plane retry safely.
    pub idempotency_key: String,
    /// The tenant the operation is about, when it has one.
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
    /// When the control plane created it (Unix seconds).
    pub issued_at: i64,
    /// After this (Unix seconds) the agent must refuse to run it.
    pub expires_at: i64,
    /// Protocol version of this envelope.
    pub protocol_version: u16,
    /// What to do.
    pub op: Operation,
}

impl OperationEnvelope {
    /// Refuses envelopes that are expired, issued implausibly far in the
    /// future, valid for too long, or of an unsupported version. `now` is Unix
    /// seconds on the agent's clock.
    ///
    /// # Errors
    /// [`ErrorCode::Expired`] or [`ErrorCode::UnsupportedVersion`].
    pub fn check_fresh(&self, now: i64) -> Result<(), AgentError> {
        check_header(self.protocol_version, self.issued_at, self.expires_at, now)
    }
}

fn check_header(version: u16, issued_at: i64, expires_at: i64, now: i64) -> Result<(), AgentError> {
    if !(MIN_SUPPORTED_VERSION..=PROTOCOL_VERSION).contains(&version) {
        return Err(AgentError::new(
            ErrorCode::UnsupportedVersion,
            format!("protocol version {version} is not supported"),
        ));
    }
    if now >= expires_at {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation has expired",
        ));
    }
    if issued_at > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation was issued in the future; check the clocks",
        ));
    }
    if expires_at.saturating_sub(issued_at) > MAX_ENVELOPE_LIFETIME_SECS {
        return Err(AgentError::new(
            ErrorCode::Expired,
            "the operation's validity window is too long",
        ));
    }
    Ok(())
}

// ----------------------------------------------------------------- results

/// What the node reports about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Agent build version.
    pub agent_version: String,
    /// Logical CPU cores.
    pub cpu_cores: u32,
    /// Total memory in bytes.
    pub memory_bytes: u64,
    /// Size of the filesystem holding tenant data, in bytes.
    pub disk_total_bytes: u64,
    /// Free space there, in bytes.
    pub disk_free_bytes: u64,
    /// Kernel release, e.g. `6.8.0-31-generic`.
    pub kernel: String,
    /// Docker Engine version, if reachable.
    #[serde(default)]
    pub docker_version: Option<String>,
    /// How the agent reaches Docker.
    #[serde(default)]
    pub docker_mode: Option<DockerMode>,
    /// cgroup v2 is in use.
    #[serde(default)]
    pub cgroup_v2: bool,
    /// seccomp is available.
    #[serde(default)]
    pub seccomp: bool,
    /// AppArmor is enabled.
    #[serde(default)]
    pub apparmor: bool,
    /// Unprivileged user namespaces work.
    #[serde(default)]
    pub user_namespaces: bool,
    /// How tenant storage limits are enforced on this node (`xfs`), or absent
    /// when the filesystem under the tenants directory cannot enforce them.
    #[serde(default)]
    pub storage_quota: Option<String>,
}

/// How the agent reaches Docker (ADR 0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerMode {
    /// A rootless daemon owned by the agent user. The production mode.
    Rootless,
    /// A root daemon reached through the `docker` group: root-equivalent;
    /// development only.
    Rootful,
}

/// The data an operation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output {
    /// Answer to [`Operation::Ping`].
    Pong {
        /// The nonce from the ping.
        nonce: u64,
    },
    /// Answer to [`Operation::ReportCapabilities`].
    Capabilities(Capabilities),
    /// Answer to [`Operation::SetStorageQuota`].
    StorageQuotaSet {
        /// How the node enforces it (`xfs`).
        backend: String,
    },
    /// Answer to [`Operation::SetUpTenant`].
    TenantReady {
        /// Names of the running containers, in start order.
        containers: Vec<String>,
        /// How storage limits are enforced (`xfs`), or `None` when they are not.
        storage_quota: Option<String>,
    },
    /// Answer to [`Operation::RestartService`].
    ServiceRestarted {
        /// The containers that were restarted.
        containers: Vec<String>,
    },
    /// Answer to [`Operation::SetPhpIni`].
    PhpIniSet {
        /// How many PHP containers loaded it (the file is kept either way; a
        /// stopped container reads it when it starts).
        reloaded: usize,
    },
    /// Answer to [`Operation::SetPhpVersions`].
    PhpVersionsSet {
        /// The PHP containers that run now, the main one first.
        containers: Vec<String>,
        /// The containers that were removed because no domain uses their version.
        removed: Vec<String>,
    },
    /// Answer to [`Operation::AttachDomain`].
    DomainAttached {
        /// Whether the running web server has it (it is kept either way; a web
        /// container that is stopped loads it when it starts).
        applied: bool,
    },
    /// Answer to [`Operation::DetachDomain`].
    DomainDetached {
        /// Whether the running web server dropped it.
        applied: bool,
    },
    /// Answer to [`Operation::KeepDomains`].
    DomainsKept {
        /// How many configurations were removed.
        removed: usize,
        /// Whether the running web server dropped them.
        applied: bool,
    },
    /// Answer to [`Operation::SetCustomCertificates`].
    CustomCertificatesSet {
        /// How many certificates the tenant has now.
        count: usize,
        /// Whether the web server has them (they are kept either way).
        applied: bool,
    },
    /// Answer to [`Operation::ReportCertificates`].
    Certificates {
        /// The certificates, by name (at most [`MAX_CERTIFICATES_PER_REPORT`]).
        items: Vec<CertificateInfo>,
    },
    /// Answer to [`Operation::SetSiteRoutes`].
    SiteRoutesSet {
        /// Sites the tenant has now.
        sites: usize,
        /// Names they answer for.
        hosts: usize,
        /// Whether the web server has them (the routes are kept either way and
        /// applied as soon as it can take them).
        applied: bool,
    },
    /// Answer to [`Operation::SuspendTenant`].
    TenantSuspended {
        /// Names of the containers that are stopped, in the order they were stopped.
        containers: Vec<String>,
    },
    /// Answer to [`Operation::ResumeTenant`].
    TenantResumed {
        /// Names of the running containers, in start order.
        containers: Vec<String>,
    },
    /// Answer to [`Operation::DeleteTenant`].
    TenantDeleted {
        /// Whether the files were deleted too.
        purged: bool,
    },
    /// Answer to [`Operation::SetResourceLimits`].
    ResourceLimitsSet {
        /// Names of the containers that were updated.
        containers: Vec<String>,
    },
    /// Answer to [`Operation::ProvisionTenantStack`].
    StackProvisioned {
        /// Names of the running containers, in start order.
        containers: Vec<String>,
    },
    /// Answer to [`Operation::CreateTenant`].
    TenantCreated {
        /// The tenant's directory on the node.
        root: String,
        /// Name of its private network.
        network: String,
        /// Name of its files volume.
        volume: String,
    },
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Done.
    Succeeded,
    /// Failed for good.
    Failed,
    /// Failed, but trying again may work.
    Retryable,
}

/// The agent's answer to one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResult {
    /// The operation this answers.
    pub op_id: Uuid,
    /// How it ended.
    pub status: ResultStatus,
    /// What it produced, on success.
    #[serde(default)]
    pub output: Option<Output>,
    /// Why it failed, otherwise.
    #[serde(default)]
    pub error: Option<AgentError>,
}

impl OperationResult {
    /// A successful result.
    #[must_use]
    pub fn success(op_id: Uuid, output: Output) -> Self {
        Self {
            op_id,
            status: ResultStatus::Succeeded,
            output: Some(output),
            error: None,
        }
    }

    /// A failed result; the status follows [`AgentError::retryable`].
    #[must_use]
    pub fn failure(op_id: Uuid, error: AgentError) -> Self {
        Self {
            op_id,
            status: if error.retryable {
                ResultStatus::Retryable
            } else {
                ResultStatus::Failed
            },
            output: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------- messages

/// First message from the agent after the TLS handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Highest protocol version the agent speaks.
    pub protocol_version: u16,
    /// Agent build version.
    pub agent_version: String,
}

/// The control plane's answer to [`Hello`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    /// The version both sides will use (see [`negotiate_version`]).
    pub protocol_version: u16,
    /// The control plane's clock (Unix seconds), so the agent can warn about drift.
    pub server_time: i64,
}

/// Frames sent by the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    /// Opens the session.
    Hello(Hello),
    /// Answers an operation.
    Result(OperationResult),
}

/// Frames sent by the control plane. Operation frames are produced with
/// [`encode_operation`] and read with [`parse_operation_frame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMessage {
    /// Accepts the session.
    Welcome(Welcome),
    /// Asks the agent to run an operation.
    Operation(OperationEnvelope),
}

// -------------------------------------------------------------- enrollment

/// Path of the enrollment endpoint on the control plane.
pub const ENROLL_PATH: &str = "/api/v1/public/nodes/enroll";

/// What an agent sends to enrol (plain HTTPS, before it has a certificate).
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollRequest {
    /// The one-time token from the "add node" screen.
    #[cfg_attr(feature = "openapi", schema(example = "dhn_…"))]
    pub token: String,
    /// PEM certificate signing request made with a fresh ECDSA P-256 key that
    /// never leaves the node.
    pub csr_pem: String,
    /// Host name of the server.
    #[serde(default)]
    pub hostname: Option<String>,
    /// Agent build version.
    #[serde(default)]
    pub agent_version: Option<String>,
}

impl std::fmt::Debug for EnrollRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token or the request body.
        f.debug_struct("EnrollRequest")
            .field("hostname", &self.hostname)
            .field("agent_version", &self.agent_version)
            .finish_non_exhaustive()
    }
}

/// The node's new identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EnrollResponse {
    /// The node's id.
    pub node_id: Uuid,
    /// The node's name.
    pub name: String,
    /// The node certificate (PEM): client authentication for the mTLS connection.
    pub certificate_pem: String,
    /// The CA certificate (PEM) that signed it and that signs the control
    /// plane's server certificate.
    pub ca_certificate_pem: String,
    /// SHA-256 fingerprint of the CA certificate (lowercase hex), to compare
    /// with the one in the enrol command.
    pub ca_fingerprint_sha256: String,
    /// Certificate validity start (RFC 3339, UTC).
    #[cfg_attr(feature = "openapi", schema(format = DateTime))]
    pub not_before: String,
    /// Certificate expiry (RFC 3339, UTC). The agent renews before this.
    #[cfg_attr(feature = "openapi", schema(format = DateTime))]
    pub not_after: String,
    /// Protocol version the control plane speaks.
    pub protocol_version: u16,
    /// Where the agent opens its long-lived mutual-TLS connection
    /// (`https://host:port`), when the control plane has the agent port switched on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
}

// ----------------------------------------------------------------- framing

/// Why a frame could not be produced or read.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The frame is larger than [`MAX_FRAME_BYTES`].
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge(usize),
    /// Not valid JSON, or not a message of the expected shape.
    #[error("malformed frame: {0}")]
    Malformed(String),
}

/// Serialises a message as one frame.
///
/// # Errors
/// [`FrameError::TooLarge`] or [`FrameError::Malformed`].
pub fn encode<T: Serialize>(message: &T) -> Result<String, FrameError> {
    let text = serde_json::to_string(message).map_err(|e| FrameError::Malformed(e.to_string()))?;
    if text.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(text.len()));
    }
    Ok(text)
}

/// Reads one frame. The size is checked before any parsing.
///
/// # Errors
/// [`FrameError::TooLarge`] or [`FrameError::Malformed`].
pub fn decode<T: for<'de> Deserialize<'de>>(frame: &str) -> Result<T, FrameError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(frame.len()));
    }
    serde_json::from_str(frame).map_err(|e| FrameError::Malformed(e.to_string()))
}

/// Frame for [`ControlMessage::Operation`].
///
/// # Errors
/// Same as [`encode`].
pub fn encode_operation(envelope: &OperationEnvelope) -> Result<String, FrameError> {
    #[derive(Serialize)]
    #[serde(tag = "type", rename = "operation")]
    struct Frame<'a> {
        #[serde(flatten)]
        envelope: &'a OperationEnvelope,
    }
    encode(&Frame { envelope })
}

/// The part of an operation frame every version understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeHeader {
    /// The operations-ledger id.
    pub op_id: Uuid,
    /// Idempotency key.
    pub idempotency_key: String,
    /// Tenant the operation is about.
    pub tenant_id: Option<Uuid>,
    /// Creation time (Unix seconds).
    pub issued_at: i64,
    /// Expiry (Unix seconds).
    pub expires_at: i64,
    /// Protocol version.
    pub protocol_version: u16,
}

impl EnvelopeHeader {
    /// Same checks as [`OperationEnvelope::check_fresh`].
    ///
    /// # Errors
    /// [`ErrorCode::Expired`] or [`ErrorCode::UnsupportedVersion`].
    pub fn check_fresh(&self, now: i64) -> Result<(), AgentError> {
        check_header(self.protocol_version, self.issued_at, self.expires_at, now)
    }
}

/// The result of reading an operation frame on the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// A known, well-formed operation.
    Known(OperationEnvelope),
    /// A well-formed envelope carrying an operation this agent does not know.
    /// Answer it with [`ErrorCode::UnsupportedOperation`].
    Unsupported {
        /// The envelope header, to answer the right `op_id`.
        header: EnvelopeHeader,
        /// The unknown operation's wire name.
        op_type: String,
    },
    /// A known operation with invalid arguments. Answer it with
    /// [`ErrorCode::InvalidArguments`].
    InvalidArguments {
        /// The envelope header, to answer the right `op_id`.
        header: EnvelopeHeader,
        /// What is wrong.
        message: String,
    },
}

/// Reads an operation frame without letting an unknown operation kill the
/// session: the header is parsed first, the operation second.
///
/// # Errors
/// [`FrameError`] when the frame is not an operation envelope at all (no
/// usable header), in which case there is no `op_id` to answer.
pub fn parse_operation_frame(frame: &str) -> Result<Parsed, FrameError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(frame.len()));
    }
    #[derive(Deserialize)]
    struct Raw {
        #[serde(rename = "type")]
        kind: String,
        op_id: Uuid,
        idempotency_key: String,
        #[serde(default)]
        tenant_id: Option<Uuid>,
        issued_at: i64,
        expires_at: i64,
        protocol_version: u16,
        op: serde_json::Value,
    }
    let raw: Raw = serde_json::from_str(frame).map_err(|e| FrameError::Malformed(e.to_string()))?;
    if raw.kind != "operation" {
        return Err(FrameError::Malformed(format!(
            "expected an operation frame, got `{}`",
            raw.kind
        )));
    }
    let header = EnvelopeHeader {
        op_id: raw.op_id,
        idempotency_key: raw.idempotency_key,
        tenant_id: raw.tenant_id,
        issued_at: raw.issued_at,
        expires_at: raw.expires_at,
        protocol_version: raw.protocol_version,
    };
    let op_type = raw
        .op
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| FrameError::Malformed("operation has no `type`".to_owned()))?;
    match serde_json::from_value::<Operation>(raw.op) {
        Ok(op) => Ok(Parsed::Known(OperationEnvelope {
            op_id: header.op_id,
            idempotency_key: header.idempotency_key,
            tenant_id: header.tenant_id,
            issued_at: header.issued_at,
            expires_at: header.expires_at,
            protocol_version: header.protocol_version,
            op,
        })),
        Err(_) if !Operation::TYPE_NAMES.contains(&op_type.as_str()) => {
            Ok(Parsed::Unsupported { header, op_type })
        }
        Err(e) => Ok(Parsed::InvalidArguments {
            header,
            message: e.to_string(),
        }),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID: &str = "0190b6a2-0000-7000-8000-000000000001";

    fn id() -> Uuid {
        ID.parse().unwrap()
    }

    fn envelope(op: Operation) -> OperationEnvelope {
        OperationEnvelope {
            op_id: id(),
            idempotency_key: "tenant-1:create".into(),
            tenant_id: Some("0190b6a2-0000-7000-8000-0000000000aa".parse().unwrap()),
            issued_at: 1_000,
            expires_at: 1_300,
            protocol_version: PROTOCOL_VERSION,
            op,
        }
    }

    fn sample_capabilities() -> Capabilities {
        Capabilities {
            agent_version: "0.1.0".into(),
            cpu_cores: 4,
            memory_bytes: 8 << 30,
            disk_total_bytes: 80 << 30,
            disk_free_bytes: 40 << 30,
            kernel: "6.8.0-31-generic".into(),
            docker_version: Some("29.9.3".into()),
            docker_mode: Some(DockerMode::Rootless),
            cgroup_v2: true,
            seccomp: true,
            apparmor: true,
            user_namespaces: true,
            storage_quota: Some("xfs".into()),
        }
    }

    #[test]
    fn every_message_survives_a_round_trip() {
        for op in [Operation::Ping { nonce: 7 }, Operation::ReportCapabilities] {
            let e = envelope(op);
            let frame = encode_operation(&e).unwrap();
            assert_eq!(
                parse_operation_frame(&frame).unwrap(),
                Parsed::Known(e.clone())
            );
            // The generic path agrees with the dedicated one.
            let generic: ControlMessage = decode(&frame).unwrap();
            assert_eq!(generic, ControlMessage::Operation(e));
        }
        let results = [
            OperationResult::success(id(), Output::Pong { nonce: 7 }),
            OperationResult::success(id(), Output::Capabilities(sample_capabilities())),
            OperationResult::failure(id(), AgentError::new(ErrorCode::Internal, "boom")),
            OperationResult::failure(
                id(),
                AgentError::new(ErrorCode::Internal, "try later").retryable(),
            ),
        ];
        for r in results {
            let msg = AgentMessage::Result(r);
            assert_eq!(decode::<AgentMessage>(&encode(&msg).unwrap()).unwrap(), msg);
        }
        let hello = AgentMessage::Hello(Hello {
            protocol_version: 1,
            agent_version: "0.1.0".into(),
        });
        assert_eq!(
            decode::<AgentMessage>(&encode(&hello).unwrap()).unwrap(),
            hello
        );
        let welcome = ControlMessage::Welcome(Welcome {
            protocol_version: 1,
            server_time: 1_234,
        });
        assert_eq!(
            decode::<ControlMessage>(&encode(&welcome).unwrap()).unwrap(),
            welcome
        );
    }

    #[test]
    fn the_wire_format_is_pinned() {
        // These strings are the contract with deployed agents: change them only
        // together with PROTOCOL_VERSION or as a purely additive extension.
        let frame = encode_operation(&envelope(Operation::Ping { nonce: 7 })).unwrap();
        assert_eq!(
            frame,
            format!(
                r#"{{"type":"operation","op_id":"{ID}","idempotency_key":"tenant-1:create","tenant_id":"0190b6a2-0000-7000-8000-0000000000aa","issued_at":1000,"expires_at":1300,"protocol_version":1,"op":{{"type":"ping","nonce":7}}}}"#
            )
        );
        assert_eq!(
            encode(&AgentMessage::Hello(Hello {
                protocol_version: 1,
                agent_version: "0.1.0".into()
            }))
            .unwrap(),
            r#"{"type":"hello","protocol_version":1,"agent_version":"0.1.0"}"#
        );
        assert_eq!(
            encode(&AgentMessage::Result(OperationResult::success(
                id(),
                Output::Pong { nonce: 7 }
            )))
            .unwrap(),
            format!(
                r#"{{"type":"result","op_id":"{ID}","status":"succeeded","output":{{"type":"pong","nonce":7}},"error":null}}"#
            )
        );
        assert_eq!(
            encode(&AgentMessage::Result(OperationResult::failure(
                id(),
                AgentError::new(ErrorCode::UnsupportedOperation, "no such operation")
            )))
            .unwrap(),
            format!(
                r#"{{"type":"result","op_id":"{ID}","status":"failed","output":null,"error":{{"code":"unsupported_operation","message":"no such operation","retryable":false}}}}"#
            )
        );
    }

    #[test]
    fn the_enrollment_wire_format_is_pinned() {
        let request = EnrollRequest {
            token: "dhn_x".into(),
            csr_pem: "csr".into(),
            hostname: Some("h".into()),
            agent_version: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"token":"dhn_x","csr_pem":"csr","hostname":"h","agent_version":null}"#
        );
        // Older agents may omit the optional fields.
        let minimal: EnrollRequest =
            serde_json::from_str(r#"{"token":"t","csr_pem":"c"}"#).unwrap();
        assert_eq!((minimal.hostname, minimal.agent_version), (None, None));
        let dbg = format!("{request:?}");
        assert!(
            dbg.contains("hostname") && !dbg.contains("dhn_x") && !dbg.contains("csr"),
            "{dbg}"
        );

        let response = EnrollResponse {
            node_id: id(),
            name: "n1".into(),
            certificate_pem: "cert".into(),
            ca_certificate_pem: "ca".into(),
            ca_fingerprint_sha256: "ab".into(),
            not_before: "2026-01-01T00:00:00Z".into(),
            not_after: "2026-04-01T00:00:00Z".into(),
            protocol_version: 1,
            stream_url: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            format!(
                r#"{{"node_id":"{ID}","name":"n1","certificate_pem":"cert","ca_certificate_pem":"ca","ca_fingerprint_sha256":"ab","not_before":"2026-01-01T00:00:00Z","not_after":"2026-04-01T00:00:00Z","protocol_version":1}}"#
            )
        );
        // An older control plane omits the field; it reads back as absent.
        let with_url = EnrollResponse {
            stream_url: Some("https://panel.example.com:8444".into()),
            ..response.clone()
        };
        let json_with_url = serde_json::to_string(&with_url).unwrap();
        assert!(json_with_url.ends_with(r#""stream_url":"https://panel.example.com:8444"}"#));
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&json_with_url).unwrap(),
            with_url
        );
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&json).unwrap(),
            response
        );
        // A newer control plane may add fields.
        let newer = json.replacen('{', r#"{"future":true,"#, 1);
        assert_eq!(
            serde_json::from_str::<EnrollResponse>(&newer).unwrap(),
            response
        );
        assert_eq!(ENROLL_PATH, "/api/v1/public/nodes/enroll");
    }

    #[test]
    fn unknown_operations_do_not_break_the_session() {
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k",
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "teleport_tenant", "tenant": "x", "plan": {"cpus": 2}}
        })
        .to_string();
        match parse_operation_frame(&frame).unwrap() {
            Parsed::Unsupported { header, op_type } => {
                assert_eq!(op_type, "teleport_tenant");
                assert_eq!(header.op_id, id());
                assert_eq!(header.tenant_id, None);
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn known_operations_with_bad_arguments_are_reported_as_such() {
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k",
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "ping", "nonce": "not a number"}
        })
        .to_string();
        match parse_operation_frame(&frame).unwrap() {
            Parsed::InvalidArguments { header, message } => {
                assert_eq!(header.op_id, id());
                assert!(
                    message.contains("nonce") || message.contains("invalid type"),
                    "{message}"
                );
            }
            other => panic!("expected InvalidArguments, got {other:?}"),
        }
    }

    #[test]
    fn newer_peers_may_add_fields_everywhere() {
        // Extra fields on envelope, operation, hello and capabilities are ignored.
        let frame = json!({
            "type": "operation", "op_id": ID, "idempotency_key": "k", "future_field": [1, 2],
            "issued_at": 1000, "expires_at": 1300, "protocol_version": 1,
            "op": {"type": "ping", "nonce": 1, "trace": "abc"}
        })
        .to_string();
        assert!(matches!(
            parse_operation_frame(&frame).unwrap(),
            Parsed::Known(_)
        ));
        let hello =
            r#"{"type":"hello","protocol_version":1,"agent_version":"9.9","features":["x"]}"#;
        assert!(decode::<AgentMessage>(hello).is_ok());
        // Capabilities written by an older agent (without the optional fields) still parse.
        let old = r#"{"agent_version":"0.0.1","cpu_cores":2,"memory_bytes":1,"disk_total_bytes":2,"disk_free_bytes":1,"kernel":"6.8"}"#;
        let c: Capabilities = serde_json::from_str(old).unwrap();
        assert_eq!(
            (c.docker_version, c.docker_mode, c.cgroup_v2),
            (None, None, false)
        );
    }

    #[test]
    fn frames_without_a_usable_header_are_errors_not_unsupported() {
        for bad in [
            "not json",
            "{}",
            r#"{"type":"welcome","protocol_version":1,"server_time":1}"#,
            &json!({"type":"operation","op_id":"nope","idempotency_key":"k","issued_at":1,"expires_at":2,"protocol_version":1,"op":{"type":"ping","nonce":1}}).to_string(),
            &json!({"type":"operation","op_id":ID,"idempotency_key":"k","issued_at":1,"expires_at":2,"protocol_version":1,"op":{"nonce":1}}).to_string(),
        ] {
            assert!(matches!(parse_operation_frame(bad), Err(FrameError::Malformed(_))), "{bad}");
        }
    }

    #[test]
    fn oversized_frames_are_refused_before_parsing() {
        let big = "x".repeat(MAX_FRAME_BYTES + 1);
        assert!(matches!(
            decode::<AgentMessage>(&big),
            Err(FrameError::TooLarge(_))
        ));
        assert!(matches!(
            parse_operation_frame(&big),
            Err(FrameError::TooLarge(_))
        ));
        let hello = AgentMessage::Hello(Hello {
            protocol_version: 1,
            agent_version: "y".repeat(MAX_FRAME_BYTES),
        });
        assert!(matches!(encode(&hello), Err(FrameError::TooLarge(_))));
    }

    #[test]
    fn envelopes_are_accepted_only_inside_their_validity_window() {
        let e = envelope(Operation::ReportCapabilities); // valid 1000..1300
        assert!(e.check_fresh(1_000).is_ok());
        assert!(e.check_fresh(1_299).is_ok());
        assert_eq!(e.check_fresh(1_300).unwrap_err().code, ErrorCode::Expired);
        assert_eq!(e.check_fresh(9_999).unwrap_err().code, ErrorCode::Expired);
        // Issued far in the future (agent clock is behind, or the envelope is forged).
        assert_eq!(
            e.check_fresh(1_000 - MAX_CLOCK_SKEW_SECS - 1)
                .unwrap_err()
                .code,
            ErrorCode::Expired
        );
        assert!(e.check_fresh(1_000 - MAX_CLOCK_SKEW_SECS).is_ok());
        // A window longer than allowed is a replay risk.
        let mut long = e.clone();
        long.expires_at = long.issued_at + MAX_ENVELOPE_LIFETIME_SECS + 1;
        assert_eq!(
            long.check_fresh(1_000).unwrap_err().code,
            ErrorCode::Expired
        );
        // Unsupported versions.
        for v in [0, PROTOCOL_VERSION + 1, u16::MAX] {
            let mut bad = e.clone();
            bad.protocol_version = v;
            assert_eq!(
                bad.check_fresh(1_000).unwrap_err().code,
                ErrorCode::UnsupportedVersion,
                "{v}"
            );
        }
        // Extreme values do not overflow.
        let mut extreme = e;
        extreme.issued_at = i64::MIN;
        extreme.expires_at = i64::MAX;
        assert!(extreme.check_fresh(i64::MAX - 1).is_err());
    }

    #[test]
    fn versions_are_negotiated_down_never_up() {
        assert_eq!(negotiate_version(PROTOCOL_VERSION), Some(PROTOCOL_VERSION));
        assert_eq!(
            negotiate_version(PROTOCOL_VERSION + 5),
            Some(PROTOCOL_VERSION)
        );
        assert_eq!(
            negotiate_version(MIN_SUPPORTED_VERSION),
            Some(MIN_SUPPORTED_VERSION)
        );
        assert_eq!(
            negotiate_version(MIN_SUPPORTED_VERSION.wrapping_sub(1)),
            if MIN_SUPPORTED_VERSION == 0 {
                Some(PROTOCOL_VERSION)
            } else {
                None
            }
        );
    }

    #[test]
    fn operation_names_match_the_wire_and_the_list() {
        for op in [
            Operation::Ping { nonce: 1 },
            Operation::ReportCapabilities,
            Operation::CreateTenant {
                tenant_id: id(),
                username: "alice".into(),
                uid: 10_000,
            },
            Operation::SetStorageQuota {
                tenant_id: id(),
                uid: 10_000,
                disk_bytes: Some(1 << 30),
                inodes: None,
            },
            Operation::ProvisionTenantStack {
                tenant_id: id(),
                username: "alice".into(),
                uid: 10_000,
                cpu_millis: Some(1000),
                memory_bytes: Some(512 << 20),
                pids: None,
            },
            Operation::SetResourceLimits {
                username: "alice".into(),
                cpu_millis: None,
                memory_bytes: Some(1 << 30),
                pids: Some(300),
            },
            Operation::SetUpTenant {
                tenant_id: id(),
                username: "alice".into(),
                uid: 10_000,
                disk_bytes: Some(1 << 30),
                inodes: None,
                cpu_millis: Some(500),
                memory_bytes: Some(256 << 20),
                pids: Some(100),
                skeleton: Vec::new(),
            },
            Operation::ReportCertificates,
            Operation::SetCustomCertificates {
                username: "alice".into(),
                items: vec![],
            },
            Operation::SetSiteRoutes {
                username: "alice".into(),
                routes: vec![SiteRoute {
                    hosts: vec!["example.com".into()],
                    upstream: Some("dh-alice-web:8080".into()),
                    redirects: vec![],
                    force_https: false,
                    hsts: false,
                }],
            },
            Operation::SuspendTenant {
                username: "alice".into(),
            },
            Operation::ResumeTenant {
                username: "alice".into(),
            },
            Operation::SetPhpVersions {
                tenant_id: id(),
                username: "alice".into(),
                uid: 10_000,
                cpu_millis: None,
                memory_bytes: None,
                pids: None,
                php_version: "8.4".into(),
                extra_php: vec!["8.3".into()],
            },
            Operation::SetPhpIni {
                username: "alice".into(),
                ini: "memory_limit = 64M\n".into(),
            },
            Operation::RestartService {
                username: "alice".into(),
                service: "web".into(),
            },
            Operation::AttachDomain {
                username: "alice".into(),
                domain: "example.com".into(),
                config: "server {}".into(),
            },
            Operation::DetachDomain {
                username: "alice".into(),
                domain: "example.com".into(),
            },
            Operation::KeepDomains {
                username: "alice".into(),
                domains: vec!["example.com".into()],
            },
            Operation::DeleteTenant {
                tenant_id: id(),
                username: "alice".into(),
                uid: 10_000,
                purge_data: true,
            },
        ] {
            let wire = serde_json::to_value(&op).unwrap();
            assert_eq!(wire["type"], op.type_name());
            assert!(Operation::TYPE_NAMES.contains(&op.type_name()));
        }
        assert_eq!(
            Operation::TYPE_NAMES.len(),
            19,
            "update TYPE_NAMES and type_name() together with the enum"
        );
    }

    #[test]
    fn skeleton_files_are_checked_for_paths_sizes_and_clashes() {
        let f = |p: &str, c: &str| SkeletonFile {
            path: p.into(),
            content: c.into(),
        };
        assert!(
            check_skeleton(&[f("index.html", "x"), f("css/a.css", ""), f(".htaccess", "")]).is_ok()
        );
        for bad in [
            "", "/a", "../a", "a/../b", "a//b", ".", "a/", "a b", "a\\b", "é",
        ] {
            assert!(check_skeleton(&[f(bad, "x")]).is_err(), "{bad:?}");
        }
        assert!(check_skeleton(&[f("a", "1"), f("a", "2")]).is_err());
        assert!(check_skeleton(&[f("a", "1"), f("a/b", "2")]).is_err());
        assert!(check_skeleton(&[f("a/b", "1"), f("a", "2")]).is_err());
        assert!(check_skeleton(&[f("a", "1"), f("ab/c", "2")]).is_ok());
        assert!(check_skeleton(&[f("a", &"x".repeat(SKELETON_MAX_FILE_BYTES + 1))]).is_err());
        assert!(check_skeleton(&[f("a", "x\0")]).is_err());
        let heavy: Vec<_> = (0..3)
            .map(|i| f(&format!("f{i}"), &"x".repeat(SKELETON_MAX_FILE_BYTES)))
            .collect();
        assert!(check_skeleton(&heavy).is_err());
        // The shipped default has to fit in one operation, with room to spare.
        let worst = Operation::SetUpTenant {
            tenant_id: Uuid::nil(),
            username: "x".repeat(32),
            uid: 59_999,
            disk_bytes: Some(u64::MAX),
            inodes: Some(u64::MAX),
            cpu_millis: Some(u32::MAX),
            memory_bytes: Some(u64::MAX),
            pids: Some(u32::MAX),
            skeleton: (0..SKELETON_MAX_FILES)
                .map(|i| {
                    f(
                        &format!("{i:02}{}", "n".repeat(60)),
                        &"\"".repeat(SKELETON_MAX_TOTAL_BYTES / SKELETON_MAX_FILES),
                    )
                })
                .collect(),
        };
        // Quotes double in JSON: the worst case still fits the ledger (64 KiB) and the helper (64 KiB).
        assert!(serde_json::to_string(&worst).unwrap().len() < 56 * 1024);
    }

    #[test]
    fn a_private_key_is_never_shown_and_the_ledger_form_has_none() {
        let with_key = CustomCertificate {
            host: "example.com".into(),
            certificate: "CERT".into(),
            private_key: Some(
                "-----BEGIN PRIVATE KEY-----\nSECRET\n-----END PRIVATE KEY-----".into(),
            ),
        };
        let op = Operation::SetCustomCertificates {
            username: "alice".into(),
            items: vec![with_key.clone()],
        };
        assert!(!format!("{op:?}").contains("SECRET"), "{op:?}");
        assert!(format!("{op:?}").contains("has_key: true"));
        // On the wire the key is there; without one the field is left out altogether.
        assert!(serde_json::to_string(&op).unwrap().contains("SECRET"));
        let bare = CustomCertificate {
            private_key: None,
            ..with_key
        };
        let json = serde_json::to_value(&bare).unwrap();
        assert!(json.get("private_key").is_none(), "{json}");
        assert_eq!(
            serde_json::from_value::<CustomCertificate>(json).unwrap(),
            bare
        );
    }

    #[test]
    fn failures_map_to_the_right_status() {
        let id = id();
        let retry =
            OperationResult::failure(id, AgentError::new(ErrorCode::Internal, "x").retryable());
        assert_eq!(retry.status, ResultStatus::Retryable);
        let fail = OperationResult::failure(id, AgentError::new(ErrorCode::InvalidArguments, "x"));
        assert_eq!(fail.status, ResultStatus::Failed);
        assert!(fail.output.is_none() && fail.error.is_some());
        let ok = OperationResult::success(id, Output::Pong { nonce: 1 });
        assert!(ok.error.is_none() && ok.output.is_some());
        assert_eq!(
            AgentError::new(ErrorCode::Expired, "late").to_string(),
            "Expired: late"
        );
    }
}
