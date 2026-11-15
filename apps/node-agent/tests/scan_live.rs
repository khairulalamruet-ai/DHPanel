//! The malware scan for real (T11.09): ClamAV in containers (needs Docker, the network for the
//! image and the signatures, and a few minutes the first time). Ignored by default.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{path::PathBuf, sync::Arc};

use dhpanel_docker_manager::DockerManager;
use dhpanel_node_agent::ops::{
    scan::{scan_tenant, update_malware_signatures},
    tenant::TenantContext,
};
use dhpanel_node_protocol::{ErrorCode, Operation, Output};
use uuid::Uuid;

const EICAR: &str = "X5O!P%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

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
async fn a_planted_test_file_is_found_and_clean_files_and_folders_are_not() {
    // A fixed place: the signatures (owned by the scanner's user, so not ours to remove) are
    // kept between runs, which saves fetching them again.
    let base = std::env::temp_dir().join("dh-scan-live");
    let tenants = base.join("tenants");
    let _ = std::fs::remove_dir_all(&tenants);
    let state = base.join("state");
    std::fs::create_dir_all(&tenants).unwrap();
    std::fs::create_dir_all(state.join("vhosts")).unwrap();
    let id = Uuid::now_v7();
    let www = tenants.join(id.to_string()).join("www");
    std::fs::create_dir_all(www.join("clean")).unwrap();
    std::fs::write(www.join("eicar.com"), EICAR).unwrap();
    std::fs::write(www.join("index.html"), "<p>hello</p>").unwrap();
    std::fs::write(www.join("clean").join("a.txt"), "fine").unwrap();
    let docker = DockerManager::connect(&docker_socket()).unwrap();
    let _ = docker
        .remove_volume(&dhpanel_docker_manager::Name::parse("dh-clamav-db").unwrap())
        .await;
    let context = Arc::new(TenantContext {
        helper_socket: base.join("no-helper.sock"),
        tenants_root: tenants.clone(),
        docker,
        vhosts_root: state.join("vhosts"),
    });
    let scan = |folder: Option<&str>| {
        let context = context.clone();
        let op = Operation::ScanTenant {
            tenant_id: id,
            uid: 10_000,
            folder: folder.map(str::to_owned),
            quarantine: false,
        };
        async move { scan_tenant(context, op).await }
    };
    // The first scan fetches the signatures by itself.
    let out = scan(None).await.unwrap();
    let Output::Scanned {
        scanned,
        infected,
        more,
    } = out
    else {
        panic!("{out:?}")
    };
    assert!(!more && scanned >= 3, "{scanned}");
    assert_eq!(infected.len(), 1, "{infected:?}");
    assert_eq!(infected[0].path, "www/eicar.com");
    assert!(infected[0].signature.contains("Eicar"), "{infected:?}");
    assert!(!infected[0].quarantined);
    // A clean folder finds nothing; a folder outside the files is refused.
    let Output::Scanned { infected, .. } = scan(Some("clean")).await.unwrap() else {
        panic!()
    };
    assert!(infected.is_empty());
    for bad in ["../x", "/etc", "a/../../b"] {
        let e = scan(Some(bad)).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArguments, "{bad}");
    }
    // Updating again works (nothing new to fetch is fine).
    let out = update_malware_signatures(context.clone(), Operation::UpdateMalwareSignatures)
        .await
        .unwrap();
    assert!(matches!(out, Output::SignaturesUpdated { .. }), "{out:?}");
    // Nothing is left behind.
    let left = context.docker.list().await.unwrap();
    assert!(
        left.iter()
            .all(|n| !n.starts_with("dh-scan-") && n != "dh-clamav-update"),
        "{left:?}"
    );
    let _ = std::fs::remove_dir_all(&tenants);
}
