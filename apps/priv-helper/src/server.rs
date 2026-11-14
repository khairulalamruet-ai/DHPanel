//! The socket server: who may connect, one request per connection, a log line
//! for every decision.

use std::{
    io::{self, BufRead as _, BufReader, Read as _, Write as _},
    os::unix::{
        fs::{FileTypeExt as _, PermissionsExt as _},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::{
    protocol::{Action, ErrorCode, MAX_REQUEST_BYTES, Response, parse_request},
    tenant::{self, TenantEnv},
};

/// Default socket path (the systemd unit creates `/run/dhpanel`).
pub const DEFAULT_SOCKET: &str = "/run/dhpanel/priv-helper.sock";
/// How long a caller has to send its request and take the answer.
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Where log lines go.
pub trait Log: Sync {
    /// Writes one line. Implementations must not fail the caller.
    fn line(&self, text: &str);
}

/// Logs to stderr (the journal under systemd).
pub struct StderrLog;

impl Log for StderrLog {
    #[allow(clippy::print_stderr)]
    fn line(&self, text: &str) {
        eprintln!("dhpanel-priv-helper: {text}");
    }
}

/// How the helper is set up.
#[derive(Debug, Clone)]
pub struct Config {
    /// The socket to listen on.
    pub socket: PathBuf,
    /// The only user id that may talk to the helper.
    pub agent_uid: u32,
    /// The group that owns the socket file (not used for access: mode is 0600).
    pub agent_gid: u32,
    /// Where tenant directories live and how their ids map to the host.
    pub tenants: TenantEnv,
    /// The one port the firewall action may filter (the database's public port).
    pub db_port: u16,
}

/// Creates the socket: owned by the agent, mode 0600. A stale socket file from
/// an earlier run is replaced; anything else at that path is left alone.
///
/// # Errors
/// The path is occupied by something that is not a socket, or the socket
/// cannot be created or handed to the agent.
pub fn bind(config: &Config) -> io::Result<UnixListener> {
    match std::fs::symlink_metadata(&config.socket) {
        Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(&config.socket)?,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a socket", config.socket.display()),
            ));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let listener = UnixListener::bind(&config.socket)?;
    // Until these two calls are done the peer check below still protects us.
    std::fs::set_permissions(&config.socket, std::fs::Permissions::from_mode(0o600))?;
    let meta = std::fs::metadata(&config.socket)?;
    {
        use std::os::unix::fs::MetadataExt as _;
        if meta.uid() != config.agent_uid || meta.gid() != config.agent_gid {
            std::os::unix::fs::chown(
                &config.socket,
                Some(config.agent_uid),
                Some(config.agent_gid),
            )?;
        }
    }
    Ok(listener)
}

/// Serves until `stop` is set. Connections are handled one at a time: the
/// helper's work is short and must never race with itself.
///
/// # Errors
/// The listener fails in a way that is not about a single connection.
pub fn serve(
    listener: &UnixListener,
    config: &Config,
    log: &dyn Log,
    stop: &AtomicBool,
) -> io::Result<()> {
    listener.set_nonblocking(true)?;
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(err) = handle(stream, config, log) {
                    log.line(&format!("connection error: {}", err.kind()));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Whether a peer with this uid may use the helper: the agent, nobody else
/// (not even root: root does not need the helper, and a confused root process
/// must not be able to drive it).
#[must_use]
pub fn authorized(peer_uid: u32, config: &Config) -> bool {
    peer_uid == config.agent_uid
}

fn handle(stream: UnixStream, config: &Config, log: &dyn Log) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let peer_uid = rustix::net::sockopt::socket_peercred(&stream)
        .map(|cred| cred.uid.as_raw())
        .map_err(io::Error::from)?;
    if !authorized(peer_uid, config) {
        log.line(&format!("REFUSED uid={peer_uid}: not the agent"));
        return answer(
            &stream,
            &Response::refused(
                "",
                ErrorCode::Forbidden,
                "only the node agent may use this socket",
            ),
        );
    }

    // One line, bounded: read at most one byte more than allowed to notice excess.
    let mut line = Vec::new();
    BufReader::new((&stream).take(MAX_REQUEST_BYTES as u64 + 1)).read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    let request = match parse_request(&line) {
        Ok(request) => request,
        Err(rejected) => {
            let code = rejected.response.error.as_ref().map(|e| e.code);
            log.line(&format!(
                "REFUSED uid={peer_uid} id={} action={} reason={code:?}",
                printable(&rejected.response.id),
                rejected.asked_for.as_deref().unwrap_or("-"),
            ));
            return answer(&stream, &rejected.response);
        }
    };
    let response = match execute(&request.action, config) {
        Ok(()) => {
            log.line(&format!(
                "DONE uid={peer_uid} id={} action={}",
                request.id,
                request.action.name()
            ));
            Response::done(&request.id)
        }
        Err((code, message)) => {
            log.line(&format!(
                "FAILED uid={peer_uid} id={} action={} reason={code:?}",
                request.id,
                request.action.name()
            ));
            Response::refused(&request.id, code, message)
        }
    };
    answer(&stream, &response)
}

/// Carries out an action. Each arm validates its own arguments: the helper
/// never relies on what the agent checked.
fn execute(action: &Action, config: &Config) -> Result<(), (ErrorCode, &'static str)> {
    match action {
        Action::Noop => Ok(()),
        Action::PrepareTenant {
            tenant_id,
            uid,
            skeleton,
        } => tenant::prepare_with_files(&config.tenants, tenant_id, *uid, skeleton),
        Action::SetStorageQuota {
            tenant_id,
            uid,
            disk_bytes,
            inodes,
        } => tenant::set_storage_quota(&config.tenants, tenant_id, *uid, *disk_bytes, *inodes),
        Action::SetDbFirewall { cidrs } => crate::firewall::set(config.db_port, cidrs),
        Action::RemoveTenant { tenant_id, uid } => tenant::remove(&config.tenants, tenant_id, *uid),
    }
}

fn answer(mut stream: &UnixStream, response: &Response) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(response).map_err(io::Error::other)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}

fn printable(id: &str) -> &str {
    if id.is_empty() { "-" } else { id }
}

/// Looks a user up in a passwd file (`name:x:uid:gid:...`). Local users only,
/// which is what the installer creates.
#[must_use]
pub fn lookup_user(passwd: &str, name: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == name).then_some(())?;
        let _password = fields.next()?;
        Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
    })
}

/// Reads `/etc/passwd` and looks the user up.
#[must_use]
pub fn lookup_system_user(name: &str) -> Option<(u32, u32)> {
    lookup_user(
        &std::fs::read_to_string(Path::new("/etc/passwd")).ok()?,
        name,
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct Memory(Mutex<Vec<String>>);

    impl Log for Memory {
        fn line(&self, text: &str) {
            self.0.lock().unwrap().push(text.to_owned());
        }
    }

    fn test_tenants(gid: u32) -> TenantEnv {
        TenantEnv {
            root: std::env::temp_dir().join(format!("dhph-srv-tenants-{}", std::process::id())),
            agent_name: "dhpanel-agent".into(),
            subuid_file: "/nonexistent".into(),
            agent_gid: gid,
            mountinfo: "/nonexistent".into(),
        }
    }

    fn own_ids() -> (u32, u32) {
        use std::os::unix::fs::MetadataExt as _;
        let meta = std::fs::metadata(std::env::temp_dir()).unwrap();
        let _ = meta;
        let probe = std::env::temp_dir().join(format!(
            "dhph-probe-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&probe, b"").unwrap();
        let meta = std::fs::metadata(&probe).unwrap();
        let _ = std::fs::remove_file(&probe);
        (meta.uid(), meta.gid())
    }

    struct Running {
        config: Config,
        log: Arc<Memory>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Running {
        /// A helper that accepts `agent_uid` (our own uid unless told otherwise).
        fn start(tag: &str, agent_uid: Option<u32>) -> Self {
            let (uid, gid) = own_ids();
            let socket =
                std::env::temp_dir().join(format!("dhph-{}-{tag}.sock", std::process::id()));
            // The file is always ours; only who is *accepted* varies.
            let bind_config = Config {
                socket: socket.clone(),
                agent_uid: uid,
                agent_gid: gid,
                tenants: test_tenants(gid),
                db_port: 13306,
            };
            let listener = bind(&bind_config).unwrap();
            let config = Config {
                agent_uid: agent_uid.unwrap_or(uid),
                ..bind_config
            };
            let (log, stop) = (
                Arc::new(Memory::default()),
                Arc::new(AtomicBool::new(false)),
            );
            let thread = std::thread::spawn({
                let (config, log, stop) = (config.clone(), log.clone(), stop.clone());
                move || serve(&listener, &config, &*log, &stop).unwrap()
            });
            Self {
                config,
                log,
                stop,
                thread: Some(thread),
            }
        }

        fn ask(&self, line: &[u8]) -> Response {
            let mut stream = UnixStream::connect(&self.config.socket).unwrap();
            stream.write_all(line).unwrap();
            stream.write_all(b"\n").unwrap();
            let mut answer = String::new();
            BufReader::new(stream).read_line(&mut answer).unwrap();
            serde_json::from_str(&answer).unwrap()
        }

        fn log(&self) -> Vec<String> {
            self.log.0.lock().unwrap().clone()
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            let _ = std::fs::remove_file(&self.config.socket);
        }
    }

    #[test]
    fn the_socket_is_private_and_a_noop_is_done_and_logged() {
        let helper = Running::start("noop", None);
        let mode = std::fs::metadata(&helper.config.socket)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let response = helper.ask(br#"{"id":"req-1","action":{"type":"noop"}}"#);
        assert_eq!(response, Response::done("req-1"));
        let log = helper.log();
        assert_eq!(log.len(), 1);
        assert!(
            log[0].starts_with("DONE uid=") && log[0].ends_with("id=req-1 action=noop"),
            "{log:?}"
        );
    }

    #[test]
    fn an_unknown_action_is_refused_and_logged() {
        let helper = Running::start("unknown", None);
        let response = helper.ask(br#"{"id":"req-2","action":{"type":"run_command","cmd":"id"}}"#);
        assert!(!response.ok);
        assert_eq!(response.id, "req-2");
        assert_eq!(response.error.unwrap().code, ErrorCode::UnknownAction);
        let log = helper.log();
        assert_eq!(log.len(), 1);
        assert!(log[0].starts_with("REFUSED uid="), "{log:?}");
        assert!(
            log[0].contains("id=req-2 action=run_command reason=Some(UnknownAction)"),
            "{log:?}"
        );
        assert!(!log[0].contains("cmd"), "arguments are not logged");

        // Garbage, a log-injection attempt and an oversized line are refused too,
        // and the helper keeps serving afterwards.
        assert_eq!(
            helper.ask(b"not json").error.unwrap().code,
            ErrorCode::InvalidRequest
        );
        let injected =
            helper.ask(br#"{"id":"x","action":{"type":"a\nDONE uid=0 id=fake action=noop"}}"#);
        assert_eq!(injected.error.unwrap().code, ErrorCode::InvalidRequest);
        let huge = vec![b'x'; MAX_REQUEST_BYTES + 100];
        assert_eq!(
            helper.ask(&huge).error.unwrap().code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            helper.ask(br#"{"id":"ok","action":{"type":"noop"}}"#),
            Response::done("ok")
        );
        let log = helper.log();
        assert_eq!(log.len(), 5);
        assert!(
            log.iter().all(|l| !l.contains('\n')),
            "one line per decision"
        );
        assert_eq!(log.iter().filter(|l| l.starts_with("DONE")).count(), 1);
    }

    #[test]
    fn a_caller_that_is_not_the_agent_is_refused_before_its_request_is_read() {
        // The helper expects another uid than ours, so we are the stranger.
        let (uid, _) = own_ids();
        let helper = Running::start("stranger", Some(uid.wrapping_add(1)));
        let response = helper.ask(br#"{"id":"req-3","action":{"type":"noop"}}"#);
        assert_eq!(response.error.unwrap().code, ErrorCode::Forbidden);
        assert_eq!(response.id, "", "the request was not read");
        let log = helper.log();
        assert_eq!(log, [format!("REFUSED uid={uid}: not the agent")]);
        assert!(
            !authorized(0, &helper.config),
            "root is not the agent either"
        );
    }

    #[test]
    fn prepare_tenant_is_validated_by_the_helper_itself() {
        let helper = Running::start("tenant", None);
        let ask = |json: &str| helper.ask(json.as_bytes());
        // Bad ids and ids outside the tenant range are refused before anything is touched.
        for bad in [
            r#"{"id":"t1","action":{"type":"prepare_tenant","tenant_id":"../../etc","uid":10000}}"#,
            r#"{"id":"t2","action":{"type":"prepare_tenant","tenant_id":"0190b6a2-0000-7000-8000-000000000001","uid":0}}"#,
            r#"{"id":"t3","action":{"type":"prepare_tenant","tenant_id":"0190b6a2-0000-7000-8000-000000000001","uid":99999}}"#,
        ] {
            let response = ask(bad);
            assert_eq!(
                response.error.unwrap().code,
                ErrorCode::InvalidArguments,
                "{bad}"
            );
        }
        // Extra fields and missing ones are refused as bad arguments, never guessed.
        let extra = ask(
            r#"{"id":"t4","action":{"type":"prepare_tenant","tenant_id":"0190b6a2-0000-7000-8000-000000000001","uid":10000,"path":"/etc"}}"#,
        );
        assert_eq!(extra.error.unwrap().code, ErrorCode::InvalidArguments);
        let missing = ask(r#"{"id":"t5","action":{"type":"prepare_tenant","uid":10000}}"#);
        assert_eq!(missing.error.unwrap().code, ErrorCode::InvalidArguments);
        // A well-formed request on a machine without the agent's subordinate range fails clearly.
        let unreadable = ask(
            r#"{"id":"t6","action":{"type":"prepare_tenant","tenant_id":"0190b6a2-0000-7000-8000-000000000001","uid":10000}}"#,
        );
        assert_eq!(unreadable.error.unwrap().code, ErrorCode::Failed);
        assert!(
            !helper.config.tenants.root.exists(),
            "no refused request created anything"
        );
        let log = helper.log();
        assert!(
            log.iter().all(|l| !l.contains("etc")),
            "arguments are not logged: {log:?}"
        );
        assert!(
            log.iter()
                .filter(|l| l.contains("action=prepare_tenant"))
                .count()
                >= 5
        );
    }

    #[test]
    fn a_stale_socket_is_replaced_but_other_files_are_left_alone() {
        let (uid, gid) = own_ids();
        let path = std::env::temp_dir().join(format!("dhph-{}-stale.sock", std::process::id()));
        let config = Config {
            socket: path.clone(),
            agent_uid: uid,
            agent_gid: gid,
            tenants: test_tenants(gid),
            db_port: 13306,
        };
        drop(bind(&config).unwrap()); // leaves the socket file behind
        assert!(path.exists());
        drop(bind(&config).unwrap()); // replaced, no error
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"precious").unwrap();
        assert!(bind(&config).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"precious");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn users_are_found_in_passwd_text() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\ndhpanel-agent:x:999:987::/var/lib/dhpanel-agent/home:/usr/sbin/nologin\nbroken:x:abc:1::/:/bin/sh\n";
        assert_eq!(lookup_user(passwd, "dhpanel-agent"), Some((999, 987)));
        assert_eq!(lookup_user(passwd, "root"), Some((0, 0)));
        assert_eq!(lookup_user(passwd, "dhpanel"), None, "no prefix matches");
        assert_eq!(lookup_user(passwd, "broken"), None);
        assert_eq!(lookup_user(passwd, "nobody"), None);
    }
}
