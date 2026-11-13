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
    models::{NetworkCreateRequest, VolumeCreateRequest},
    query_parameters::{
        CreateContainerOptionsBuilder, CreateImageOptionsBuilder, ListContainersOptionsBuilder,
        RemoveContainerOptionsBuilder, StatsOptionsBuilder, StopContainerOptionsBuilder,
    },
};
use futures_util::StreamExt as _;

pub use crate::spec::{
    Capability, ContainerSpec, ImageRef, MANAGED_LABEL, Name, Network, PublishedPort, Restart,
    SpecError, Tmpfs, VolumeMount,
};

/// Longest a single API call may take (pulls have their own, longer limit).
const API_TIMEOUT_SECS: u64 = 30;
/// Longest an image pull may take.
pub const PULL_TIMEOUT: Duration = Duration::from_secs(600);

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
        })
    }
}
