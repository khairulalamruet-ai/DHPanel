//! attach_domain / detach_domain against a real nginx container started from the
//! stack template (T06.04). Ignored by default; needs the login user's rootless
//! Docker (run it with deploy/dev/vhost-live.sh).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, process::Command, sync::Arc, time::Duration};

use dhpanel_docker_manager::{DockerManager, PublishedPort};
use dhpanel_domain::{TenantUsername, UidRange};
use dhpanel_node_agent::ops::{
    tenant::TenantContext,
    vhost::{attach_domain, conf_dir, detach_domain},
};
use dhpanel_node_protocol::{ErrorCode, Operation, Output};
use dhpanel_tenant_stack::{
    Allowlist, PlanResources, Tenant, conf_volume_name, files_volume_name, network_name, render,
};
use uuid::Uuid;

const PORT: u16 = 18_091;

fn docker_socket() -> PathBuf {
    let uid = std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("Uid:"))
        .and_then(|l| l.split_whitespace().nth(1).map(str::to_owned))
        .unwrap();
    PathBuf::from(format!("/run/user/{uid}/docker.sock"))
}

/// What the site answers on the published port: `(curl exit code, body)`.
fn fetch() -> (i32, String) {
    let out = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "5",
            &format!("http://127.0.0.1:{PORT}/"),
        ])
        .output()
        .expect("curl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
    )
}

async fn answers(wanted: &str) -> bool {
    for _ in 0..30 {
        if fetch().1 == wanted {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

fn site(says: &str) -> String {
    format!("server {{\n    listen 8080;\n    location / {{ return 200 \"{says}\\n\"; }}\n}}\n")
}

#[tokio::test]
#[ignore = "needs rootless Docker (see the file header)"]
async fn a_refused_configuration_is_rolled_back_and_the_site_keeps_running() {
    let socket = docker_socket();
    if !socket.exists() {
        eprintln!("skipped: no Docker at {}", socket.display());
        return;
    }
    let docker = DockerManager::connect(&socket).unwrap();
    let root = std::env::temp_dir().join(format!("dh-vhost-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let context = Arc::new(TenantContext {
        helper_socket: "/nonexistent".into(),
        tenants_root: "/nonexistent".into(),
        docker: docker.clone(),
        vhosts_root: root.clone(),
    });
    let username = format!("v{}x", std::process::id());
    let owner = TenantUsername::parse(&username).unwrap();
    let uid = UidRange::DEFAULT.allocate(10_088..10_089).unwrap();
    let list = Allowlist::embedded().unwrap();
    let stack = render(
        &Tenant {
            id: Uuid::now_v7(),
            username: &owner,
            uid,
            resources: PlanResources {
                cpu_millis: Some(500),
                memory_bytes: Some(512 << 20),
                pids: Some(128),
            },
        },
        &list,
    )
    .unwrap();
    let mut web = stack
        .services
        .into_iter()
        .find(|s| s.role.as_deref() == Some("web"))
        .unwrap();
    web.ports = vec![PublishedPort {
        container_port: 8080,
        host_port: PORT,
        host_ip: "127.0.0.1".parse().unwrap(),
    }];
    // What provisioning makes: the network, the volumes, the web container.
    docker
        .ensure_network(&network_name(&owner).unwrap(), false)
        .await
        .unwrap();
    docker
        .create_volume(&files_volume_name(&owner).unwrap())
        .await
        .unwrap();
    let dir = conf_dir(&root, &owner).unwrap();
    docker
        .create_bind_volume(&conf_volume_name(&owner).unwrap(), &dir, &root)
        .await
        .unwrap();
    docker.pull_image(&web.image).await.unwrap();
    docker.create_container(&web).await.unwrap();
    docker.start(&web.name).await.unwrap();
    let file = dir.join("example.com.conf");
    let attach = |config: String| Operation::AttachDomain {
        username: username.clone(),
        domain: "example.com".into(),
        config,
    };

    // A good configuration is served.
    let done = attach_domain(context.clone(), attach(site("one")))
        .await
        .unwrap();
    assert_eq!(done, Output::DomainAttached { applied: true });
    assert!(answers("one").await, "{:?}", fetch());

    // A configuration nginx refuses: an error naming the reason, the old file
    // back, the site still answering with the old content.
    let err = attach_domain(
        context.clone(),
        attach("server {\n    listen 8080;\n    nonsense_directive on;\n}\n".into()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert!(!err.retryable);
    assert!(
        err.message.contains("nonsense_directive"),
        "{}",
        err.message
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), site("one"));
    assert_eq!(fetch().1, "one");
    // A second site whose configuration clashes with the first one is refused too.
    let err = attach_domain(
        context.clone(),
        Operation::AttachDomain {
            username: username.clone(),
            domain: "other.example.com".into(),
            config: site("two"),
        },
    )
    .await;
    // Same `listen 8080` without a name is fine for nginx (only a warning): it
    // works, which also shows a second domain is its own file.
    assert!(err.is_ok(), "{err:?}");
    assert!(dir.join("other.example.com.conf").exists());
    detach_domain(
        context.clone(),
        Operation::DetachDomain {
            username: username.clone(),
            domain: "other.example.com".into(),
        },
    )
    .await
    .unwrap();

    // A refused first configuration leaves no file at all.
    let err = attach_domain(
        context.clone(),
        Operation::AttachDomain {
            username: username.clone(),
            domain: "bad.example.com".into(),
            config: "server { listen 8080; broken }".into(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert!(!dir.join("bad.example.com.conf").exists());

    // A new good one replaces the old.
    attach_domain(context.clone(), attach(site("three")))
        .await
        .unwrap();
    assert!(answers("three").await, "{:?}", fetch());

    // Removing the domain removes the site (and doing it again is fine).
    let gone = Operation::DetachDomain {
        username: username.clone(),
        domain: "example.com".into(),
    };
    assert_eq!(
        detach_domain(context.clone(), gone.clone()).await.unwrap(),
        Output::DomainDetached { applied: true }
    );
    assert!(!file.exists());
    for _ in 0..30 {
        if fetch().1.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(fetch().1, "", "nothing answers any more");
    detach_domain(context.clone(), gone).await.unwrap();

    // Keeping a list removes every other domain's file, and the site with it.
    attach_domain(context.clone(), attach(site("kept")))
        .await
        .unwrap();
    let other = |config: String| Operation::AttachDomain {
        username: username.clone(),
        domain: "gone.example.com".into(),
        config,
    };
    attach_domain(
        context.clone(),
        other("server {\n    listen 8081;\n    location / { return 200 \"x\\n\"; }\n}\n".into()),
    )
    .await
    .unwrap();
    assert!(dir.join("gone.example.com.conf").exists());
    let kept = dhpanel_node_agent::ops::vhost::keep_domains(
        context.clone(),
        Operation::KeepDomains {
            username: username.clone(),
            domains: vec!["example.com".into()],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        kept,
        Output::DomainsKept {
            removed: 1,
            applied: true
        }
    );
    assert!(!dir.join("gone.example.com.conf").exists());
    assert!(file.exists());
    assert!(answers("kept").await, "{:?}", fetch());
    // With the web container stopped the file is kept and not applied.
    docker
        .stop(&web.name, Duration::from_secs(5))
        .await
        .unwrap();
    let done = attach_domain(context.clone(), attach(site("later")))
        .await
        .unwrap();
    assert_eq!(done, Output::DomainAttached { applied: false });
    assert_eq!(std::fs::read_to_string(&file).unwrap(), site("later"));
    // ... and is served as soon as the container starts.
    docker.start(&web.name).await.unwrap();
    assert!(answers("later").await, "{:?}", fetch());

    // Clean up.
    let _ = docker.remove(&web.name, true).await;
    let _ = docker
        .remove_volume(&conf_volume_name(&owner).unwrap())
        .await;
    let _ = docker
        .remove_volume(&files_volume_name(&owner).unwrap())
        .await;
    let _ = docker.remove_network(&network_name(&owner).unwrap()).await;
    let _ = std::fs::remove_dir_all(&root);
}
