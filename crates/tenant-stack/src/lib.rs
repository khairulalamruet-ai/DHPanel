//! The stack that runs for one tenant (T04.06).
//!
//! A *stack template* says which containers make a tenant's site, how the
//! plan's resources are divided between them and what they mount. Rendering it
//! for a tenant ([`render`]) gives ready container specifications for the
//! docker-manager crate, which adds its own hardening on top (all capabilities
//! dropped, no new privileges, read-only root, limits, no host mounts).
//!
//! Three rules are enforced here, and checked by the security checklist in the
//! tests:
//! * **Only allowed images.** Every image comes from the allow-list
//!   (`deploy/stack/images.toml`): pinned by digest, listed by a reviewed change.
//! * **Never root.** Every container runs as the tenant's own numeric id, which
//!   is always 10000 or higher.
//! * **Nothing shared.** Names, the network and the volumes are the tenant's
//!   own (`dh-<username>-...`); no container publishes a port, mounts a host
//!   path, or joins another tenant's network.

use std::{collections::BTreeMap, fmt};

use dhpanel_docker_manager::{
    Capability, ContainerSpec, ImageRef, Name, Network, Restart, SpecError, Tmpfs, VolumeMount,
};
use dhpanel_domain::{TenantUsername, Uid};
use serde::Deserialize;
use uuid::Uuid;

/// The embedded allow-list.
pub const EMBEDDED_ALLOWLIST: &str = include_str!("../../../deploy/stack/images.toml");

/// What an unlimited plan resource is capped to for the stack as a whole: a
/// tenant that is "unlimited" is still not allowed to exhaust a node.
pub const UNLIMITED_CPU_MILLIS: u32 = 4_000;
/// See [`UNLIMITED_CPU_MILLIS`].
pub const UNLIMITED_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// See [`UNLIMITED_CPU_MILLIS`].
pub const UNLIMITED_PIDS: u32 = 1_024;

/// The part a container plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The web server in front.
    Web,
    /// PHP-FPM behind it.
    Php,
}

impl Role {
    /// The name used in labels and in the allow-list.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Php => "php",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a stack could not be rendered or an allow-list not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StackError {
    /// The allow-list text is not valid.
    #[error("image allow-list: {0}")]
    Allowlist(String),
    /// The allow-list has no image for a role the template needs.
    #[error("no allowed image for the `{0}` role")]
    NoImage(Role),
    /// An image that is not in the allow-list was asked for.
    #[error("the image for `{0}` is not on the allow-list")]
    ImageNotAllowed(Role),
    /// The plan leaves too little of a resource to run the stack.
    #[error("the plan's {0} is too small to run a stack")]
    PlanTooSmall(&'static str),
    /// A rendered specification is not valid.
    #[error(transparent)]
    Spec(#[from] SpecError),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAllowlist {
    image: Vec<RawImage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawImage {
    role: Role,
    reference: String,
    #[serde(default)]
    note: String,
}

/// The images that may run, by role, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowlist {
    by_role: BTreeMap<Role, Vec<ImageRef>>,
}

impl Allowlist {
    /// Reads an allow-list. Every reference must be pinned by digest.
    ///
    /// # Errors
    /// [`StackError::Allowlist`] for TOML errors, tags, duplicates or a missing role.
    pub fn parse(text: &str) -> Result<Self, StackError> {
        let raw: RawAllowlist = toml::from_str(text)
            .map_err(|e| StackError::Allowlist(e.to_string().replace('\n', " ")))?;
        let mut by_role: BTreeMap<Role, Vec<ImageRef>> = BTreeMap::new();
        for image in raw.image {
            let reference = ImageRef::parse(&image.reference).map_err(|e| {
                StackError::Allowlist(format!(
                    "{} ({}): {e}",
                    image.role,
                    image.note.chars().take(40).collect::<String>()
                ))
            })?;
            let list = by_role.entry(image.role).or_default();
            if list.contains(&reference) {
                return Err(StackError::Allowlist(format!(
                    "{} lists the same image twice",
                    image.role
                )));
            }
            list.push(reference);
        }
        for role in [Role::Web, Role::Php] {
            if !by_role.contains_key(&role) {
                return Err(StackError::Allowlist(format!(
                    "no image for the `{role}` role"
                )));
            }
        }
        Ok(Self { by_role })
    }

    /// The list shipped with this build.
    ///
    /// # Errors
    /// [`StackError::Allowlist`] (a build mistake; tested).
    pub fn embedded() -> Result<Self, StackError> {
        Self::parse(EMBEDDED_ALLOWLIST)
    }

    /// The image new stacks use for a role.
    ///
    /// # Errors
    /// [`StackError::NoImage`].
    pub fn current(&self, role: Role) -> Result<&ImageRef, StackError> {
        self.by_role
            .get(&role)
            .and_then(|list| list.first())
            .ok_or(StackError::NoImage(role))
    }

    /// Whether `image` may run in `role`.
    #[must_use]
    pub fn allows(&self, role: Role, image: &ImageRef) -> bool {
        self.by_role
            .get(&role)
            .is_some_and(|list| list.contains(image))
    }
}

/// What the plan gives the whole stack. `None` means unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanResources {
    /// CPU in thousandths of a core.
    pub cpu_millis: Option<u32>,
    /// Memory in bytes.
    pub memory_bytes: Option<u64>,
    /// Processes.
    pub pids: Option<u32>,
}

/// The tenant a stack is rendered for.
#[derive(Debug, Clone)]
pub struct Tenant<'a> {
    /// Its id.
    pub id: Uuid,
    /// Its system name.
    pub username: &'a TenantUsername,
    /// Its numeric id (also its group id).
    pub uid: Uid,
    /// What its plan allows.
    pub resources: PlanResources,
}

/// One container of the template.
struct ServiceTemplate {
    role: Role,
    /// Share of the plan's resources, in percent (the shares add up to 100).
    share: u32,
    min_memory_bytes: u64,
    min_pids: u32,
    command: &'static [&'static str],
    /// Memory-backed scratch directories: path and size.
    tmpfs: &'static [(&'static str, u64)],
    /// Where the files volume is mounted, and whether read-only.
    files_at: &'static str,
    files_read_only: bool,
}

const MIB: u64 = 1024 * 1024;

/// The web + PHP template. nginx is the unprivileged build (port 8080), so it
/// needs no capability; PHP-FPM listens on 9000 inside the tenant's network.
const WEB_PHP: &[ServiceTemplate] = &[
    ServiceTemplate {
        role: Role::Web,
        share: 25,
        min_memory_bytes: 32 * MIB,
        min_pids: 16,
        command: &[],
        tmpfs: &[
            ("/tmp", 16 * MIB),
            ("/var/cache/nginx", 64 * MIB),
            ("/run", MIB),
        ],
        files_at: "/var/www/html",
        files_read_only: true,
    },
    ServiceTemplate {
        role: Role::Php,
        share: 75,
        min_memory_bytes: 64 * MIB,
        min_pids: 32,
        command: &[],
        tmpfs: &[("/tmp", 64 * MIB), ("/run", MIB)],
        files_at: "/var/www/html",
        files_read_only: false,
    },
];

/// A rendered stack: the tenant's network, volumes and containers.
#[derive(Debug, Clone)]
pub struct Stack {
    /// The tenant's private network.
    pub network: Name,
    /// The tenant's volumes (the site files).
    pub volumes: Vec<Name>,
    /// Containers, in the order to start them (PHP before the web server).
    pub services: Vec<ContainerSpec>,
}

fn name(tenant: &TenantUsername, suffix: &str) -> Result<Name, SpecError> {
    Name::parse(&format!("dh-{tenant}-{suffix}"))
}

/// Name of a tenant's private network.
///
/// # Errors
/// [`SpecError`] (cannot happen for a valid tenant name).
pub fn network_name(tenant: &TenantUsername) -> Result<Name, SpecError> {
    name(tenant, "net")
}

/// Name of a tenant's files volume.
///
/// # Errors
/// [`SpecError`] (cannot happen for a valid tenant name).
pub fn files_volume_name(tenant: &TenantUsername) -> Result<Name, SpecError> {
    name(tenant, "www")
}

/// Renders the web + PHP stack for a tenant from the allow-list.
///
/// # Errors
/// [`StackError`]: no image for a role, a plan too small to run the stack, or a
/// specification the docker-manager crate refuses.
pub fn render(tenant: &Tenant<'_>, allowlist: &Allowlist) -> Result<Stack, StackError> {
    let cpu = tenant.resources.cpu_millis.unwrap_or(UNLIMITED_CPU_MILLIS);
    let memory = tenant
        .resources
        .memory_bytes
        .unwrap_or(UNLIMITED_MEMORY_BYTES);
    let pids = tenant.resources.pids.unwrap_or(UNLIMITED_PIDS);

    let network = network_name(tenant.username)?;
    let files = files_volume_name(tenant.username)?;
    let user = Some((tenant.uid.get(), tenant.uid.get()));
    let mut services = Vec::new();
    // PHP first, so the web server never starts without its backend.
    for template in WEB_PHP.iter().rev() {
        let image = allowlist.current(template.role)?.clone();
        if !allowlist.allows(template.role, &image) {
            return Err(StackError::ImageNotAllowed(template.role));
        }
        let share = u64::from(template.share);
        let memory_bytes = memory * share / 100;
        let pids_limit = u32::try_from(u64::from(pids) * share / 100).unwrap_or(u32::MAX);
        let cpu_millis = u32::try_from(u64::from(cpu) * share / 100).unwrap_or(u32::MAX);
        if memory_bytes < template.min_memory_bytes {
            return Err(StackError::PlanTooSmall("memory"));
        }
        if pids_limit < template.min_pids {
            return Err(StackError::PlanTooSmall("process limit"));
        }
        if cpu_millis < 10 {
            return Err(StackError::PlanTooSmall("CPU"));
        }
        let mut spec = ContainerSpec::new(name(tenant.username, template.role.as_str())?, image);
        spec.command = template.command.iter().map(|a| (*a).to_owned()).collect();
        spec.user = user;
        spec.memory_bytes = memory_bytes;
        spec.cpu_millis = cpu_millis;
        spec.pids_limit = pids_limit;
        spec.tmpfs = template
            .tmpfs
            .iter()
            .map(|(target, size)| Tmpfs {
                target: (*target).to_owned(),
                size_bytes: *size,
            })
            .collect();
        spec.volumes = vec![VolumeMount {
            volume: files.clone(),
            target: template.files_at.to_owned(),
            read_only: template.files_read_only,
        }];
        spec.network = Network::Named(network.clone());
        spec.restart = Restart::UnlessStopped;
        spec.cap_add = Vec::<Capability>::new();
        spec.tenant = Some(tenant.id);
        spec.role = Some(template.role.as_str().to_owned());
        spec.validate()?;
        services.push(spec);
    }
    Ok(Stack {
        network,
        volumes: vec![files],
        services,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use dhpanel_docker_manager::{MANAGED_LABEL, ROLE_LABEL, TENANT_LABEL};
    use dhpanel_domain::UidRange;

    use super::*;

    fn allowlist() -> Allowlist {
        Allowlist::embedded().unwrap()
    }

    fn uid(n: u32) -> Uid {
        UidRange::DEFAULT.allocate(10_000..10_000 + n).unwrap()
    }

    fn render_for(name: &str, n: u32, resources: PlanResources) -> Result<Stack, StackError> {
        let username = TenantUsername::parse(name).unwrap();
        render(
            &Tenant {
                id: Uuid::now_v7(),
                username: &username,
                uid: uid(n),
                resources,
            },
            &allowlist(),
        )
    }

    const BASIC: PlanResources = PlanResources {
        cpu_millis: Some(500),
        memory_bytes: Some(512 * MIB),
        pids: Some(128),
    };

    #[test]
    fn the_shipped_allowlist_is_valid_and_pinned() {
        let list = allowlist();
        for role in [Role::Web, Role::Php] {
            let image = list.current(role).unwrap();
            assert!(image.as_str().contains("@sha256:"), "{image:?}");
            assert!(list.allows(role, image));
        }
        assert!(
            !list.allows(Role::Web, list.current(Role::Php).unwrap()),
            "roles are separate"
        );
    }

    #[test]
    fn allowlists_refuse_tags_duplicates_unknown_keys_and_missing_roles() {
        let digest = "a".repeat(64);
        let good = |role: &str, reference: &str| {
            format!("[[image]]\nrole = \"{role}\"\nreference = \"{reference}\"\n")
        };
        let web = good("web", &format!("nginx@sha256:{digest}"));
        let php = good("php", &format!("php@sha256:{digest}"));
        assert!(Allowlist::parse(&format!("{web}{php}")).is_ok());
        for (text, needle) in [
            (format!("{}{php}", good("web", "nginx:latest")), "pinned"),
            (format!("{web}{web}{php}"), "twice"),
            (web.clone(), "no image for the `php`"),
            (
                format!(
                    "{web}{php}[[image]]\nrole = \"mail\"\nreference = \"x@sha256:{digest}\"\n"
                ),
                "mail",
            ),
            (
                format!(
                    "{web}{php}[[image]]\nrole = \"web\"\nreference = \"x@sha256:{digest}\"\nroot = true\n"
                ),
                "root",
            ),
            ("not toml [".to_owned(), "allow-list"),
            (String::new(), "image"),
        ] {
            let err = Allowlist::parse(&text).unwrap_err().to_string();
            assert!(err.contains(needle), "{needle}: {err}");
        }
    }

    /// The security checklist of the container template: everything the review
    /// of T04.06 looks for, as assertions over what is rendered.
    #[test]
    fn the_security_checklist_holds_for_every_plan_and_tenant() {
        let list = allowlist();
        let plans = [
            BASIC,
            PlanResources {
                cpu_millis: None,
                memory_bytes: None,
                pids: None,
            },
            PlanResources {
                cpu_millis: Some(10_000),
                memory_bytes: Some(16 * 1024 * MIB),
                pids: Some(4096),
            },
            PlanResources {
                cpu_millis: Some(100),
                memory_bytes: Some(192 * MIB),
                pids: Some(64),
            },
        ];
        for (n, resources) in plans.into_iter().enumerate() {
            let username = format!("tenant{n}");
            let stack = render_for(&username, n as u32, resources).unwrap();
            let owner = TenantUsername::parse(&username).unwrap();
            let prefix = format!("dh-{owner}-");
            // The tenant's own network and volume, named after the tenant.
            assert_eq!(stack.network.as_str(), format!("{prefix}net"));
            assert_eq!(stack.volumes.len(), 1);
            assert!(stack.volumes[0].as_str().starts_with(&prefix));
            assert_eq!(stack.services.len(), 2);
            let (mut memory, mut pids, mut cpu) = (0_u64, 0_u64, 0_u64);
            for spec in &stack.services {
                // 1. Never root: the tenant's own id, and that id is at least 10000.
                let (u, g) = spec.user.expect("a user is always set");
                assert_eq!(u, g);
                assert_eq!(u, 10_000 + n as u32);
                assert!(u >= 10_000);
                // 2. Only allowed images, pinned.
                let role = if spec.role.as_deref() == Some("web") {
                    Role::Web
                } else {
                    Role::Php
                };
                assert!(list.allows(role, &spec.image), "{:?}", spec.image);
                // 3. Nothing added back, nothing published, nothing from the host.
                assert!(spec.cap_add.is_empty(), "no capabilities");
                assert!(spec.ports.is_empty(), "no published ports");
                assert!(!spec.writable_root, "read-only root");
                assert!(
                    spec.env.is_empty(),
                    "no environment (and so no secrets) in the template"
                );
                // 4. Names, network and volumes are the tenant's own.
                assert!(
                    spec.name.as_str().starts_with(&prefix),
                    "{}",
                    spec.name.as_str()
                );
                assert_eq!(spec.network, Network::Named(stack.network.clone()));
                assert!(
                    spec.volumes
                        .iter()
                        .all(|v| v.volume.as_str().starts_with(&prefix))
                );
                assert!(spec.tmpfs.iter().all(|t| t.size_bytes <= 64 * MIB));
                // 5. Limits are set, and recorded as labels with the tenant.
                assert!(
                    spec.memory_bytes >= 32 * MIB && spec.pids_limit >= 16 && spec.cpu_millis >= 10
                );
                assert_eq!(spec.restart, Restart::UnlessStopped);
                memory += spec.memory_bytes;
                pids += u64::from(spec.pids_limit);
                cpu += u64::from(spec.cpu_millis);
                // 6. What docker-manager sends is the hardened request.
                let body = spec.to_create_body().unwrap();
                let host = body.host_config.unwrap();
                assert_eq!(host.cap_drop, Some(vec!["ALL".to_owned()]));
                assert_eq!(
                    host.security_opt,
                    Some(vec!["no-new-privileges:true".to_owned()])
                );
                assert_eq!(
                    (host.privileged, host.readonly_rootfs),
                    (Some(false), Some(true))
                );
                assert!(host.binds.is_none() && host.pid_mode.is_none() && host.devices.is_none());
                assert_eq!(body.user.as_deref(), Some(&*format!("{u}:{u}")));
                let labels = body.labels.unwrap();
                assert_eq!(labels[MANAGED_LABEL], "true");
                assert_eq!(labels[TENANT_LABEL], spec.tenant.unwrap().to_string());
                assert_eq!(labels[ROLE_LABEL], spec.role.clone().unwrap());
            }
            // The two containers together stay within what the plan gives.
            let cap = |value: Option<u64>, unlimited: u64| value.unwrap_or(unlimited);
            assert!(memory <= cap(resources.memory_bytes, UNLIMITED_MEMORY_BYTES));
            assert!(pids <= cap(resources.pids.map(u64::from), u64::from(UNLIMITED_PIDS)));
            assert!(
                cpu <= cap(
                    resources.cpu_millis.map(u64::from),
                    u64::from(UNLIMITED_CPU_MILLIS)
                )
            );
        }
    }

    #[test]
    fn php_starts_before_the_web_server_and_only_php_can_write_the_files() {
        let stack = render_for("alice", 0, BASIC).unwrap();
        let roles: Vec<&str> = stack
            .services
            .iter()
            .map(|s| s.role.as_deref().unwrap())
            .collect();
        assert_eq!(roles, ["php", "web"]);
        assert!(!stack.services[0].volumes[0].read_only);
        assert!(
            stack.services[1].volumes[0].read_only,
            "the web server only reads the site"
        );
        // Resources follow the 75 / 25 split.
        assert_eq!(stack.services[0].memory_bytes, 384 * MIB);
        assert_eq!(stack.services[1].memory_bytes, 128 * MIB);
        assert_eq!(
            (stack.services[0].pids_limit, stack.services[1].pids_limit),
            (96, 32)
        );
        assert_eq!(
            (stack.services[0].cpu_millis, stack.services[1].cpu_millis),
            (375, 125)
        );
    }

    #[test]
    fn a_plan_too_small_to_run_the_stack_is_refused() {
        let tiny_memory = PlanResources {
            memory_bytes: Some(64 * MIB),
            ..BASIC
        };
        assert_eq!(
            render_for("alice", 0, tiny_memory).unwrap_err(),
            StackError::PlanTooSmall("memory")
        );
        let tiny_pids = PlanResources {
            pids: Some(20),
            ..BASIC
        };
        assert_eq!(
            render_for("alice", 0, tiny_pids).unwrap_err(),
            StackError::PlanTooSmall("process limit")
        );
        let tiny_cpu = PlanResources {
            cpu_millis: Some(20),
            ..BASIC
        };
        assert_eq!(
            render_for("alice", 0, tiny_cpu).unwrap_err(),
            StackError::PlanTooSmall("CPU")
        );
    }

    #[test]
    fn two_tenants_share_nothing() {
        let a = render_for("alice", 0, BASIC).unwrap();
        let b = render_for("bobby", 1, BASIC).unwrap();
        assert_ne!(a.network, b.network);
        assert_ne!(a.volumes, b.volumes);
        let names = |s: &Stack| {
            s.services
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
        };
        assert!(names(&a).iter().all(|n| !names(&b).contains(n)));
        assert_ne!(a.services[0].user, b.services[0].user);
    }
}
