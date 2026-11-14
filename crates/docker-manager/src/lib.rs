//! The only way the node agent talks to Docker (T02.14).
//!
//! A deliberately small surface over the Docker Engine API:
//!
//! * pull an image **by digest**;
//! * create a container from a validated [`ContainerSpec`], always hardened
//!   (see [`ContainerSpec::to_create_body`]);
//! * start, stop, remove, inspect it and read its resource usage;
//! * create and remove named volumes and networks.
//!
//! There is **no exec**, no attach, no host bind mount, no privileged mode, no
//! host namespaces and no way to pass raw Docker options: such a request cannot
//! be expressed with these types. Every object created carries the
//! [`MANAGED_LABEL`], and every action first checks that label, so a container,
//! volume or network that this crate did not create is never touched.
//!
//! The daemon is the agent's own rootless Docker (ADR 0004), reached through
//! its Unix socket.

pub mod spec;

use std::{collections::HashMap, path::Path, time::Duration};

use bollard::{
    Docker,
    errors::Error as BollardError,
    exec::{CreateExecOptions, StartExecResults},
    models::{ContainerUpdateBody, NetworkCreateRequest, VolumeCreateRequest},
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, ListContainersOptionsBuilder,
        RemoveContainerOptionsBuilder, StatsOptionsBuilder, StopContainerOptionsBuilder,
    },
};
use futures_util::StreamExt as _;

pub use crate::spec::{
    Capability, ContainerSpec, ImageRef, Limits, MANAGED_LABEL, Name, Network, PublishedPort,
    ROLE_LABEL, Restart, SpecError, TENANT_LABEL, Tmpfs, VolumeMount,
};

/// Longest a single API call may take (pulls have their own, longer limit).
const API_TIMEOUT_SECS: u64 = 30;
/// Longest an image pull may take.
pub const PULL_TIMEOUT: Duration = Duration::from_secs(600);

/// Longest a command run inside a container may take.
const EXEC_TIMEOUT: Duration = Duration::from_secs(20);
/// Most output kept from such a command.
const MAX_EXEC_OUTPUT: usize = 8 * 1024;

/// The commands the manager runs inside a web container. Each is a fixed list of
/// words; nothing a request says ever becomes part of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NginxCommand {
    /// Check the configuration (`nginx -t`).
    Test,
    /// Load the configuration without dropping connections (`nginx -s reload`).
    Reload,
}

impl NginxCommand {
    fn argv(self) -> &'static [&'static str] {
        match self {
            Self::Test => &["nginx", "-t"],
            Self::Reload => &["nginx", "-s", "reload"],
        }
    }
}

/// How a command inside a container ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    /// Exit code (`-1` when Docker did not say).
    pub exit_code: i64,
    /// What it printed (stdout and stderr together), clipped.
    pub output: String,
}

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    /// The request itself is not acceptable.
    #[error(transparent)]
    Invalid(#[from] SpecError),
    /// No such container, image, volume or network.
    #[error("not found")]
    NotFound,
    /// The object exists but was not created by this crate.
    #[error("not managed by DHPanel")]
    NotManaged,
    /// Something with that name exists already, or the object is in use.
    #[error("conflict: {0}")]
    Conflict(String),
    /// The Docker daemon cannot be reached or did not answer in time.
    #[error("docker is unavailable: {0}")]
    Unavailable(String),
    /// Docker refused or failed.
    #[error("docker error: {0}")]
    Api(String),
}

impl DockerError {
    /// Whether trying again later may work.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }
}

fn map_error(err: BollardError) -> DockerError {
    match err {
        BollardError::DockerResponseServerError {
            status_code: 404, ..
        } => DockerError::NotFound,
        BollardError::DockerResponseServerError {
            status_code: 409,
            message,
        } => DockerError::Conflict(clip(&message)),
        BollardError::DockerResponseServerError {
            status_code,
            message,
        } => DockerError::Api(format!("{status_code}: {}", clip(&message))),
        BollardError::RequestTimeoutError => DockerError::Unavailable("timed out".into()),
        other @ (BollardError::IOError { .. }
        | BollardError::HyperResponseError { .. }
        | BollardError::SocketNotFoundError(_)) => {
            DockerError::Unavailable(clip(&other.to_string()))
        }
        other => DockerError::Api(clip(&other.to_string())),
    }
}

fn clip(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(300).collect()
}

/// A container's state, as far as callers need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfo {
    /// Docker's id.
    pub id: String,
    /// Our name.
    pub name: String,
    /// The image reference it was created from.
    pub image: String,
    /// `created`, `running`, `exited`, `paused`, `restarting`, `dead`, ...
    pub status: String,
    /// Whether the main process is running.
    pub running: bool,
    /// Exit code of the last run, if it has ended.
    pub exit_code: Option<i64>,
    /// Labels.
    pub labels: HashMap<String, String>,
    /// The limits in force (zeros if Docker reports none).
    pub limits: Limits,
}

/// A point-in-time reading of a container's resource usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerUsage {
    /// Memory in use, in bytes.
    pub memory_bytes: u64,
    /// The memory limit, in bytes.
    pub memory_limit_bytes: u64,
    /// CPU time used since the container started, in nanoseconds.
    pub cpu_total_nanos: u64,
    /// Processes right now.
    pub pids: u64,
}

/// The Docker actions the agent may take.
#[derive(Clone)]
pub struct DockerManager {
    docker: Docker,
}

impl DockerManager {
    /// Connects to the daemon behind a Unix socket. Nothing is sent yet; use
    /// [`DockerManager::version`] to check that it answers.
    ///
    /// # Errors
    /// [`DockerError::Unavailable`] when the client cannot be set up.
    pub fn connect(socket: &Path) -> Result<Self, DockerError> {
        let path = socket
            .to_str()
            .ok_or_else(|| DockerError::Unavailable("the socket path is not valid UTF-8".into()))?;
        let docker =
            Docker::connect_with_unix(path, API_TIMEOUT_SECS, bollard::API_DEFAULT_VERSION)
                .map_err(map_error)?;
        Ok(Self { docker })
    }

    /// The daemon's version, which also proves it is reachable.
    ///
    /// # Errors
    /// [`DockerError::Unavailable`] or [`DockerError::Api`].
    pub async fn version(&self) -> Result<String, DockerError> {
        let version = self.docker.version().await.map_err(map_error)?;
        Ok(version.version.unwrap_or_default())
    }

    /// Pulls an image by digest. Returns when the image is present locally.
    ///
    /// # Errors
    /// [`DockerError::NotFound`] if the registry does not have it,
    /// [`DockerError::Unavailable`] on time-out, [`DockerError::Api`] otherwise.
    pub async fn pull_image(&self, image: &ImageRef) -> Result<(), DockerError> {
        let options = CreateImageOptionsBuilder::default()
            .from_image(image.as_str())
            .build();
        let pull = async {
            let mut progress = self.docker.create_image(Some(options), None, None);
            while let Some(step) = progress.next().await {
                step.map_err(map_error)?;
            }
            Ok::<(), DockerError>(())
        };
        tokio::time::timeout(PULL_TIMEOUT, pull)
            .await
            .map_err(|_| DockerError::Unavailable("the image pull timed out".into()))??;
        // The pull stream can end without an error for an image that is not there.
        self.docker
            .inspect_image(image.as_str())
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    /// Creates a container (not started). Returns Docker's id.
    ///
    /// # Errors
    /// [`DockerError::Invalid`] for a bad specification, [`DockerError::Conflict`]
    /// if the name is taken, [`DockerError::NotFound`] if the image, a volume or
    /// the network is missing.
    pub async fn create_container(&self, spec: &ContainerSpec) -> Result<String, DockerError> {
        let body = spec.to_create_body()?;
        let options = CreateContainerOptionsBuilder::default()
            .name(spec.name.as_str())
            .build();
        let created = self
            .docker
            .create_container(Some(options), body)
            .await
            .map_err(map_error)?;
        Ok(created.id)
    }

    /// Starts a container we created. Starting a running container is fine.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`], [`DockerError::Api`].
    pub async fn start(&self, name: &Name) -> Result<(), DockerError> {
        self.require_managed_container(name).await?;
        self.docker
            .start_container(name.as_str(), None)
            .await
            .map_err(map_error)
    }

    /// Stops a container we created: SIGTERM, then SIGKILL after `grace`.
    /// Stopping a stopped container is fine.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`], [`DockerError::Api`].
    pub async fn stop(&self, name: &Name, grace: Duration) -> Result<(), DockerError> {
        self.require_managed_container(name).await?;
        let seconds = i32::try_from(grace.as_secs().min(300)).unwrap_or(300);
        let options = StopContainerOptionsBuilder::default().t(seconds).build();
        self.docker
            .stop_container(name.as_str(), Some(options))
            .await
            .map_err(map_error)
    }

    /// Removes a container we created (killing it first if `force`). Its
    /// anonymous volumes go with it; named volumes stay.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`],
    /// [`DockerError::Conflict`] if it is running and `force` is false.
    pub async fn remove(&self, name: &Name, force: bool) -> Result<(), DockerError> {
        self.require_managed_container(name).await?;
        let options = RemoveContainerOptionsBuilder::default()
            .force(force)
            .v(true)
            .build();
        self.docker
            .remove_container(name.as_str(), Some(options))
            .await
            .map_err(map_error)
    }

    /// Describes a container we created.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`].
    pub async fn inspect(&self, name: &Name) -> Result<ContainerInfo, DockerError> {
        self.require_managed_container(name).await
    }

    /// Runs one of the fixed nginx commands inside a running web container of
    /// ours. No argument comes from the caller: the command is the variant.
    ///
    /// # Errors
    /// [`DockerError::NotFound`] (no such container), [`DockerError::NotManaged`],
    /// [`DockerError::Conflict`] when the container is not running, or an API error.
    pub async fn run_nginx(
        &self,
        name: &Name,
        command: NginxCommand,
    ) -> Result<CommandResult, DockerError> {
        let info = self.require_managed_container(name).await?;
        if !info.running {
            return Err(DockerError::Conflict("the container is not running".into()));
        }
        let created = self
            .docker
            .create_exec(
                name.as_str(),
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command.argv().to_vec()),
                    ..Default::default()
                },
            )
            .await
            .map_err(map_error)?;
        let started = self
            .docker
            .start_exec(&created.id, None)
            .await
            .map_err(map_error)?;
        let mut output = String::new();
        if let StartExecResults::Attached {
            output: mut stream, ..
        } = started
        {
            let read = async {
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(map_error)?;
                    if output.len() < MAX_EXEC_OUTPUT {
                        output.push_str(&String::from_utf8_lossy(&chunk.into_bytes()));
                    }
                }
                Ok::<(), DockerError>(())
            };
            tokio::time::timeout(EXEC_TIMEOUT, read)
                .await
                .map_err(|_| DockerError::Unavailable("the command timed out".into()))??;
        }
        output.truncate(output.floor_char_boundary(MAX_EXEC_OUTPUT));
        let done = self
            .docker
            .inspect_exec(&created.id)
            .await
            .map_err(map_error)?;
        Ok(CommandResult {
            exit_code: done.exit_code.unwrap_or(-1),
            output,
        })
    }

    /// Reads a running container's resource usage once.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`], [`DockerError::Api`].
    pub async fn usage(&self, name: &Name) -> Result<ContainerUsage, DockerError> {
        self.require_managed_container(name).await?;
        let options = StatsOptionsBuilder::default()
            .stream(false)
            .one_shot(true)
            .build();
        let mut stream = self.docker.stats(name.as_str(), Some(options));
        let stats = stream
            .next()
            .await
            .ok_or_else(|| DockerError::Api("no statistics returned".into()))?
            .map_err(map_error)?;
        let memory = stats.memory_stats.unwrap_or_default();
        Ok(ContainerUsage {
            memory_bytes: memory.usage.unwrap_or(0),
            memory_limit_bytes: memory.limit.unwrap_or(0),
            cpu_total_nanos: stats
                .cpu_stats
                .and_then(|c| c.cpu_usage)
                .and_then(|u| u.total_usage)
                .unwrap_or(0),
            pids: stats.pids_stats.and_then(|p| p.current).unwrap_or(0),
        })
    }

    /// Changes the limits of a container we created, running or not, without
    /// restarting it. Memory and swap are set to the same value, so a container
    /// never gets swap.
    ///
    /// # Errors
    /// [`DockerError::Invalid`] for numbers out of range, [`DockerError::NotFound`],
    /// [`DockerError::NotManaged`], [`DockerError::Api`] (for example when the new
    /// memory limit is below what the container already uses).
    pub async fn update_limits(&self, name: &Name, limits: &Limits) -> Result<(), DockerError> {
        limits.validate()?;
        self.require_managed_container(name).await?;
        // Validated above: at most 1 TiB, so this always fits.
        let memory = i64::try_from(limits.memory_bytes).unwrap_or(i64::MAX);
        let body = ContainerUpdateBody {
            memory: Some(memory),
            memory_swap: Some(memory),
            nano_cpus: Some(i64::from(limits.cpu_millis) * 1_000_000),
            pids_limit: Some(i64::from(limits.pids_limit)),
            ..Default::default()
        };
        self.docker
            .update_container(name.as_str(), body)
            .await
            .map_err(map_error)
    }

    /// Names of all containers we created (running or not), sorted.
    ///
    /// # Errors
    /// [`DockerError::Unavailable`] or [`DockerError::Api`].
    pub async fn list(&self) -> Result<Vec<String>, DockerError> {
        let filters: HashMap<String, Vec<String>> =
            [("label".to_owned(), vec![format!("{MANAGED_LABEL}=true")])].into();
        let options = ListContainersOptionsBuilder::default()
            .all(true)
            .filters(&filters)
            .build();
        let mut names: Vec<String> = self
            .docker
            .list_containers(Some(options))
            .await
            .map_err(map_error)?
            .into_iter()
            .filter_map(|c| c.names.and_then(|n| n.into_iter().next()))
            .map(|n| n.trim_start_matches('/').to_owned())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Creates a named volume. Creating one that already exists (and is ours)
    /// is fine.
    ///
    /// # Errors
    /// [`DockerError::NotManaged`] if a volume of that name exists that we did
    /// not create.
    pub async fn create_volume(&self, name: &Name) -> Result<(), DockerError> {
        match self.docker.inspect_volume(name.as_str()).await {
            Ok(existing) => {
                return if existing.labels.get(MANAGED_LABEL).map(String::as_str) == Some("true") {
                    Ok(())
                } else {
                    Err(DockerError::NotManaged)
                };
            }
            Err(err) => match map_error(err) {
                DockerError::NotFound => {}
                other => return Err(other),
            },
        }
        self.docker
            .create_volume(VolumeCreateRequest {
                name: Some(name.as_str().to_owned()),
                labels: Some([(MANAGED_LABEL.to_owned(), "true".to_owned())].into()),
                ..Default::default()
            })
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    /// Creates a named volume whose data lives in a host directory (the
    /// tenant's directory, prepared by the privileged helper). The directory
    /// must be an absolute, plain path *below* `allowed_root` (a path with
    /// `..`, a symlink-looking trick or anything outside the root is refused),
    /// so a request can never make Docker expose an arbitrary host path.
    /// Creating one that already exists with the same directory is fine.
    ///
    /// # Errors
    /// [`DockerError::Invalid`] for a path outside the root,
    /// [`DockerError::NotManaged`] or [`DockerError::Conflict`] if the volume
    /// exists but is not ours or points somewhere else.
    pub async fn create_bind_volume(
        &self,
        name: &Name,
        host_dir: &Path,
        allowed_root: &Path,
    ) -> Result<(), DockerError> {
        let device = bind_device(host_dir, allowed_root)?;
        match self.docker.inspect_volume(name.as_str()).await {
            Ok(existing) => {
                if existing.labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
                    return Err(DockerError::NotManaged);
                }
                return if existing.options.get("device").map(String::as_str)
                    == Some(device.as_str())
                {
                    Ok(())
                } else {
                    Err(DockerError::Conflict(
                        "the volume points to another directory".into(),
                    ))
                };
            }
            Err(err) => match map_error(err) {
                DockerError::NotFound => {}
                other => return Err(other),
            },
        }
        self.docker
            .create_volume(VolumeCreateRequest {
                name: Some(name.as_str().to_owned()),
                driver: Some("local".to_owned()),
                driver_opts: Some(
                    [
                        ("type".to_owned(), "none".to_owned()),
                        ("o".to_owned(), "bind".to_owned()),
                        ("device".to_owned(), device),
                    ]
                    .into(),
                ),
                labels: Some([(MANAGED_LABEL.to_owned(), "true".to_owned())].into()),
                ..Default::default()
            })
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    /// Removes a named volume we created, with its data.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`],
    /// [`DockerError::Conflict`] while a container uses it.
    pub async fn remove_volume(&self, name: &Name) -> Result<(), DockerError> {
        let volume = self
            .docker
            .inspect_volume(name.as_str())
            .await
            .map_err(map_error)?;
        if volume.labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
            return Err(DockerError::NotManaged);
        }
        self.docker
            .remove_volume(
                name.as_str(),
                None::<bollard::query_parameters::RemoveVolumeOptions>,
            )
            .await
            .map_err(map_error)
    }

    /// Creates a bridge network. With `internal`, containers on it can talk to
    /// each other but not to the outside.
    ///
    /// # Errors
    /// [`DockerError::Conflict`] if the name is taken.
    pub async fn create_network(&self, name: &Name, internal: bool) -> Result<(), DockerError> {
        self.docker
            .create_network(NetworkCreateRequest {
                name: name.as_str().to_owned(),
                driver: Some("bridge".to_owned()),
                internal: Some(internal),
                labels: Some([(MANAGED_LABEL.to_owned(), "true".to_owned())].into()),
                ..Default::default()
            })
            .await
            .map(|_| ())
            .map_err(map_error)
    }

    /// Creates the network unless it already exists and is ours.
    ///
    /// # Errors
    /// [`DockerError::NotManaged`] if a network of that name exists that we did
    /// not create.
    pub async fn ensure_network(&self, name: &Name, internal: bool) -> Result<(), DockerError> {
        match self.docker.inspect_network(name.as_str(), None).await {
            Ok(existing) => {
                let ours = existing
                    .labels
                    .as_ref()
                    .and_then(|l| l.get(MANAGED_LABEL))
                    .map(String::as_str)
                    == Some("true");
                return if ours {
                    Ok(())
                } else {
                    Err(DockerError::NotManaged)
                };
            }
            Err(err) => match map_error(err) {
                DockerError::NotFound => {}
                other => return Err(other),
            },
        }
        self.create_network(name, internal).await
    }

    /// Removes a network we created.
    ///
    /// # Errors
    /// [`DockerError::NotFound`], [`DockerError::NotManaged`],
    /// [`DockerError::Conflict`] while a container is attached.
    pub async fn remove_network(&self, name: &Name) -> Result<(), DockerError> {
        let network = self
            .docker
            .inspect_network(name.as_str(), None)
            .await
            .map_err(map_error)?;
        let ours = network
            .labels
            .as_ref()
            .and_then(|l| l.get(MANAGED_LABEL))
            .map(String::as_str)
            == Some("true");
        if !ours {
            return Err(DockerError::NotManaged);
        }
        self.docker
            .remove_network(name.as_str())
            .await
            .map_err(map_error)
    }

    async fn require_managed_container(&self, name: &Name) -> Result<ContainerInfo, DockerError> {
        let c = self
            .docker
            .inspect_container(name.as_str(), None)
            .await
            .map_err(map_error)?;
        let config = c.config.unwrap_or_default();
        let labels = config.labels.unwrap_or_default();
        if labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
            return Err(DockerError::NotManaged);
        }
        let state = c.state.unwrap_or_default();
        let running = state.running.unwrap_or(false);
        Ok(ContainerInfo {
            id: c.id.unwrap_or_default(),
            name: c
                .name
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_owned(),
            image: config.image.unwrap_or_default(),
            status: state.status.map(|s| s.to_string()).unwrap_or_default(),
            running,
            exit_code: (!running).then_some(state.exit_code).flatten(),
            labels,
            limits: {
                let host = c.host_config.unwrap_or_default();
                Limits {
                    memory_bytes: host.memory.and_then(|m| u64::try_from(m).ok()).unwrap_or(0),
                    cpu_millis: host
                        .nano_cpus
                        .and_then(|n| u32::try_from(n / 1_000_000).ok())
                        .unwrap_or(0),
                    pids_limit: host
                        .pids_limit
                        .and_then(|p| u32::try_from(p).ok())
                        .unwrap_or(0),
                }
            },
        })
    }
}

/// Checks that `dir` is an absolute path strictly below `root`, made of plain
/// components only, and returns it as text for Docker.
fn bind_device(dir: &Path, root: &Path) -> Result<String, DockerError> {
    use std::path::Component;
    let plain = |p: &Path| {
        p.is_absolute()
            && p.components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
    };
    let bad = |reason: &str| {
        DockerError::Invalid(SpecError {
            field: "host_dir",
            reason: reason.to_owned(),
        })
    };
    if !plain(dir) || !plain(root) {
        return Err(bad("must be an absolute path without `.` or `..`"));
    }
    if dir == root || !dir.starts_with(root) {
        return Err(bad("must be inside the allowed directory"));
    }
    let text = dir.to_str().ok_or_else(|| bad("must be valid UTF-8"))?;
    // `Path` quietly drops `.` and repeated or trailing slashes when it walks the
    // components; what Docker receives is the text, so the text must already be
    // in its plain form.
    let normal: std::path::PathBuf = dir.components().collect();
    if normal.to_str() != Some(text) {
        return Err(bad(
            "must be written plainly (no `.`, `//` or trailing `/`)",
        ));
    }
    if text.chars().any(|c| c.is_control() || c == ',' || c == '=') {
        return Err(bad("contains a character Docker options cannot carry"));
    }
    Ok(text.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn bind_directories_must_be_plain_paths_inside_the_root() {
        let root = Path::new("/srv/tenants");
        assert_eq!(
            bind_device(Path::new("/srv/tenants/0190b6a2/www"), root).unwrap(),
            "/srv/tenants/0190b6a2/www"
        );
        for bad in [
            "/srv/tenants",
            "/srv",
            "/etc",
            "/srv/tenants/../etc",
            "/srv/tenants/./x",
            "srv/tenants/x",
            "/srv/tenantsX/x",
            "/srv/tenants/a,b",
            "/srv/tenants/a=b",
            "/srv/tenants/a\nb",
        ] {
            assert!(bind_device(Path::new(bad), root).is_err(), "{bad:?}");
        }
        assert!(bind_device(Path::new("/srv/tenants/x"), Path::new("/srv/../etc")).is_err());
    }
}
