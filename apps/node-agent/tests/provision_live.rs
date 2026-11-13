//! `provision_tenant_stack` for real: the helper (root, started by the live
//! script), the agent's handlers and the box's rootless Docker.
//! Ignored by default; skips without DHPANEL_TEST_HELPER_SOCKET and
//! DHPANEL_TEST_TENANTS_ROOT.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, process::Command, sync::Arc};

use dhpanel_docker_manager::{DockerManager, Name};
use dhpanel_node_agent::ops::tenant::{TenantContext, create_tenant, provision_tenant_stack};
use dhpanel_node_protocol::{ErrorCode, Operation, Output};
use uuid::Uuid;

fn docker_socket() -> PathBuf {
    let uid = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .and_then(|l| l.split_whitespace().nth(1).map(str::to_owned))
        .unwrap();
    PathBuf::from(format!("/run/user/{uid}/docker.sock"))
}

/// `docker inspect --format <format> <name>` through the same rootless Docker.
fn inspect(socket: &std::path::Path, name: &str, format: &str) -> String {
    let out = Command::new("docker")
        .args(["-H", &format!("unix://{}", socket.display())])
        .args(["inspect", "--format", format, name])
        .output()
        .expect("docker CLI");
    assert!(out.status.success(), "docker inspect {name} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[tokio::test]
#[ignore = "needs the privileged helper running as root, Docker and registry access"]
async fn the_stack_runs_as_the_tenant_not_as_root_and_provisioning_can_be_repeated() {
    let (Some(helper_socket), Some(tenants_root)) = (
        std::env::var_os("DHPANEL_TEST_HELPER_SOCKET"),
        std::env::var_os("DHPANEL_TEST_TENANTS_ROOT"),
    ) else {
        eprintln!("skipped: no helper (see the live script)");
        return;
    };
    let socket = docker_socket();
    let docker = DockerManager::connect(&socket).unwrap();
    let context = Arc::new(TenantContext {
        helper_socket: PathBuf::from(helper_socket),
        tenants_root: PathBuf::from(tenants_root),
        docker: docker.clone(),
    });
    let tenant_id = Uuid::now_v7();
    let username = format!("p{}x", std::process::id());
    let uid = 10_077;
    create_tenant(
        context.clone(),
        Operation::CreateTenant {
            tenant_id,
            username: username.clone(),
            uid,
        },
    )
    .await
    .expect("tenant created");

    let provision = |memory_bytes| Operation::ProvisionTenantStack {
        tenant_id,
        username: username.clone(),
        uid,
        cpu_millis: Some(1000),
        memory_bytes,
        pids: Some(200),
    };

    // A plan too small to run the stack is refused before anything is pulled.
    let err = provision_tenant_stack(context.clone(), provision(Some(16 << 20)))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert!(!err.retryable);

    let op = provision(Some(512 << 20));
    let output = provision_tenant_stack(context.clone(), op.clone())
        .await
        .expect("stack provisioned");
    let Output::StackProvisioned { containers } = output.clone() else {
        panic!("unexpected output {output:?}")
    };
    assert_eq!(
        containers,
        [format!("dh-{username}-php"), format!("dh-{username}-web")],
        "PHP starts before the web server"
    );

    for name in &containers {
        let info = docker.inspect(&Name::parse(name).unwrap()).await.unwrap();
        assert!(info.running, "{name} is not running");
        // docker inspect: non-root, hardened.
        let user = inspect(&socket, name, "{{.Config.User}}");
        assert_eq!(user, format!("{uid}:{uid}"), "{name} must not run as root");
        assert_eq!(
            inspect(&socket, name, "{{.HostConfig.Privileged}}"),
            "false"
        );
        assert_eq!(
            inspect(&socket, name, "{{.HostConfig.ReadonlyRootfs}}"),
            "true"
        );
        assert_eq!(
            inspect(&socket, name, "{{.HostConfig.CapDrop}}"),
            "[ALL]",
            "{name} drops every capability"
        );
        // And the process really runs with that id, not just the config.
        let top = Command::new("docker")
            .args(["-H", &format!("unix://{}", socket.display())])
            .args(["exec", name, "id", "-u"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&top.stdout).trim(),
            uid.to_string(),
            "{name}: process uid"
        );
    }

    // The tenant's directory still belongs to the tenant: the images' own
    // owners and modes were not copied into it.
    {
        use std::os::unix::fs::MetadataExt as _;
        let www = PathBuf::from(std::env::var_os("DHPANEL_TEST_TENANTS_ROOT").unwrap())
            .join(tenant_id.to_string())
            .join("www");
        let meta = std::fs::metadata(&www).unwrap();
        assert_eq!(meta.mode() & 0o7777, 0o750, "mode of {www:?}");
        assert_ne!(meta.uid(), 0);
        assert_eq!(
            meta.uid(),
            std::fs::metadata(www.parent().unwrap()).unwrap().uid()
        );
    }

    // Repeating changes nothing and is not an error.
    assert_eq!(
        provision_tenant_stack(context.clone(), op).await.unwrap(),
        output
    );

    // Clean up what this test made (the script removes the directories).
    for name in &containers {
        let _ = docker.remove(&Name::parse(name).unwrap(), true).await;
    }
    let _ = docker
        .remove_volume(&Name::parse(&format!("dh-{username}-www")).unwrap())
        .await;
    let _ = docker
        .remove_network(&Name::parse(&format!("dh-{username}-net")).unwrap())
        .await;
}

#[tokio::test]
#[ignore = "needs the privileged helper running as root, Docker and registry access"]
async fn changing_the_plan_changes_the_limits_of_the_running_stack() {
    use dhpanel_node_agent::ops::tenant::set_resource_limits;
    let (Some(helper_socket), Some(tenants_root)) = (
        std::env::var_os("DHPANEL_TEST_HELPER_SOCKET"),
        std::env::var_os("DHPANEL_TEST_TENANTS_ROOT"),
    ) else {
        eprintln!("skipped: no helper (see the live script)");
        return;
    };
    let docker = DockerManager::connect(&docker_socket()).unwrap();
    let context = Arc::new(TenantContext {
        helper_socket: PathBuf::from(helper_socket),
        tenants_root: PathBuf::from(tenants_root),
        docker: docker.clone(),
    });
    let username = format!("l{}x", std::process::id());
    let limits_op = |memory_bytes, pids| Operation::SetResourceLimits {
        username: username.clone(),
        cpu_millis: Some(2000),
        memory_bytes: Some(memory_bytes),
        pids: Some(pids),
    };

    // Nothing provisioned yet: refused, not retryable.
    let err = set_resource_limits(context.clone(), limits_op(512 << 20, 200))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert!(!err.retryable);

    let tenant_id = Uuid::now_v7();
    let uid = 10_078;
    create_tenant(
        context.clone(),
        Operation::CreateTenant {
            tenant_id,
            username: username.clone(),
            uid,
        },
    )
    .await
    .unwrap();
    provision_tenant_stack(
        context.clone(),
        Operation::ProvisionTenantStack {
            tenant_id,
            username: username.clone(),
            uid,
            cpu_millis: Some(1000),
            memory_bytes: Some(256 << 20),
            pids: Some(100),
        },
    )
    .await
    .expect("stack provisioned");

    let php = Name::parse(&format!("dh-{username}-php")).unwrap();
    let web = Name::parse(&format!("dh-{username}-web")).unwrap();
    let before = docker.inspect(&php).await.unwrap();
    assert_eq!(before.limits.memory_bytes, 192 << 20, "75% of 256 MiB");

    // A bigger plan: the running containers get bigger limits, no restart.
    let started = docker.inspect(&php).await.unwrap().id;
    let output = set_resource_limits(context.clone(), limits_op(1024 << 20, 400))
        .await
        .expect("limits changed");
    assert_eq!(
        output,
        Output::ResourceLimitsSet {
            containers: vec![php.as_str().to_owned(), web.as_str().to_owned()]
        }
    );
    let after = docker.inspect(&php).await.unwrap();
    assert!(after.running, "no restart needed");
    assert_eq!(after.id, started);
    assert_eq!(after.limits.memory_bytes, 768 << 20);
    assert_eq!(after.limits.cpu_millis, 1500);
    assert_eq!(after.limits.pids_limit, 300);
    let web_after = docker.inspect(&web).await.unwrap();
    assert_eq!(web_after.limits.memory_bytes, 256 << 20);
    assert_eq!(web_after.limits.cpu_millis, 500);
    assert_eq!(web_after.limits.pids_limit, 100);

    // A smaller plan lowers them again; one that is too small is refused untouched.
    set_resource_limits(context.clone(), limits_op(300 << 20, 150))
        .await
        .unwrap();
    assert_eq!(
        docker.inspect(&php).await.unwrap().limits.memory_bytes,
        225 << 20
    );
    let err = set_resource_limits(context.clone(), limits_op(8 << 20, 150))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert_eq!(
        docker.inspect(&php).await.unwrap().limits.memory_bytes,
        225 << 20,
        "a refused plan changes nothing"
    );

    for name in [&php, &web] {
        let _ = docker.remove(name, true).await;
    }
    let _ = docker
        .remove_volume(&Name::parse(&format!("dh-{username}-www")).unwrap())
        .await;
    let _ = docker
        .remove_network(&Name::parse(&format!("dh-{username}-net")).unwrap())
        .await;
}
