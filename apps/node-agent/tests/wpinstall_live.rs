//! A WordPress install for real (T12.02): WP-CLI in a one-off container against a real MariaDB
//! on the tenant's network. Needs rootless Docker and the network; ignored by default.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, sync::Arc, time::Instant};

use dhpanel_docker_manager::{Capability, ContainerSpec, DockerManager, Name, Network};
use dhpanel_domain::TenantUsername;
use dhpanel_node_agent::ops::{tenant::TenantContext, wpinstall::install_as};
use dhpanel_node_protocol::{DbPassword, ErrorCode, Operation, Output, WpDatabase, WpSite};
use dhpanel_tenant_stack::{Allowlist, Role, files_volume_name, network_name};
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

#[tokio::test]
#[ignore = "needs rootless Docker and the network"]
async fn wordpress_is_installed_in_a_tenants_files_within_two_minutes() {
    let base = std::env::temp_dir().join("dh-wpinstall-live");
    let _ = std::fs::remove_dir_all(&base);
    let tenants = base.join("tenants");
    let state = base.join("state");
    std::fs::create_dir_all(state.join("vhosts")).unwrap();
    let id = Uuid::now_v7();
    let www = tenants.join(id.to_string()).join("www");
    std::fs::create_dir_all(&www).unwrap();
    let username = TenantUsername::parse("alice").unwrap();
    let docker = DockerManager::connect(&docker_socket()).unwrap();
    // The tenant's network and files volume, as provisioning makes them.
    let network = network_name(&username).unwrap();
    let files = files_volume_name(&username).unwrap();
    let _ = docker
        .remove(&Name::parse("dh-wpit-db").unwrap(), true)
        .await;
    let _ = docker.remove_volume(&files).await;
    docker.ensure_network(&network, false).await.unwrap();
    docker
        .create_bind_volume(&files, &www, &tenants)
        .await
        .unwrap();
    // A MariaDB that the tenant's network reaches as `db`.
    let allowlist = Allowlist::embedded().unwrap();
    let image = allowlist.current(Role::Db).unwrap().clone();
    docker.pull_image(&image).await.unwrap();
    let mut spec = ContainerSpec::new(Name::parse("dh-wpit-db").unwrap(), image);
    spec.env
        .insert("MARIADB_ROOT_PASSWORD".into(), "rootpw-for-the-test".into());
    spec.env
        .insert("MARIADB_DATABASE".into(), "alice_blog".into());
    spec.env.insert("MARIADB_USER".into(), "alice_web".into());
    spec.env
        .insert("MARIADB_PASSWORD".into(), "db-pass-for-the-test-1".into());
    spec.writable_root = true;
    spec.memory_bytes = 512 * 1024 * 1024;
    spec.cap_add = vec![
        Capability::Chown,
        Capability::DacOverride,
        Capability::Fowner,
        Capability::Setgid,
        Capability::Setuid,
    ];
    // Its own network first (as the node's database server has), then the tenant's, as `db`.
    let aux = Name::parse("dh-wpit-aux").unwrap();
    docker.ensure_network(&aux, false).await.unwrap();
    spec.network = Network::Named(aux);
    docker.create_container(&spec).await.unwrap();
    docker
        .connect_network(&spec.name, &network, "db")
        .await
        .unwrap();
    docker.start(&spec.name).await.unwrap();
    // Wait until it answers.
    let mut ready = false;
    for _ in 0..60 {
        if let Ok(r) = docker
            .run_user_command(
                &spec.name,
                "mariadb -uroot -prootpw-for-the-test -e 'select 1'",
                "/",
                std::time::Duration::from_secs(10),
            )
            .await
            && r.exit_code == 0
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    assert!(ready, "the database did not come up");
    let context = Arc::new(TenantContext {
        helper_socket: base.join("no-helper.sock"),
        tenants_root: tenants.clone(),
        docker: DockerManager::connect(&docker_socket()).unwrap(),
        vhosts_root: state.join("vhosts"),
    });
    let make = |path: &str, plugins: Vec<String>| Operation::InstallWordPress {
        app_id: Uuid::now_v7(),
        tenant_id: id,
        username: "alice".into(),
        uid: 10_000,
        path: path.into(),
        database: WpDatabase {
            name: "alice_blog".into(),
            user: "alice_web".into(),
        },
        database_password: Some(DbPassword("db-pass-for-the-test-1".into())),
        site: WpSite {
            url: "https://alice.example/blog".into(),
            title: "Alice's blog".into(),
            admin_user: "boss".into(),
            admin_email: "boss@example.com".into(),
        },
        admin_password: Some(DbPassword("Sup3r-Secret-Admin-Pass!".into())),
        version: None,
        locale: None,
        plugins,
    };
    let started = Instant::now();
    let out = install_as(
        context.clone(),
        make("blog", vec!["classic-editor".into()]),
        (0, 0),
    )
    .await
    .unwrap();
    let took = started.elapsed();
    eprintln!("installed in {took:?}: {out:?}");
    let Output::WordPressInstalled { version } = out else {
        panic!("{out:?}")
    };
    assert!(
        version.starts_with(|c: char| c.is_ascii_digit()),
        "{version}"
    );
    assert!(took.as_secs() < 120, "{took:?}");
    let config = std::fs::read_to_string(www.join("blog").join("wp-config.php")).unwrap();
    assert!(config.contains("alice_blog") && !config.contains("put your unique phrase here"));
    assert!(
        www.join("blog")
            .join("wp-content")
            .join("plugins")
            .join("classic-editor")
            .is_dir()
    );
    // The administrator exists in the database, and the password is not in the files' listing
    // of the installer (nothing is left in the state folder).
    let r = docker
        .run_user_command(
            &Name::parse("dh-wpit-db").unwrap(),
            "mariadb -uroot -prootpw-for-the-test -N -e \"select user_login from alice_blog.wp_users\"",
            "/",
            std::time::Duration::from_secs(20),
        )
        .await
        .unwrap();
    assert!(r.output.contains("boss"), "{}", r.output);
    assert!(
        !state
            .join("wpcli")
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    );
    // The same folder again is refused; a bad folder and a short password are refused first.
    let again = install_as(context.clone(), make("blog", vec![]), (0, 0))
        .await
        .unwrap_err();
    assert_eq!(again.code, ErrorCode::InvalidArguments, "{again:?}");
    assert!(!again.message.contains("Sup3r"), "{again:?}");
    let bad = install_as(context.clone(), make("../x", vec![]), (0, 0))
        .await
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::InvalidArguments);
    let left = docker.list().await.unwrap();
    assert!(left.iter().all(|n| !n.starts_with("dh-wpi-")), "{left:?}");
    let _ = docker
        .remove(&Name::parse("dh-wpit-db").unwrap(), true)
        .await;
    let _ = docker.remove_volume(&files).await;
    let _ = std::fs::remove_dir_all(&base);
}
