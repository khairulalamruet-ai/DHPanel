//! `list_containers` and `restart_container` (T10.12): an account's own services.
//!
//! The account is known by the label `dhpanel.tenant` that every container of its stack
//! carries, so a name that belongs to another account (or to the node: the database, phpMyAdmin,
//! the file server) is refused, not guessed from its spelling.

use std::{sync::Arc, time::Duration};

use dhpanel_docker_manager::{DockerError, Name};
use dhpanel_node_protocol::{AgentError, ContainerItem, Operation, Output};

use super::{
    sqlimport::invalid,
    tenant::{TenantContext, docker_error},
};

const GRACE: Duration = Duration::from_secs(15);

/// `list_containers`.
///
/// # Errors
/// [`AgentError`] when Docker cannot be asked (retryable).
pub async fn list_containers(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::ListContainers { tenant_id } = op else {
        return Err(super::wrong_operation());
    };
    let items = context
        .docker
        .list_tenant(&tenant_id.to_string())
        .await
        .map_err(|e| docker_error(&e))?
        .into_iter()
        .map(|c| ContainerItem {
            name: c.name,
            role: c.role,
            image: c.image,
            state: c.state,
            status: c.status,
            created: c.created,
        })
        .collect();
    Ok(Output::Containers { items })
}

/// `restart_container`.
///
/// # Errors
/// [`AgentError`]: a name that is not one of the account's containers (not retryable), or
/// Docker failing.
pub async fn restart_container(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::RestartContainer { tenant_id, name } = op else {
        return Err(super::wrong_operation());
    };
    let name = Name::parse(&name).map_err(|_| invalid("the container's name is not usable"))?;
    let info = context.docker.inspect(&name).await.map_err(|e| match e {
        DockerError::NotFound | DockerError::NotManaged => {
            invalid("this account has no such container")
        }
        other => docker_error(&other),
    })?;
    let mine = info
        .labels
        .get(dhpanel_docker_manager::TENANT_LABEL)
        .is_some_and(|t| *t == tenant_id.to_string());
    if !mine {
        // The same answer as for a container that is not there: nothing is given away.
        return Err(invalid("this account has no such container"));
    }
    context
        .docker
        .restart(&name, GRACE)
        .await
        .map_err(|e| docker_error(&e))?;
    let state = context
        .docker
        .inspect(&name)
        .await
        .map(|i| i.status)
        .unwrap_or_default();
    Ok(Output::ContainerRestarted {
        name: name.as_str().to_owned(),
        state,
    })
}
