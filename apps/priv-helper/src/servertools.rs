//! The server tools (T10.15): the time zone, the swap file, the page cache and the reboot.
//!
//! Each one is a fixed command with at most a number or a checked name in it, and none goes
//! through a shell with anything a caller wrote:
//!
//! * **time zone**: `timedatectl set-timezone <zone>`, where the zone is a file under the
//!   zone-info folder (`Area/City` made of letters, digits and `_+-`);
//! * **swap**: one file (`/swapfile`), made with `fallocate`, `chmod 600`, `mkswap` and
//!   `swapon`, taken away with `swapoff` and `rm`; an entry in `/etc/fstab` keeps it across
//!   reboots. These run as transient systemd units (`systemd-run`), outside this helper's
//!   sandbox, so the helper does not have to be given write access to `/etc` and `/`;
//! * **page cache**: `sync` and `sysctl -w vm.drop_caches=3`, the same way;
//! * **reboot**: `systemctl reboot`.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::protocol::ErrorCode;

type Failure = (ErrorCode, &'static str);

/// Where things are (changed only by tests).
#[derive(Debug, Clone)]
pub struct Env {
    /// The command that reboots the machine (called with `reboot`).
    pub reboot_command: PathBuf,
    /// The swap file.
    pub swap_file: PathBuf,
    /// The folder the time zones are files of.
    pub zoneinfo: PathBuf,
    /// Run the commands directly instead of as transient units (a test, where there is no
    /// systemd to ask).
    pub direct: bool,
}

impl Default for Env {
    fn default() -> Self {
        Self {
            reboot_command: "/usr/bin/systemctl".into(),
            swap_file: "/swapfile".into(),
            zoneinfo: "/usr/share/zoneinfo".into(),
            direct: false,
        }
    }
}

/// The biggest swap file that can be asked for, in MiB.
pub const MAX_SWAP_MB: u32 = 32_768;

fn run(env: &Env, program: &str, args: &[&str]) -> bool {
    let mut command = if env.direct {
        Command::new(program)
    } else {
        let mut c = Command::new("/usr/bin/systemd-run");
        c.args([
            "--quiet",
            "--wait",
            "--collect",
            "--service-type=oneshot",
            "--",
            program,
        ]);
        c
    };
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A zone name: `Europe/Berlin`, `UTC`, `Etc/GMT+1`: plain parts, no dots at the start.
fn valid_zone(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
        })
}

/// Sets the time zone.
///
/// # Errors
/// A name that is not a zone of this machine, or the command failing.
pub fn set_timezone(env: &Env, zone: &str) -> Result<(), Failure> {
    if !valid_zone(zone) || !env.zoneinfo.join(zone).is_file() {
        return Err((
            ErrorCode::InvalidArguments,
            "not a time zone of this machine",
        ));
    }
    // Direct (not a transient unit): timedatectl talks to systemd itself.
    let ok = Command::new("/usr/bin/timedatectl")
        .args(["set-timezone", zone])
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err((ErrorCode::Failed, "the time zone could not be set"))
    }
}

fn path_text(path: &Path) -> Result<&str, Failure> {
    path.to_str()
        .filter(|p| p.starts_with('/') && !p.contains(['\0', '\n', ' ']))
        .ok_or((ErrorCode::Failed, "the swap file's path is not usable"))
}

/// Makes the swap file `megabytes` big (replacing the old one), or takes it away with `0`.
///
/// # Errors
/// A size out of range, or a command failing (the old file is gone by then).
pub fn set_swap(env: &Env, megabytes: u32) -> Result<(), Failure> {
    if megabytes > MAX_SWAP_MB {
        return Err((ErrorCode::InvalidArguments, "the swap size is out of range"));
    }
    let file = path_text(&env.swap_file)?;
    // Whatever is there goes first (it may not be in use: failing is fine).
    let _ = run(env, "/usr/sbin/swapoff", &[file]);
    if !run(env, "/usr/bin/rm", &["-f", file]) {
        return Err((ErrorCode::Failed, "the old swap file could not be removed"));
    }
    // Its line in fstab goes too, and comes back with the new one.
    let pattern = format!("\\|^{file} |d");
    if !run(env, "/usr/bin/sed", &["-i", &pattern, "/etc/fstab"]) && !env.direct {
        return Err((ErrorCode::Failed, "fstab could not be changed"));
    }
    if megabytes == 0 {
        return Ok(());
    }
    let size = format!("{megabytes}M");
    let steps: [(&str, Vec<&str>); 4] = [
        ("/usr/bin/fallocate", vec!["-l", &size, file]),
        ("/usr/bin/chmod", vec!["600", file]),
        ("/usr/sbin/mkswap", vec![file]),
        ("/usr/sbin/swapon", vec![file]),
    ];
    for (program, args) in steps {
        if !run(env, program, &args) {
            let _ = run(env, "/usr/bin/rm", &["-f", file]);
            return Err((ErrorCode::Failed, "the swap file could not be made"));
        }
    }
    // The constant line, appended: nothing of the request is in it but the path, checked above.
    let line = format!("{file} none swap sw 0 0");
    if !run(
        env,
        "/usr/bin/sh",
        &["-c", "echo \"$1\" >> /etc/fstab", "sh", &line],
    ) && !env.direct
    {
        return Err((ErrorCode::Failed, "fstab could not be changed"));
    }
    Ok(())
}

/// Drops the page cache (`sync`, then `vm.drop_caches=3`).
///
/// # Errors
/// The command failing.
pub fn drop_caches(env: &Env) -> Result<(), Failure> {
    let _ = run(env, "/usr/bin/sync", &[]);
    if run(env, "/usr/sbin/sysctl", &["-w", "vm.drop_caches=3"]) {
        Ok(())
    } else {
        Err((ErrorCode::Failed, "the cache could not be dropped"))
    }
}

/// Reboots the machine.
///
/// # Errors
/// The command failing.
pub fn reboot(env: &Env) -> Result<(), Failure> {
    let ok = Command::new(&env.reboot_command)
        .arg("reboot")
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        Ok(())
    } else {
        Err((ErrorCode::Failed, "the reboot could not be started"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_zone_names_pass() {
        for good in [
            "UTC",
            "Europe/Berlin",
            "Asia/Dhaka",
            "Etc/GMT+1",
            "America/Argentina/Buenos_Aires",
        ] {
            assert!(valid_zone(good), "{good}");
        }
        for bad in [
            "",
            "../etc/passwd",
            "/etc/passwd",
            "a//b",
            ".hidden",
            "a b",
            "x;y",
            "a/..",
        ] {
            assert!(!valid_zone(bad), "{bad}");
        }
    }

    #[test]
    fn a_zone_must_exist_and_a_swap_size_must_fit() {
        let env = Env {
            zoneinfo: "/nonexistent".into(),
            ..Env::default()
        };
        assert!(set_timezone(&env, "UTC").is_err());
        assert_eq!(
            set_swap(&Env::default(), MAX_SWAP_MB + 1).unwrap_err().0,
            ErrorCode::InvalidArguments
        );
    }
}
