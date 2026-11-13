//! `dhpanel-priv-helper`: the small root helper of the node agent.
//!
//! ```text
//! dhpanel-priv-helper [--socket PATH] [--agent-user NAME | --agent-uid UID --agent-gid GID]
//! ```
//!
//! Started by systemd as root. See the library documentation for what it will
//! and will not do. Exit codes: 64 bad usage, 78 the agent user does not exist,
//! 74 the socket cannot be set up.

use std::{path::PathBuf, process::ExitCode, sync::atomic::AtomicBool};

use dhpanel_priv_helper::{
    server::{Config, DEFAULT_SOCKET, Log as _, StderrLog, bind, lookup_system_user, serve},
    tenant::TenantEnv,
};

const USAGE: &str = "usage: dhpanel-priv-helper [--socket PATH] [--tenants-root DIR] [--agent-user NAME | --agent-uid UID --agent-gid GID]";

#[allow(clippy::print_stdout)]
fn main() -> ExitCode {
    let log = StderrLog;
    let mut socket = PathBuf::from(DEFAULT_SOCKET);
    let mut user = String::from("dhpanel-agent");
    let mut tenants_root = PathBuf::from("/srv/tenants");
    let (mut uid, mut gid) = (None::<u32>, None::<u32>);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next();
        let parsed = match arg.as_str() {
            "--socket" => value().map(|v| socket = v.into()),
            "--agent-user" => value().map(|v| user = v),
            "--tenants-root" => value().map(|v| tenants_root = v.into()),
            "--agent-uid" => value().and_then(|v| v.parse().ok()).map(|v| uid = Some(v)),
            "--agent-gid" => value().and_then(|v| v.parse().ok()).map(|v| gid = Some(v)),
            "--version" => {
                println!("{}", dhpanel_common::build_banner("priv-helper"));
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => None,
        };
        if parsed.is_none() {
            log.line(&format!(
                "bad or incomplete option `{}`; {USAGE}",
                arg.escape_default()
            ));
            return ExitCode::from(64);
        }
    }
    let (agent_uid, agent_gid) = match (uid, gid) {
        (Some(uid), Some(gid)) => (uid, gid),
        (None, None) => match lookup_system_user(&user) {
            Some(ids) => ids,
            None => {
                log.line(&format!("user `{}` does not exist", user.escape_default()));
                return ExitCode::from(78);
            }
        },
        _ => {
            log.line("--agent-uid and --agent-gid go together");
            return ExitCode::from(64);
        }
    };
    let config = Config {
        socket,
        agent_uid,
        agent_gid,
        tenants: TenantEnv {
            root: tenants_root,
            agent_name: user.clone(),
            subuid_file: "/etc/subuid".into(),
            agent_gid,
        },
    };
    let listener = match bind(&config) {
        Ok(listener) => listener,
        Err(err) => {
            log.line(&format!("cannot set up {}: {err}", config.socket.display()));
            return ExitCode::from(74);
        }
    };
    log.line(&format!(
        "listening on {} for uid {agent_uid}",
        config.socket.display()
    ));
    // Runs until systemd stops the process; a stale socket file is replaced at the next start.
    match serve(&listener, &config, &log, &AtomicBool::new(false)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log.line(&format!("stopped: {err}"));
            ExitCode::FAILURE
        }
    }
}
