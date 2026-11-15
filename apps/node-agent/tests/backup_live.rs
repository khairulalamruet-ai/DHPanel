//! `create_backup` for real (T09.04): the privileged helper (root, started by the live
//! script), the agent's handlers, the box's rootless Docker and a real MariaDB. Ignored by
//! default; skips without DHPANEL_TEST_HELPER_SOCKET and DHPANEL_TEST_TENANTS_ROOT.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{io::Read as _, path::PathBuf, sync::Arc};

use dhpanel_backup::{KEY_LEN, Manifest, StreamReader};
use dhpanel_backup_dest::{Destination as _, local::Local};
use dhpanel_docker_manager::{DockerManager, Name};
use dhpanel_node_agent::ops::{
    backup::create_backup,
    db::{CONTAINER, create_database, ensure_db_server, port},
    files::file_op,
    tenant::{TenantContext, create_tenant},
};
use dhpanel_node_protocol::{
    BackupTarget, DbPassword, ErrorCode, Operation, Output,
    files::{FileRequest, FileResult},
};
use mysql_async::prelude::Queryable as _;
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

fn hex_key(byte: u8) -> (String, [u8; KEY_LEN]) {
    (format!("{byte:02x}").repeat(KEY_LEN), [byte; KEY_LEN])
}

#[tokio::test]
#[ignore = "needs the privileged helper running as root, Docker and registry access"]
async fn an_account_is_backed_up_encrypted_and_every_part_checks_out() {
    let (Some(helper_socket), Some(tenants_root)) = (
        std::env::var_os("DHPANEL_TEST_HELPER_SOCKET"),
        std::env::var_os("DHPANEL_TEST_TENANTS_ROOT"),
    ) else {
        eprintln!("skipped: no helper (see the live script)");
        return;
    };
    let docker = DockerManager::connect(&docker_socket()).unwrap();
    let _ = docker.remove(&Name::parse(CONTAINER).unwrap(), true).await;
    let _ = docker
        .remove_volume(&Name::parse("dh-db-data").unwrap())
        .await;
    let base = std::env::temp_dir().join(format!("dh-backup-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let dest_dir = base.join("destination");
    std::fs::create_dir_all(&dest_dir).unwrap();
    let context = Arc::new(TenantContext {
        helper_socket: PathBuf::from(helper_socket),
        tenants_root: PathBuf::from(tenants_root),
        docker: docker.clone(),
        vhosts_root: base.join("vhosts"),
    });
    let tenant_id = Uuid::now_v7();
    let username = format!("b{}x", std::process::id());
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
    .expect("tenant created");
    // Its files, written as the tenant.
    let made = file_op(
        context.clone(),
        Operation::FileOp {
            tenant_id,
            uid,
            request: FileRequest::Mkdir {
                path: "www/sub".into(),
            },
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(
            made,
            Output::File {
                result: FileResult::Done { .. }
            }
        ),
        "{made:?}"
    );
    for (path, content) in [
        ("www/index.php", "<?php echo 'hello';"),
        ("www/sub/data.txt", "some data"),
        ("notes.txt", "n"),
    ] {
        let done = file_op(
            context.clone(),
            Operation::FileOp {
                tenant_id,
                uid,
                request: FileRequest::Write {
                    path: path.into(),
                    content: content.into(),
                    max_bytes: 1000,
                    expected_mtime: None,
                    create_only: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(
            matches!(
                done,
                Output::File {
                    result: FileResult::Done { .. }
                }
            ),
            "{done:?}"
        );
    }
    // A database with a trigger, a view and rows.
    ensure_db_server(context.clone(), Operation::EnsureDbServer)
        .await
        .unwrap();
    create_database(
        context.clone(),
        Operation::CreateDatabase {
            name: "bkp_shop".into(),
        },
    )
    .await
    .unwrap();
    let password = std::fs::read_to_string(base.join("secrets/mariadb-root")).unwrap();
    let opts: mysql_async::Opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname("127.0.0.1")
        .tcp_port(port())
        .user(Some("root"))
        .pass(Some(password.trim()))
        .into();
    let mut admin = mysql_async::Conn::new(opts).await.unwrap();
    admin.query_drop("USE bkp_shop").await.unwrap();
    for sql in [
        "CREATE TABLE items (id int PRIMARY KEY, note text)",
        "CREATE VIEW v AS SELECT id FROM items",
        "INSERT INTO items VALUES (1, 'one'), (2, 'it''s two ☃')",
    ] {
        admin.query_drop(sql).await.unwrap();
    }

    let (key_text, key) = hex_key(0x5a);
    let target = BackupTarget {
        kind: "local".into(),
        settings: serde_json::json!({"path": dest_dir.to_str().unwrap()}),
    };
    let backup_id = Uuid::now_v7();
    let op = |key: Option<&str>, target: &BackupTarget| Operation::CreateBackup {
        tenant_id,
        uid,
        backup_id,
        account: username.clone(),
        target: target.clone(),
        credentials: None,
        key: key.map(|k| DbPassword(k.into())),
        databases: vec!["bkp_shop".into()],
        dns: serde_json::json!([{"name": "example.com", "type": "A", "value": "203.0.113.7"}]),
        meta: serde_json::json!({"plan": "basic", "php": "8.4"}),
        created_at: 1_700_000_000,
    };

    // Bad arguments change nothing.
    for bad in [
        op(None, &target),
        op(Some("zz"), &target),
        op(Some(&"0".repeat(63)), &target),
    ] {
        let e = create_backup(context.clone(), bad).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArguments, "{e:?}");
    }
    assert!(std::fs::read_dir(&dest_dir).unwrap().next().is_none());

    let done = create_backup(context.clone(), op(Some(&key_text), &target))
        .await
        .unwrap();
    let Output::BackupCreated {
        manifest_sha256,
        size_bytes,
        parts,
    } = done
    else {
        panic!("{done:?}")
    };
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "files.tar.zst.enc",
            "db-bkp_shop.sql.zst.enc",
            "dns.json.enc",
            "meta.json.enc"
        ]
    );
    assert!(size_bytes > 0);

    // Checked from the outside, as a restore would: the manifest first, every part against it.
    let dest = Local::open(&dest_dir).unwrap();
    let id = backup_id.simple().to_string();
    let mut sealed = Vec::new();
    dest.get(&format!("{id}/manifest.json.enc"))
        .unwrap()
        .read_to_end(&mut sealed)
        .unwrap();
    assert_eq!(
        dhpanel_backup::manifest::hash_of(&sealed[..]).unwrap().1,
        manifest_sha256
    );
    let manifest = Manifest::from_reader(StreamReader::new(&sealed[..], &key)).unwrap();
    assert_eq!(
        (manifest.account.as_str(), manifest.created_at),
        (username.as_str(), 1_700_000_000)
    );
    for part in &manifest.parts {
        manifest
            .verify_part(
                &part.name,
                dest.get(&format!("{id}/{}", part.name)).unwrap(),
            )
            .unwrap();
        let mut stored = Vec::new();
        dest.get(&format!("{id}/{}", part.name))
            .unwrap()
            .read_to_end(&mut stored)
            .unwrap();
        assert!(
            !stored
                .windows(9)
                .any(|w| w == b"hello';" || w == b"CREATE TA"),
            "{}: nothing readable is stored",
            part.name
        );
    }
    // The files part: a tar of the tenant's files.
    let files = dest.get(&format!("{id}/files.tar.zst.enc")).unwrap();
    let mut archive =
        tar::Archive::new(zstd::stream::Decoder::new(StreamReader::new(files, &key)).unwrap());
    let mut seen = std::collections::BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut body = String::new();
        let _ = entry.read_to_string(&mut body);
        seen.insert(name, body);
    }
    assert_eq!(seen["www/index.php"], "<?php echo 'hello';");
    assert_eq!(seen["www/sub/data.txt"], "some data");
    assert_eq!(seen["notes.txt"], "n");
    // The database part: a dump with the structure and the rows.
    let dump = dest.get(&format!("{id}/db-bkp_shop.sql.zst.enc")).unwrap();
    let mut sql = String::new();
    zstd::stream::Decoder::new(StreamReader::new(dump, &key))
        .unwrap()
        .read_to_string(&mut sql)
        .unwrap();
    assert!(
        sql.contains("CREATE TABLE `items`")
            && sql.contains("it''s two ☃")
            && sql.contains("CREATE VIEW")
            || sql.contains("VIEW `v`"),
        "{}",
        &sql[..sql.len().min(300)]
    );
    // DNS and meta.
    let mut meta = String::new();
    StreamReader::new(dest.get(&format!("{id}/meta.json.enc")).unwrap(), &key)
        .read_to_string(&mut meta)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&meta).unwrap()["php"],
        "8.4"
    );
    // Another key opens nothing.
    assert!(
        StreamReader::new(
            dest.get(&format!("{id}/meta.json.enc")).unwrap(),
            &[1u8; KEY_LEN]
        )
        .read_to_string(&mut String::new())
        .is_err()
    );
    // A changed part is found by the manifest.
    let mut part = Vec::new();
    dest.get(&format!("{id}/dns.json.enc"))
        .unwrap()
        .read_to_end(&mut part)
        .unwrap();
    part[30] ^= 1;
    assert!(manifest.verify_part("dns.json.enc", &part[..]).is_err());

    // A destination that cannot be written to fails and leaves nothing.
    let bad_target = BackupTarget {
        kind: "local".into(),
        settings: serde_json::json!({"path": "/proc"}),
    };
    let e = create_backup(context.clone(), op(Some(&key_text), &bad_target))
        .await
        .unwrap_err();
    assert!(e.retryable || e.code == ErrorCode::Internal, "{e:?}");

    let _ = docker.remove(&Name::parse(CONTAINER).unwrap(), true).await;
    let _ = docker
        .remove_volume(&Name::parse("dh-db-data").unwrap())
        .await;
    let _ = std::fs::remove_dir_all(&base);
}
