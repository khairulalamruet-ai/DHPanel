//! The node's shared database server for real (T07.01): MariaDB in Docker, reached
//! from a tenant's own network and from nowhere else. Ignored by default; needs the
//! login user's rootless Docker and the image (the first run pulls it).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, process::Command, sync::Arc, time::Duration};

use dhpanel_docker_manager::{ContainerSpec, DockerManager, ImageRef, Name, Network};
use dhpanel_domain::TenantUsername;
use dhpanel_node_agent::ops::{
    db::{CONTAINER, allow_tenant_databases, ensure_db_server, port},
    tenant::TenantContext,
};
use dhpanel_node_protocol::{ErrorCode, Operation, Output};
use dhpanel_tenant_stack::network_name;

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

/// Runs a shell snippet in a throw-away container on `network`; returns its exit code.
async fn probe(docker: &DockerManager, network: &Name, n: u32, script: &str) -> i64 {
    let mut spec = ContainerSpec::new(
        Name::parse(&format!("dh-dbprobe-{}-{n}", std::process::id())).unwrap(),
        ImageRef::parse(BUSYBOX).unwrap(),
    );
    spec.command = vec!["sh".into(), "-c".into(), script.into()];
    spec.network = Network::Named(network.clone());
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

#[tokio::test]
#[ignore = "needs rootless Docker (see the file header)"]
async fn the_database_server_is_reached_from_its_tenants_network_and_nowhere_else() {
    let socket = docker_socket();
    if !socket.exists() {
        eprintln!("skipped: no Docker at {}", socket.display());
        return;
    }
    let docker = DockerManager::connect(&socket).unwrap();
    let root = std::env::temp_dir().join(format!("dh-db-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let context = Arc::new(TenantContext {
        helper_socket: "/nonexistent".into(),
        tenants_root: "/nonexistent".into(),
        docker: docker.clone(),
        vhosts_root: root.join("vhosts"),
    });
    let (a, b) = (
        TenantUsername::parse(&format!("da{}x", std::process::id())).unwrap(),
        TenantUsername::parse(&format!("db{}x", std::process::id())).unwrap(),
    );
    for t in [&a, &b] {
        docker
            .ensure_network(&network_name(t).unwrap(), false)
            .await
            .unwrap();
    }

    // The server starts, makes its password once, and is there for the next call.
    let ready = ensure_db_server(context.clone(), Operation::EnsureDbServer)
        .await
        .unwrap();
    let Output::DbServerReady { port: p, created } = ready else {
        panic!("{ready:?}");
    };
    assert_eq!(p, port());
    let password = std::fs::read_to_string(root.join("secrets/mariadb-root")).unwrap();
    assert_eq!(password.len(), 64);
    let again = ensure_db_server(context.clone(), Operation::EnsureDbServer)
        .await
        .unwrap();
    assert_eq!(
        again,
        Output::DbServerReady {
            port: p,
            created: false
        }
    );
    let _ = created;

    // Published on the node's loopback address and nowhere else.
    let published = Command::new("docker")
        .args(["port", CONTAINER, "3306/tcp"])
        .output()
        .unwrap();
    let published = String::from_utf8_lossy(&published.stdout).into_owned();
    assert!(
        published.lines().all(|l| l.starts_with("127.0.0.1:")),
        "{published:?}"
    );
    assert!(!published.trim().is_empty());

    // The tenant that was let in reaches it as `db`; the other tenant does not.
    let done = allow_tenant_databases(
        context.clone(),
        Operation::AllowTenantDatabases {
            username: a.as_str().to_owned(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        done,
        Output::TenantDbAllowed {
            network: network_name(&a).unwrap().as_str().to_owned()
        }
    );
    // (and doing it again is fine)
    allow_tenant_databases(
        context.clone(),
        Operation::AllowTenantDatabases {
            username: a.as_str().to_owned(),
        },
    )
    .await
    .unwrap();
    let reach = "nc -z -w 3 db 3306";
    assert_eq!(
        probe(&docker, &network_name(&a).unwrap(), 1, reach).await,
        0
    );
    assert_ne!(
        probe(&docker, &network_name(&b).unwrap(), 2, reach).await,
        0
    );

    // A tenant with no network here is refused.
    let err = allow_tenant_databases(
        context.clone(),
        Operation::AllowTenantDatabases {
            username: "nobodyhere".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);

    // Clean up (the data volume too: this is a test server).
    let _ = docker.remove(&Name::parse(CONTAINER).unwrap(), true).await;
    let _ = docker
        .remove_volume(&Name::parse("dh-db-data").unwrap())
        .await;
    for t in [&a, &b] {
        let _ = docker.remove_network(&network_name(t).unwrap()).await;
    }
    let _ = docker
        .remove_network(&Name::parse("dh-db-net").unwrap())
        .await;
    let _ = std::fs::remove_dir_all(&root);
}
