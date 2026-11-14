//! The PHP versions of a tenant for real (T06.09): the main PHP container changes
//! version, another version runs next to it for a site that asks for it, and the
//! web server reaches each through its own configuration. Ignored by default;
//! needs the login user's rootless Docker.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use dhpanel_docker_manager::{ContainerSpec, DockerManager, ImageRef, Name, Network, VolumeMount};
use dhpanel_domain::TenantUsername;
use dhpanel_node_agent::ops::{
    tenant::{TenantContext, provision_tenant_stack, set_php_versions},
    vhost::attach_domain,
};
use dhpanel_node_protocol::{ErrorCode, Operation, Output};
use dhpanel_tenant_stack::{conf_volume_name, files_volume_name, network_name};
use uuid::Uuid;

const BUSYBOX: &str =
    "busybox@sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec";

fn docker_socket() -> PathBuf {
    let uid = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .and_then(|l| l.split_whitespace().nth(1).map(str::to_owned))
        .unwrap();
    PathBuf::from(format!("/run/user/{uid}/docker.sock"))
}

/// Runs a shell snippet in a throw-away container on the tenant's network, with
/// the files volume at /data (as root of the user namespace); returns its exit code.
async fn run(docker: &DockerManager, owner: &TenantUsername, n: u32, script: &str) -> i64 {
    let mut spec = ContainerSpec::new(
        Name::parse(&format!("dh-{owner}-run{n}")).unwrap(),
        ImageRef::parse(BUSYBOX).unwrap(),
    );
    spec.command = vec!["sh".into(), "-c".into(), script.into()];
    spec.network = Network::Named(network_name(owner).unwrap());
    spec.volumes.push(VolumeMount {
        volume: files_volume_name(owner).unwrap(),
        target: "/data".into(),
        read_only: false,
    });
    docker.pull_image(&spec.image).await.unwrap();
    docker.create_container(&spec).await.unwrap();
    docker.start(&spec.name).await.unwrap();
    let mut exit = -1;
    for _ in 0..300 {
        let info = docker.inspect(&spec.name).await.unwrap();
        if !info.running && info.status != "created" {
            exit = info.exit_code.unwrap_or(-1);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    docker.remove(&spec.name, true).await.unwrap();
    exit
}

/// Whether the site `host` answers with a PHP version starting with `prefix`.
async fn serves(
    docker: &DockerManager,
    owner: &TenantUsername,
    n: u32,
    host: &str,
    prefix: &str,
) -> bool {
    for attempt in 0..20 {
        let script = format!(
            "wget -q -O - --header 'Host: {host}' http://dh-{owner}-web:8080/index.php | grep -q '^{prefix}'"
        );
        if run(docker, owner, n * 100 + attempt, &script).await == 0 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

#[tokio::test]
#[ignore = "needs rootless Docker (see the file header)"]
async fn the_main_php_changes_version_another_runs_beside_it_and_extras_are_removed() {
    let socket = docker_socket();
    if !socket.exists() {
        eprintln!("skipped: no Docker at {}", socket.display());
        return;
    }
    let docker = DockerManager::connect(&socket).unwrap();
    let root = std::env::temp_dir().join(format!("dh-php-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let context = Arc::new(TenantContext {
        helper_socket: "/nonexistent".into(),
        tenants_root: "/nonexistent".into(),
        docker: docker.clone(),
        vhosts_root: root.clone(),
    });
    let name = format!("q{}x", std::process::id());
    let owner = TenantUsername::parse(&name).unwrap();
    let uid = 10_089;
    let tenant_id = Uuid::now_v7();
    let limits = |php_version: &str, extra: &[&str]| Operation::SetPhpVersions {
        tenant_id,
        username: name.clone(),
        uid,
        cpu_millis: Some(1000),
        memory_bytes: Some(768 << 20),
        pids: Some(256),
        php_version: php_version.into(),
        extra_php: extra.iter().map(|v| (*v).to_owned()).collect(),
    };

    // What create_tenant makes (the helper part is not needed here): the network
    // and the files volume, with a page that says which PHP runs it.
    docker
        .ensure_network(&network_name(&owner).unwrap(), false)
        .await
        .unwrap();
    docker
        .create_volume(&files_volume_name(&owner).unwrap())
        .await
        .unwrap();
    let seeded = run(
        &docker,
        &owner,
        1,
        "echo '<?php echo PHP_VERSION;' > /data/index.php && chmod 755 /data && chmod 644 /data/index.php",
    )
    .await;
    assert_eq!(seeded, 0);

    // A new stack runs the version new accounts start with; two sites on it.
    let done = provision_tenant_stack(
        context.clone(),
        Operation::ProvisionTenantStack {
            tenant_id,
            username: name.clone(),
            uid,
            cpu_millis: Some(1000),
            memory_bytes: Some(768 << 20),
            pids: Some(256),
        },
    )
    .await
    .unwrap();
    assert!(matches!(done, Output::StackProvisioned { .. }));
    let site = |domain: &str, php: &str| Operation::AttachDomain {
        username: name.clone(),
        domain: domain.into(),
        config: format!(
            "server {{\n    listen 8080;\n    server_name {domain};\n    root /var/www/html;\n    location ~ \\.php$ {{\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n        fastcgi_pass {php};\n    }}\n}}\n"
        ),
    };
    attach_domain(
        context.clone(),
        site("one.example.com", &format!("dh-{name}-php:9000")),
    )
    .await
    .unwrap();
    assert!(serves(&docker, &owner, 2, "one.example.com", "8.4.").await);

    // The main container moves to 8.3: its files stay, the site follows.
    let moved = set_php_versions(context.clone(), limits("8.3", &[]))
        .await
        .unwrap();
    assert_eq!(
        moved,
        Output::PhpVersionsSet {
            containers: vec![format!("dh-{name}-php")],
            removed: vec![]
        }
    );
    assert!(serves(&docker, &owner, 3, "one.example.com", "8.3.").await);
    // Saying it again changes nothing.
    assert_eq!(
        set_php_versions(context.clone(), limits("8.3", &[]))
            .await
            .unwrap(),
        moved
    );

    // A second site on another version gets a container of its own; the first is
    // untouched and the plan's PHP share is divided between the two.
    attach_domain(
        context.clone(),
        site("two.example.com", &format!("dh-{name}-php-8-2:9000")),
    )
    .await
    .err();
    let both = set_php_versions(context.clone(), limits("8.3", &["8.2"]))
        .await
        .unwrap();
    assert_eq!(
        both,
        Output::PhpVersionsSet {
            containers: vec![format!("dh-{name}-php"), format!("dh-{name}-php-8-2")],
            removed: vec![]
        }
    );
    attach_domain(
        context.clone(),
        site("two.example.com", &format!("dh-{name}-php-8-2:9000")),
    )
    .await
    .unwrap();
    assert!(serves(&docker, &owner, 4, "two.example.com", "8.2.").await);
    assert!(serves(&docker, &owner, 5, "one.example.com", "8.3.").await);
    let main = docker
        .inspect(&Name::parse(&format!("dh-{name}-php")).unwrap())
        .await
        .unwrap();
    let extra = docker
        .inspect(&Name::parse(&format!("dh-{name}-php-8-2")).unwrap())
        .await
        .unwrap();
    assert_eq!(main.limits.memory_bytes, extra.limits.memory_bytes);
    assert_eq!(main.limits.memory_bytes, (768u64 << 20) * 75 / 100 / 2);

    // A version nobody offers, and a stack that was never made, are refused.
    let err = set_php_versions(context.clone(), limits("7.4", &[]))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    let err = set_php_versions(
        context.clone(),
        Operation::SetPhpVersions {
            tenant_id,
            username: format!("{name}z"),
            uid,
            cpu_millis: None,
            memory_bytes: None,
            pids: None,
            php_version: "8.4".into(),
            extra_php: vec![],
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);

    // The version nobody uses any more is removed; the other site goes on.
    let back = set_php_versions(context.clone(), limits("8.3", &[]))
        .await
        .unwrap();
    assert_eq!(
        back,
        Output::PhpVersionsSet {
            containers: vec![format!("dh-{name}-php")],
            removed: vec![format!("dh-{name}-php-8-2")]
        }
    );
    assert!(
        docker
            .inspect(&Name::parse(&format!("dh-{name}-php-8-2")).unwrap())
            .await
            .is_err()
    );
    assert!(serves(&docker, &owner, 6, "one.example.com", "8.3.").await);

    // Clean up.
    for container in [format!("dh-{name}-web"), format!("dh-{name}-php")] {
        let _ = docker.remove(&Name::parse(&container).unwrap(), true).await;
    }
    let _ = docker
        .remove_volume(&conf_volume_name(&owner).unwrap())
        .await;
    let _ = docker
        .remove_volume(&files_volume_name(&owner).unwrap())
        .await;
    let _ = docker.remove_network(&network_name(&owner).unwrap()).await;
    let _ = std::fs::remove_dir_all(&root);
}
