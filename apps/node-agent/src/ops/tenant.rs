//! Tenant operations (T04.07 onwards).
//!
//! `create_tenant` prepares a node for a tenant in three steps, each safe to
//! repeat so that a retried operation finishes the job instead of failing:
//!
//! 1. the privileged helper creates `/srv/tenants/<id>` and `.../www`, owned by
//!    the tenant's host id (the only part that needs root);
//! 2. the tenant's private network;
//! 3. the tenant's files volume, which is that `www` directory (the only kind of
//!    host path Docker is ever asked to expose, and only below the tenants root).
//!
//! The agent re-checks every argument: the control plane is trusted to ask, not
//! to ask correctly.

use std::{
    io::{BufRead as _, BufReader, Write as _},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use dhpanel_docker_manager::{
    ContainerSpec, DockerError, DockerManager, Name, Network, VolumeMount,
};
use dhpanel_domain::{TenantUsername, UidRange};
use dhpanel_node_protocol::{AgentError, ErrorCode, Operation, Output};
use dhpanel_priv_helper::protocol::{Action, ErrorCode as HelperCode, Request, Response};
use dhpanel_tenant_stack::{
    Allowlist, PlanResources, Role, Tenant, files_volume_name, network_name, plan_limits, render,
};
use uuid::Uuid;

/// Longest the helper may take to answer.
const HELPER_TIMEOUT: Duration = Duration::from_secs(10);

/// What tenant operations need.
#[derive(Clone)]
pub struct TenantContext {
    /// The privileged helper's socket.
    pub helper_socket: PathBuf,
    /// Where tenant directories live (must match the helper's `--tenants-root`).
    pub tenants_root: PathBuf,
    /// The agent's Docker.
    pub docker: DockerManager,
}

/// Sends one request to the helper and reads its answer.
///
/// # Errors
/// A connection or protocol failure (retryable) or the helper's own refusal.
pub fn ask_helper(socket: &Path, action: Action) -> Result<(), AgentError> {
    let unavailable = |what: &str| {
        AgentError::new(
            ErrorCode::Internal,
            format!("the privileged helper is unavailable: {what}"),
        )
        .retryable()
    };
    let stream = UnixStream::connect(socket).map_err(|e| unavailable(&e.kind().to_string()))?;
    stream
        .set_read_timeout(Some(HELPER_TIMEOUT))
        .map_err(|e| unavailable(&e.kind().to_string()))?;
    stream
        .set_write_timeout(Some(HELPER_TIMEOUT))
        .map_err(|e| unavailable(&e.kind().to_string()))?;
    let request = Request {
        id: format!("agent-{}", Uuid::now_v7().simple()),
        action,
    };
    let mut line = serde_json::to_vec(&request).map_err(|e| unavailable(&e.to_string()))?;
    line.push(b'\n');
    (&stream)
        .write_all(&line)
        .map_err(|e| unavailable(&e.kind().to_string()))?;
    let mut answer = String::new();
    BufReader::new(&stream)
        .read_line(&mut answer)
        .map_err(|e| unavailable(&e.kind().to_string()))?;
    let response: Response =
        serde_json::from_str(&answer).map_err(|_| unavailable("unreadable answer"))?;
    if response.ok {
        return Ok(());
    }
    let error = response.error;
    Err(match error.as_ref().map(|e| e.code) {
        Some(
            HelperCode::InvalidArguments | HelperCode::UnknownAction | HelperCode::InvalidRequest,
        ) => AgentError::new(
            ErrorCode::InvalidArguments,
            "the privileged helper refused the request",
        ),
        Some(HelperCode::Forbidden) => AgentError::new(
            ErrorCode::Internal,
            "the privileged helper does not accept this agent (check its --agent-user)",
        ),
        Some(HelperCode::Unsupported) => AgentError::new(
            ErrorCode::UnsupportedOperation,
            "this node's filesystem cannot enforce storage limits (it needs XFS with project quotas)",
        ),
        // The helper tried and failed: another try may succeed once the cause is fixed.
        _ => AgentError::new(
            ErrorCode::Internal,
            error.map_or_else(
                || "the privileged helper failed".to_owned(),
                |e| format!("the privileged helper failed: {}", e.message),
            ),
        )
        .retryable(),
    })
}

fn docker_error(err: &DockerError) -> AgentError {
    let message = format!("docker: {err}");
    match err {
        DockerError::Unavailable(_) => AgentError::new(ErrorCode::Internal, message).retryable(),
        DockerError::Invalid(_) | DockerError::NotManaged | DockerError::Conflict(_) => {
            AgentError::new(ErrorCode::InvalidArguments, message)
        }
        _ => AgentError::new(ErrorCode::Internal, message),
    }
}

/// `set_storage_quota`.
///
/// # Errors
/// [`AgentError`]: bad arguments, `unsupported_operation` when the node cannot
/// enforce quotas, or the helper being unavailable (retryable).
pub async fn set_storage_quota(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::SetStorageQuota {
        tenant_id,
        uid,
        disk_bytes,
        inodes,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    if !UidRange::DEFAULT.contains(uid) {
        return Err(AgentError::new(
            ErrorCode::InvalidArguments,
            "the user id is outside the tenant range",
        ));
    }
    let socket = context.helper_socket.clone();
    tokio::task::spawn_blocking(move || {
        ask_helper(
            &socket,
            Action::SetStorageQuota {
                tenant_id: tenant_id.to_string(),
                uid,
                disk_bytes,
                inodes,
            },
        )
    })
    .await
    .map_err(|_| {
        AgentError::new(ErrorCode::Internal, "the helper call was cancelled").retryable()
    })??;
    Ok(Output::StorageQuotaSet {
        backend: "xfs".to_owned(),
    })
}

/// How long the stack gets to come up, and how long a container must stay up
/// to count as healthy (a crash loop restarts faster than this).
const START_TIMEOUT: Duration = Duration::from_secs(60);
const STEADY_FOR: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(500);

/// Waits until `name` has been running steadily for [`STEADY_FOR`].
async fn wait_healthy(docker: &DockerManager, name: &Name) -> Result<(), AgentError> {
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    let mut up_since: Option<tokio::time::Instant> = None;
    loop {
        let info = docker.inspect(name).await.map_err(|e| docker_error(&e))?;
        match (info.running, info.status.as_str()) {
            (true, "running") => {
                let since = *up_since.get_or_insert_with(tokio::time::Instant::now);
                if since.elapsed() >= STEADY_FOR {
                    return Ok(());
                }
            }
            (_, "exited" | "dead") => {
                return Err(AgentError::new(
                    ErrorCode::Internal,
                    format!(
                        "container {} stopped right after starting (exit code {})",
                        name.as_str(),
                        info.exit_code
                            .map_or_else(|| "?".to_owned(), |c| c.to_string())
                    ),
                ));
            }
            // Restarting, created or paused: not steady yet.
            _ => up_since = None,
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AgentError::new(
                ErrorCode::Internal,
                format!("container {} did not come up in time", name.as_str()),
            )
            .retryable());
        }
        tokio::time::sleep(POLL).await;
    }
}

/// `provision_tenant_stack`.
///
/// # Errors
/// [`AgentError`]: bad arguments or a plan too small for the stack (not
/// retryable); Docker unavailable, a slow start (retryable); a container that
/// stops again right after starting (not retryable: it needs a person).
pub async fn provision_tenant_stack(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::ProvisionTenantStack {
        tenant_id,
        username,
        uid,
        cpu_millis,
        memory_bytes,
        pids,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let invalid = |m: &str| AgentError::new(ErrorCode::InvalidArguments, m.to_owned());
    let username = TenantUsername::parse(&username).map_err(|e| invalid(&e.to_string()))?;
    let uid = UidRange::DEFAULT
        .checked(uid)
        .ok_or_else(|| invalid("the user id is outside the tenant range"))?;
    let allowlist = Allowlist::embedded().map_err(|e| {
        AgentError::new(
            ErrorCode::Internal,
            format!("the image list is broken: {e}"),
        )
    })?;
    let stack = render(
        &Tenant {
            id: tenant_id,
            username: &username,
            uid,
            resources: PlanResources {
                cpu_millis,
                memory_bytes,
                pids,
            },
        },
        &allowlist,
    )
    .map_err(|e| invalid(&e.to_string()))?;

    let docker = &context.docker;
    let mut started = Vec::new();
    for spec in &stack.services {
        match docker.inspect(&spec.name).await {
            Ok(_) => {}
            Err(DockerError::NotFound) => {
                docker
                    .pull_image(&spec.image)
                    .await
                    .map_err(|e| docker_error(&e))?;
                match docker.create_container(spec).await {
                    // Another attempt created it in the meantime.
                    Ok(_) | Err(DockerError::Conflict(_)) => {}
                    Err(e) => return Err(docker_error(&e)),
                }
            }
            Err(e) => return Err(docker_error(&e)),
        }
        docker
            .start(&spec.name)
            .await
            .map_err(|e| docker_error(&e))?;
        wait_healthy(docker, &spec.name).await?;
        started.push(spec.name.as_str().to_owned());
    }
    Ok(Output::StackProvisioned {
        containers: started,
    })
}

/// `set_up_tenant`: the three set-up steps in a row. Each step is repeatable, so
/// a retry after a failure in the middle carries on where it stopped.
///
/// # Errors
/// As the steps it runs.
pub async fn set_up_tenant(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::SetUpTenant {
        tenant_id,
        username,
        uid,
        disk_bytes,
        inodes,
        cpu_millis,
        memory_bytes,
        pids,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    create_tenant(
        context.clone(),
        Operation::CreateTenant {
            tenant_id,
            username: username.clone(),
            uid,
        },
    )
    .await?;
    let mut storage_quota = None;
    if disk_bytes.is_some() || inodes.is_some() {
        match set_storage_quota(
            context.clone(),
            Operation::SetStorageQuota {
                tenant_id,
                uid,
                disk_bytes,
                inodes,
            },
        )
        .await
        {
            Ok(Output::StorageQuotaSet { backend }) => storage_quota = Some(backend),
            Ok(_) => {}
            // A node without project quotas runs the tenant without a disk limit;
            // the control plane knows from the node's capabilities.
            Err(e) if e.code == ErrorCode::UnsupportedOperation => {}
            Err(e) => return Err(e),
        }
    }
    let Output::StackProvisioned { containers } = provision_tenant_stack(
        context,
        Operation::ProvisionTenantStack {
            tenant_id,
            username,
            uid,
            cpu_millis,
            memory_bytes,
            pids,
        },
    )
    .await?
    else {
        return Err(AgentError::new(
            ErrorCode::Internal,
            "unexpected answer while setting up a tenant",
        ));
    };
    Ok(Output::TenantReady {
        containers,
        storage_quota,
    })
}

/// How long a stopping container gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(20);
/// Longest the file-deleting container may take.
const PURGE_TIMEOUT: Duration = Duration::from_secs(600);

fn parse_username(username: &str) -> Result<TenantUsername, AgentError> {
    TenantUsername::parse(username)
        .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))
}

/// Names of a tenant's containers, in start order (PHP, then the web server).
fn stack_names(username: &TenantUsername) -> Result<Vec<Name>, AgentError> {
    plan_limits(
        username,
        &PlanResources {
            cpu_millis: None,
            memory_bytes: None,
            pids: None,
        },
    )
    .map(|all| all.into_iter().map(|(name, _)| name).collect())
    .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))
}

/// Fails (not retryable) unless every container of the stack exists.
async fn require_provisioned(docker: &DockerManager, names: &[Name]) -> Result<(), AgentError> {
    for name in names {
        match docker.inspect(name).await {
            Ok(_) => {}
            Err(DockerError::NotFound) => {
                return Err(AgentError::new(
                    ErrorCode::InvalidArguments,
                    "the tenant's stack has not been provisioned here",
                ));
            }
            Err(e) => return Err(docker_error(&e)),
        }
    }
    Ok(())
}

/// `suspend_tenant`: stops the web server first, then PHP. The files stay.
///
/// # Errors
/// [`AgentError`]: bad arguments or no stack here (not retryable); Docker
/// unavailable (retryable).
pub async fn suspend_tenant(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::SuspendTenant { username } = op else {
        return Err(super::wrong_operation());
    };
    let mut names = stack_names(&parse_username(&username)?)?;
    require_provisioned(&context.docker, &names).await?;
    names.reverse();
    let mut stopped = Vec::new();
    for name in &names {
        context
            .docker
            .stop(name, STOP_GRACE)
            .await
            .map_err(|e| docker_error(&e))?;
        stopped.push(name.as_str().to_owned());
    }
    Ok(Output::TenantSuspended {
        containers: stopped,
    })
}

/// `resume_tenant`: starts PHP, then the web server, and waits until they run.
///
/// # Errors
/// As [`provision_tenant_stack`], without the plan checks.
pub async fn resume_tenant(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::ResumeTenant { username } = op else {
        return Err(super::wrong_operation());
    };
    let names = stack_names(&parse_username(&username)?)?;
    require_provisioned(&context.docker, &names).await?;
    let mut running = Vec::new();
    for name in &names {
        context
            .docker
            .start(name)
            .await
            .map_err(|e| docker_error(&e))?;
        wait_healthy(&context.docker, name).await?;
        running.push(name.as_str().to_owned());
    }
    Ok(Output::TenantResumed {
        containers: running,
    })
}

/// Ignores "it is not there": removing what is already gone is success.
fn gone_is_fine(result: Result<(), DockerError>) -> Result<(), AgentError> {
    match result {
        Ok(()) | Err(DockerError::NotFound) => Ok(()),
        Err(e) => Err(docker_error(&e)),
    }
}

/// Deletes every file of a tenant by running a throw-away container as the
/// tenant itself: the tenant may delete its own files and nobody else's, so no
/// privileged code ever walks a directory the tenant controls.
async fn delete_files(
    context: &TenantContext,
    id: &str,
    username: &TenantUsername,
    uid: u32,
) -> Result<(), AgentError> {
    let docker = &context.docker;
    let root = context.tenants_root.join(id);
    let volume = files_volume_name(username).map_err(|e| invalid_arg(&e.to_string()))?;
    // The volume goes when a tenant is deleted without purging; make it again.
    docker
        .create_bind_volume(&volume, &root.join("www"), &context.tenants_root)
        .await
        .map_err(|e| docker_error(&e))?;
    let allowlist = Allowlist::embedded().map_err(|e| {
        AgentError::new(
            ErrorCode::Internal,
            format!("the image list is broken: {e}"),
        )
    })?;
    let image = allowlist
        .current(Role::Php)
        .map_err(|e| invalid_arg(&e.to_string()))?
        .clone();
    let name =
        Name::parse(&format!("dh-{username}-purge")).map_err(|e| invalid_arg(&e.to_string()))?;
    let mut spec = ContainerSpec::new(name.clone(), image.clone());
    spec.command = ["find", "/data", "-mindepth", "1", "-delete"]
        .map(str::to_owned)
        .to_vec();
    spec.user = Some((uid, uid));
    spec.memory_bytes = 64 * 1024 * 1024;
    spec.pids_limit = 32;
    spec.network = Network::None;
    spec.volumes = vec![VolumeMount {
        volume,
        target: "/data".to_owned(),
        read_only: false,
    }];
    gone_is_fine(docker.remove(&name, true).await)?; // left over from a failed attempt
    docker
        .pull_image(&image)
        .await
        .map_err(|e| docker_error(&e))?;
    docker
        .create_container(&spec)
        .await
        .map_err(|e| docker_error(&e))?;
    let outcome = async {
        docker.start(&name).await.map_err(|e| docker_error(&e))?;
        let deadline = tokio::time::Instant::now() + PURGE_TIMEOUT;
        loop {
            let info = docker.inspect(&name).await.map_err(|e| docker_error(&e))?;
            if !info.running && info.status != "created" {
                return if info.exit_code == Some(0) {
                    Ok(())
                } else {
                    Err(AgentError::new(
                        ErrorCode::Internal,
                        "deleting the tenant's files failed",
                    ))
                };
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AgentError::new(
                    ErrorCode::Internal,
                    "deleting the tenant's files took too long",
                )
                .retryable());
            }
            tokio::time::sleep(POLL).await;
        }
    }
    .await;
    gone_is_fine(docker.remove(&name, true).await)?;
    outcome
}

fn invalid_arg(message: &str) -> AgentError {
    AgentError::new(ErrorCode::InvalidArguments, message.to_owned())
}

/// `delete_tenant`: removes the stack; with `purge_data` also the files.
///
/// # Errors
/// [`AgentError`]: bad arguments (not retryable); Docker or the helper
/// unavailable, a slow purge (retryable).
pub async fn delete_tenant(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::DeleteTenant {
        tenant_id,
        username,
        uid,
        purge_data,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let username = parse_username(&username)?;
    if !UidRange::DEFAULT.contains(uid) {
        return Err(invalid_arg("the user id is outside the tenant range"));
    }
    let docker = &context.docker;
    let id = tenant_id.to_string();
    let mut names = stack_names(&username)?;
    names.reverse(); // web first
    for name in &names {
        gone_is_fine(docker.remove(name, true).await)?;
    }
    if purge_data {
        // Only if the directory is still there (a repeated purge has nothing to do).
        let root = context.tenants_root.join(&id);
        if tokio::fs::symlink_metadata(&root).await.is_ok() {
            delete_files(&context, &id, &username, uid).await?;
        }
    }
    let network = network_name(&username).map_err(|e| invalid_arg(&e.to_string()))?;
    let volume = files_volume_name(&username).map_err(|e| invalid_arg(&e.to_string()))?;
    gone_is_fine(docker.remove_volume(&volume).await)?;
    gone_is_fine(docker.remove_network(&network).await)?;
    if purge_data {
        let socket = context.helper_socket.clone();
        tokio::task::spawn_blocking(move || {
            ask_helper(&socket, Action::RemoveTenant { tenant_id: id, uid })
        })
        .await
        .map_err(|_| {
            AgentError::new(ErrorCode::Internal, "the helper call was cancelled").retryable()
        })??;
    }
    Ok(Output::TenantDeleted { purged: purge_data })
}

/// `set_resource_limits`.
///
/// # Errors
/// [`AgentError`]: bad arguments, a plan too small to run the stack, or a stack
/// that was never provisioned here (none retryable); Docker unavailable
/// (retryable).
pub async fn set_resource_limits(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::SetResourceLimits {
        username,
        cpu_millis,
        memory_bytes,
        pids,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let invalid = |m: &str| AgentError::new(ErrorCode::InvalidArguments, m.to_owned());
    let username = TenantUsername::parse(&username).map_err(|e| invalid(&e.to_string()))?;
    let wanted = plan_limits(
        &username,
        &PlanResources {
            cpu_millis,
            memory_bytes,
            pids,
        },
    )
    .map_err(|e| invalid(&e.to_string()))?;
    // All containers must be there before any is changed: a half-updated stack
    // is worse than a refused request.
    for (name, _) in &wanted {
        match context.docker.inspect(name).await {
            Ok(_) => {}
            Err(DockerError::NotFound) => {
                return Err(invalid("the tenant's stack has not been provisioned here"));
            }
            Err(e) => return Err(docker_error(&e)),
        }
    }
    let mut updated = Vec::new();
    for (name, limits) in &wanted {
        context
            .docker
            .update_limits(name, limits)
            .await
            .map_err(|e| docker_error(&e))?;
        updated.push(name.as_str().to_owned());
    }
    Ok(Output::ResourceLimitsSet {
        containers: updated,
    })
}

/// `create_tenant`.
///
/// # Errors
/// [`AgentError`]: bad arguments (not retryable), or the helper / Docker not
/// available (retryable).
pub async fn create_tenant(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::CreateTenant {
        tenant_id,
        username,
        uid,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let invalid = |m: &str| AgentError::new(ErrorCode::InvalidArguments, m.to_owned());
    let username = TenantUsername::parse(&username).map_err(|e| invalid(&e.to_string()))?;
    if !UidRange::DEFAULT.contains(uid) {
        return Err(invalid("the user id is outside the tenant range"));
    }
    let id = tenant_id.to_string();
    let root = context.tenants_root.join(&id);
    let (network, volume) = (
        network_name(&username).map_err(|e| invalid(&e.to_string()))?,
        files_volume_name(&username).map_err(|e| invalid(&e.to_string()))?,
    );

    // 1. Directories (blocking socket call off the async runtime).
    let socket = context.helper_socket.clone();
    tokio::task::spawn_blocking(move || {
        ask_helper(&socket, Action::PrepareTenant { tenant_id: id, uid })
    })
    .await
    .map_err(|_| {
        AgentError::new(ErrorCode::Internal, "the helper call was cancelled").retryable()
    })??;
    // 2. Network, 3. volume.
    context
        .docker
        .ensure_network(&network, false)
        .await
        .map_err(|e| docker_error(&e))?;
    context
        .docker
        .create_bind_volume(&volume, &root.join("www"), &context.tenants_root)
        .await
        .map_err(|e| docker_error(&e))?;
    Ok(Output::TenantCreated {
        root: root.display().to_string(),
        network: network.as_str().to_owned(),
        volume: volume.as_str().to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A context whose helper and Docker are not there: enough to see what the
    /// handler checks first and what it does when they are missing.
    fn nowhere() -> Arc<TenantContext> {
        Arc::new(TenantContext {
            helper_socket: "/nonexistent/helper.sock".into(),
            tenants_root: "/nonexistent/tenants".into(),
            // Docker is only contacted after the helper, so any existing path will do.
            docker: DockerManager::connect(Path::new("/")).unwrap(),
        })
    }

    fn create(username: &str, uid: u32) -> Operation {
        Operation::CreateTenant {
            tenant_id: Uuid::now_v7(),
            username: username.into(),
            uid,
        }
    }

    #[tokio::test]
    async fn bad_arguments_are_refused_before_anything_is_touched() {
        for (username, uid) in [
            ("Bad Name", 10_000),
            ("ab", 10_000),
            ("root", 10_000),
            ("alice", 0),
            ("alice", 9_999),
            ("alice", 60_000),
            ("alice", u32::MAX),
        ] {
            let err = create_tenant(nowhere(), create(username, uid))
                .await
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidArguments, "{username} {uid}");
            assert!(!err.retryable);
        }
        let err = create_tenant(nowhere(), Operation::Ping { nonce: 1 })
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            ErrorCode::Internal,
            "a mis-registered handler is an internal error"
        );
    }

    #[tokio::test]
    async fn a_missing_helper_is_a_retryable_failure_not_a_crash() {
        let err = create_tenant(nowhere(), create("alice", 10_000))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.retryable, "the helper may be starting: try again");
        assert!(err.message.contains("privileged helper"), "{}", err.message);
    }
}
