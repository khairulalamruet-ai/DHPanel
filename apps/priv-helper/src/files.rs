//! The file manager's work on a tenant's files (T08.01-T08.06).
//!
//! The helper itself never touches a tenant's files. For each request it starts a
//! copy of its own program (`dhpanel-priv-helper fileop --root DIR`, through
//! `setpriv`) **as the tenant's host user id and group**, with no supplementary groups
//! and none of the helper's capabilities, so the worker can do exactly what the tenant could do with
//! its own files and nothing else: another tenant's files, the agent's state and the
//! system are as closed to it as they are to the tenant. The worker reads one request
//! from its input, opens the tenant's directory with `dhpanel-safe-fs`, does it with
//! `dhpanel-fileops` and prints one answer. It is given a time limit and killed after.

use std::{
    io::{Read as _, Write as _},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use dhpanel_node_protocol::files::{FileErrorCode, FileRequest, FileResult};

use crate::{
    protocol::ErrorCode,
    tenant::{Failure, TenantEnv, host_uid, subuid_range, valid_tenant_id},
};

/// Longest a worker may run (a big archive takes a while).
pub const TIME_LIMIT: Duration = Duration::from_secs(600);
/// Most a worker's answer may be.
const MAX_ANSWER: u64 = 2 * 1024 * 1024;

/// Runs a request as the tenant and returns the answer.
///
/// # Errors
/// The id or uid is not acceptable, the worker could not be started, ran too long or
/// said nothing usable.
pub fn run(
    env: &TenantEnv,
    tenant_id: &str,
    uid: u32,
    request: &FileRequest,
) -> Result<FileResult, Failure> {
    if !valid_tenant_id(tenant_id) {
        return Err((ErrorCode::InvalidArguments, "bad tenant id"));
    }
    let text = std::fs::read_to_string(&env.subuid_file)
        .map_err(|_| (ErrorCode::Failed, "cannot read the subordinate ids"))?;
    let range = subuid_range(&text, &env.agent_name)
        .ok_or((ErrorCode::Failed, "the agent has no subordinate ids"))?;
    let host = host_uid(range, uid)?;
    let root = env.root.join(tenant_id);
    work(&root, host, host, request)
}

/// Starts the worker as `uid`/`gid` (the caller has already checked both).
///
/// # Errors
/// See [`run`].
pub fn work(root: &Path, uid: u32, gid: u32, request: &FileRequest) -> Result<FileResult, Failure> {
    let program = std::env::current_exe()
        .map_err(|_| (ErrorCode::Failed, "cannot find the helper itself"))?;
    let input = serde_json::to_vec(request)
        .map_err(|_| (ErrorCode::InvalidArguments, "the request cannot be encoded"))?;
    // `setpriv` (util-linux, on every Linux) drops to the tenant's user and group,
    // clears the supplementary groups and forbids gaining anything later, then runs
    // the worker.
    let setpriv = ["/usr/bin/setpriv", "/bin/setpriv"]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .ok_or((ErrorCode::Failed, "setpriv (util-linux) is missing"))?;
    let mut child = Command::new(setpriv)
        .arg(format!("--reuid={uid}"))
        .arg(format!("--regid={gid}"))
        .arg("--clear-groups")
        .arg("--no-new-privs")
        .arg("--")
        .arg(program)
        .arg("fileop")
        .arg("--root")
        .arg(root)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| (ErrorCode::Failed, "cannot start the file worker"))?;
    let (mut stdin, stdout) = child
        .stdin
        .take()
        .zip(child.stdout.take())
        .ok_or((ErrorCode::Failed, "cannot talk to the file worker"))?;
    let stderr = child.stderr.take();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.take(MAX_ANSWER).read_to_end(&mut out);
        out
    });
    let started = Instant::now();
    let finished = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status.success()),
            Ok(None) if started.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break Some(false),
        }
    };
    let _ = writer.join();
    let answer = reader.join().unwrap_or_default();
    // What the worker said about itself goes to the log (never to the caller).
    let said = stderr.map_or_else(String::new, |mut e| {
        let mut text = String::new();
        let _ = e.by_ref().take(300).read_to_string(&mut text);
        text
    });
    if finished != Some(true) && !said.is_empty() {
        eprintln!(
            "dhpanel-priv-helper: file worker said: {}",
            said.escape_debug()
        );
    }
    match finished {
        None => Ok(FileResult::error(
            FileErrorCode::Failed,
            "the request took too long and was stopped",
        )),
        Some(true) => serde_json::from_slice(&answer)
            .map_err(|_| (ErrorCode::Failed, "the file worker's answer is unreadable")),
        Some(false) => Err((ErrorCode::Failed, "the file worker failed")),
    }
}

/// The worker's side: one request in, one answer out.
///
/// # Errors
/// The root cannot be opened or the request cannot be read.
pub fn worker(
    root: &Path,
    input: impl std::io::Read,
    mut out: impl std::io::Write,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    input
        .take(crate::protocol::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let request: FileRequest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = match dhpanel_safe_fs::Root::open(root) {
        Ok(root) => dhpanel_fileops::handle(&root, request),
        Err(_) => FileResult::error(FileErrorCode::NotFound, "the files are not there"),
    };
    let json = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    out.write_all(&json).map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_worker_answers_a_request_on_a_directory() {
        let dir = std::env::temp_dir().join(format!("dhph-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        let request = serde_json::to_vec(&FileRequest::List {
            path: String::new(),
            offset: 0,
            limit: 10,
        })
        .unwrap();
        let mut out = Vec::new();
        worker(&dir, &request[..], &mut out).unwrap();
        let FileResult::Entries { entries, total } = serde_json::from_slice(&out).unwrap() else {
            panic!("{}", String::from_utf8_lossy(&out));
        };
        assert_eq!((total, entries[0].name.as_str()), (1, "a.txt"));
        // A directory that is not there, and a request that is not one.
        let mut out = Vec::new();
        worker(&dir.join("nope"), &request[..], &mut out).unwrap();
        assert!(matches!(
            serde_json::from_slice::<FileResult>(&out).unwrap(),
            FileResult::Error {
                code: FileErrorCode::NotFound,
                ..
            }
        ));
        assert!(worker(&dir, &b"not json"[..], Vec::new()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_ids_never_start_a_worker() {
        let env = TenantEnv {
            root: "/nonexistent".into(),
            agent_name: "dhpanel-agent".into(),
            subuid_file: "/nonexistent".into(),
            agent_gid: 1,
            mountinfo: "/nonexistent".into(),
        };
        let request = FileRequest::TrashList;
        assert_eq!(
            run(&env, "../../etc", 10_000, &request).unwrap_err().0,
            ErrorCode::InvalidArguments
        );
    }
}
