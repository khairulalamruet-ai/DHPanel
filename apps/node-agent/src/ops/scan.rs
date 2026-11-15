//! Malware scans (T11.09): ClamAV, run once per job in a container of its own.
//!
//! A scan mounts one tenant's folder read-only into a fresh container with no network and
//! runs `clamscan` over it; the container is removed afterwards. Signatures live in a folder
//! of the agent's state and are brought up to date by `freshclam` in another one-off
//! container (the only one with a network). Nothing scanned ever runs: the scanner reads.
//! Infected files can be moved to the tenant's trash through the file manager's own
//! delete, so the way back is its restore.

use std::{path::PathBuf, sync::Arc, time::Duration};

use dhpanel_docker_manager::{Capability, ContainerSpec, Network, Tmpfs, VolumeMount};
use dhpanel_node_protocol::{
    AgentError, ErrorCode, Infection, Operation, Output,
    files::{FileRequest, FileResult},
};
use dhpanel_priv_helper::protocol::Action;
use dhpanel_tenant_stack::{Allowlist, Role};

use super::{
    db,
    sqlimport::failed,
    tenant::{TenantContext, ask_helper_data, docker_error},
};

const GIB: u64 = 1024 * 1024 * 1024;
/// The network the signature update uses.
pub const NETWORK: &str = "dh-clamav-net";
const SCAN_LIMIT: Duration = Duration::from_secs(45 * 60);
const UPDATE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// Most infected files listed (and moved) by one scan.
const MAX_LISTED: usize = 500;

fn state(context: &TenantContext) -> Result<PathBuf, AgentError> {
    context
        .vhosts_root
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| failed("the state folder is unknown"))
}

fn signatures_dir(context: &TenantContext) -> Result<PathBuf, AgentError> {
    Ok(state(context)?.join("clamav").join("db"))
}

fn has_signatures(dir: &std::path::Path) -> bool {
    ["main.cvd", "main.cld"]
        .iter()
        .any(|f| dir.join(f).exists())
        && ["daily.cvd", "daily.cld"]
            .iter()
            .any(|f| dir.join(f).exists())
}

async fn image(context: &TenantContext) -> Result<dhpanel_docker_manager::ImageRef, AgentError> {
    let allowlist = Allowlist::embedded().map_err(|e| failed(&format!("the image list: {e}")))?;
    let image = allowlist
        .current(Role::Clamav)
        .map_err(|e| failed(&e.to_string()))?
        .clone();
    context
        .docker
        .pull_image(&image)
        .await
        .map_err(|e| docker_error(&e))?;
    Ok(image)
}

fn base_spec(
    name: dhpanel_docker_manager::Name,
    image: dhpanel_docker_manager::ImageRef,
    command: &[&str],
) -> ContainerSpec {
    let mut spec = ContainerSpec::new(name, image);
    // The image's own entrypoint starts its daemons and changes the owner of the signatures;
    // the tools are run directly instead.
    spec.entrypoint = command.iter().map(|s| (*s).to_owned()).collect();
    spec.memory_bytes = 2 * GIB;
    spec.cpu_millis = 1000;
    spec.pids_limit = 64;
    // Reads files of other users, as root of the container; nothing else is given back.
    spec.cap_add = vec![
        Capability::DacOverride,
        Capability::Chown,
        Capability::Fowner,
        Capability::Setuid,
        Capability::Setgid,
    ];
    spec.tmpfs = vec![Tmpfs {
        target: "/tmp".into(),
        size_bytes: 256 * 1024 * 1024,
    }];
    spec
}

/// Makes the signature folder and its volume; returns the volume.
async fn signatures_volume(
    context: &TenantContext,
) -> Result<dhpanel_docker_manager::Name, AgentError> {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = signatures_dir(context)?;
    // Only when it is new: afterwards the updater's own user owns it, and it stays open to it.
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| failed(&format!("cannot make the folder: {e}")))?;
        // The updater runs as its own user in the container; the state folder is private.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777))
            .map_err(|e| failed(&format!("cannot set the folder's rights: {e}")))?;
    }
    let volume = db::name("dh-clamav-db")?;
    context
        .docker
        .create_bind_volume(&volume, &dir, &state(context)?)
        .await
        .map_err(|e| docker_error(&e))?;
    Ok(volume)
}

async fn update(context: &TenantContext) -> Result<String, AgentError> {
    let image = image(context).await?;
    let volume = signatures_volume(context).await?;
    let net = db::name(NETWORK)?;
    context
        .docker
        .ensure_network(&net, false)
        .await
        .map_err(|e| docker_error(&e))?;
    let mut spec = base_spec(
        db::name("dh-clamav-update")?,
        image,
        &["freshclam", "--stdout"],
    );
    spec.writable_root = true;
    spec.network = Network::Named(net);
    spec.volumes = vec![VolumeMount {
        volume,
        target: "/var/lib/clamav".into(),
        read_only: false,
    }];
    // A leftover of a run that was cut off.
    let _ = context.docker.remove(&spec.name, true).await;
    let done = context
        .docker
        .run_to_end(&spec, UPDATE_LIMIT)
        .await
        .map_err(|e| docker_error(&e))?;
    let tail: String = done
        .output
        .lines()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    let tail: String = tail.chars().filter(|c| !c.is_control()).take(300).collect();
    if done.exit_code != 0 {
        return Err(AgentError::new(
            ErrorCode::Internal,
            format!("freshclam ended with {}: {tail}", done.exit_code),
        )
        .retryable());
    }
    Ok(tail)
}

/// `update_malware_signatures`.
///
/// # Errors
/// [`AgentError`]: Docker is not reachable, the image cannot be pulled, or freshclam fails
/// (retryable: the mirrors may be busy).
pub async fn update_malware_signatures(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::UpdateMalwareSignatures = op else {
        return Err(super::wrong_operation());
    };
    Ok(Output::SignaturesUpdated {
        detail: update(&context).await?,
    })
}

/// A folder name that stays inside the tenant's files.
fn plain_folder(folder: &str) -> bool {
    folder.len() <= 1024
        && !folder.contains('\0')
        && !folder.starts_with('/')
        && folder
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Reads `clamscan`'s answer: the infected files (relative to the tenant's folder) and how
/// many files it looked at.
#[must_use]
pub fn parse_scan(output: &str) -> (Vec<(String, String)>, u64) {
    let mut found = Vec::new();
    let mut scanned = 0;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Scanned files:") {
            scanned = rest.trim().parse().unwrap_or(0);
        } else if let Some(body) = line.strip_suffix(" FOUND")
            && let Some(inner) = body.strip_prefix("/scan/")
            && let Some((path, signature)) = inner.rsplit_once(": ")
        {
            found.push((path.to_owned(), signature.to_owned()));
        }
    }
    (found, scanned)
}

/// `scan_tenant`.
///
/// # Errors
/// [`AgentError`]: a folder outside the tenant's files, Docker or the image failing, the
/// signatures missing and not obtainable, or clamscan failing (exit code 2).
pub async fn scan_tenant(context: Arc<TenantContext>, op: Operation) -> Result<Output, AgentError> {
    let Operation::ScanTenant {
        tenant_id,
        uid,
        folder,
        quarantine,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    if folder.as_deref().is_some_and(|f| !plain_folder(f)) {
        return Err(AgentError::new(
            ErrorCode::InvalidArguments,
            "the folder must stay inside the tenant's files",
        ));
    }
    let id = tenant_id.to_string();
    let tenant_dir = context.tenants_root.join(&id);
    if !tenant_dir.is_dir() {
        return Err(AgentError::new(
            ErrorCode::InvalidArguments,
            "the tenant has no files on this node",
        ));
    }
    if !has_signatures(&signatures_dir(&context)?) {
        update(&context).await?;
    }
    let image = image(&context).await?;
    let signatures = signatures_volume(&context).await?;
    let short = tenant_id.simple().to_string();
    let short = &short[..12];
    let volume = db::name(&format!("dh-scan-{short}"))?;
    context
        .docker
        .create_bind_volume(&volume, &tenant_dir, &context.tenants_root)
        .await
        .map_err(|e| docker_error(&e))?;
    let target = match &folder {
        Some(f) => format!("/scan/www/{f}"),
        None => "/scan/www".to_owned(),
    };
    let name = db::name(&format!("dh-scan-{short}"))?;
    let mut spec = base_spec(
        name.clone(),
        image,
        &["clamscan", "-r", "--infected", "--stdout", &target],
    );
    spec.network = Network::None;
    spec.volumes = vec![
        VolumeMount {
            volume: signatures,
            target: "/var/lib/clamav".into(),
            read_only: true,
        },
        VolumeMount {
            volume: volume.clone(),
            target: "/scan".into(),
            read_only: true,
        },
    ];
    let _ = context.docker.remove(&name, true).await;
    let ran = context.docker.run_to_end(&spec, SCAN_LIMIT).await;
    let _ = context.docker.remove_volume(&volume).await;
    let done = ran.map_err(|e| docker_error(&e))?;
    if done.exit_code != 0 && done.exit_code != 1 {
        let tail: String = done
            .output
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ")
            .chars()
            .filter(|c| !c.is_control())
            .take(300)
            .collect();
        return Err(AgentError::new(
            ErrorCode::Internal,
            format!("clamscan ended with {}: {tail}", done.exit_code),
        ));
    }
    let (found, scanned) = parse_scan(&done.output);
    let more = found.len() > MAX_LISTED;
    let mut infected = Vec::new();
    for (path, signature) in found.into_iter().take(MAX_LISTED) {
        let mut quarantined = false;
        if quarantine {
            let socket = context.helper_socket.clone();
            let action = Action::FileOp {
                tenant_id: id.clone(),
                uid,
                request: FileRequest::Delete { path: path.clone() },
            };
            if let Ok(Ok(Some(data))) =
                tokio::task::spawn_blocking(move || ask_helper_data(&socket, action)).await
                && let Ok(result) = serde_json::from_value::<FileResult>(data)
            {
                quarantined = !matches!(result, FileResult::Error { .. });
            }
        }
        infected.push(Infection {
            path,
            signature,
            quarantined,
        });
    }
    Ok(Output::Scanned {
        scanned,
        infected,
        more,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn what_clamscan_says_is_read_and_a_folder_must_stay_inside() {
        let text = "/scan/www/a b/evil.php: Php.Trojan.Agent-1 FOUND\n/scan/www/x: y: Eicar-Test-Signature FOUND\nnoise\n\n----------- SCAN SUMMARY -----------\nScanned files: 42\nInfected files: 2\n";
        let (found, scanned) = parse_scan(text);
        assert_eq!(scanned, 42);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0],
            (
                "www/a b/evil.php".to_owned(),
                "Php.Trojan.Agent-1".to_owned()
            )
        );
        assert_eq!(found[1].1, "Eicar-Test-Signature");
        assert!(plain_folder("wp-content/uploads"));
        for bad in ["", "/etc", "../x", "a/../b", "a//b", "a\0b"] {
            assert!(!plain_folder(bad), "{bad}");
        }
    }
}
