//! An SFTP folder, through libssh2 (`ssh2`). The server's key must be the pinned one:
//! there is no "trust on first use"; a connection to a server whose key is not pinned
//! fails and says which key it showed, so the administrator can pin it.

use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use sha2::{Digest, Sha256};
use ssh2::{OpenFlags, OpenType, Session, Sftp as Ssh2Sftp};

use crate::{DestError, Destination, check};

/// How to log in.
#[derive(Debug, Clone)]
pub enum Login {
    /// A password.
    Password(String),
    /// A private key in OpenSSH or PEM text.
    Key(String),
}

/// The settings of an SFTP folder.
#[derive(Debug, Clone)]
pub struct SftpConfig {
    /// The host.
    pub host: String,
    /// The port.
    pub port: u16,
    /// The user.
    pub user: String,
    /// The folder (absolute).
    pub path: String,
    /// SHA-256 of the server's public key (wire format), in hex.
    pub host_key_sha256: Option<String>,
    /// How to log in.
    pub login: Login,
}

/// A folder on an SFTP server. One connection; the work is done one object at a time.
pub struct Sftp {
    // Kept so the connection lives as long as the SFTP channel.
    _session: Session,
    sftp: Arc<Mutex<Ssh2Sftp>>,
    base: String,
}

fn other(e: impl std::fmt::Display) -> DestError {
    DestError::Other(
        e.to_string()
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect(),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Sftp {
    /// Connects and logs in.
    ///
    /// # Errors
    /// [`DestError::HostKey`] when the server's key is not the pinned one (or none is pinned);
    /// [`DestError::Refused`] for a refused login; [`DestError::Unreachable`].
    pub fn connect(config: &SftpConfig) -> Result<Self, DestError> {
        let address = (config.host.as_str(), config.port)
            .to_socket_addrs()
            .map_err(|_| DestError::Unreachable("the name does not resolve".into()))?
            .next()
            .ok_or_else(|| DestError::Unreachable("the name does not resolve".into()))?;
        let tcp = TcpStream::connect_timeout(&address, Duration::from_secs(20))
            .map_err(|e| DestError::Unreachable(e.kind().to_string()))?;
        let mut session = Session::new().map_err(other)?;
        session.set_tcp_stream(tcp);
        // Every step may take at most a minute.
        session.set_timeout(60_000);
        session
            .handshake()
            .map_err(|e| DestError::Unreachable(other(e).to_string()))?;
        // The key first: nothing is sent to a server that is not the pinned one.
        let (key, _) = session
            .host_key()
            .ok_or_else(|| DestError::Unreachable("the server showed no key".into()))?;
        let shown = hex(&Sha256::digest(key));
        if config.host_key_sha256.as_deref() != Some(shown.as_str()) {
            return Err(DestError::HostKey(shown));
        }
        match &config.login {
            Login::Password(p) => session.userauth_password(&config.user, p),
            Login::Key(k) => session.userauth_pubkey_memory(&config.user, None, k, None),
        }
        .map_err(|_| DestError::Refused("the login was refused".into()))?;
        if !session.authenticated() {
            return Err(DestError::Refused("the login was refused".into()));
        }
        let sftp = session.sftp().map_err(other)?;
        Ok(Self {
            _session: session,
            sftp: Arc::new(Mutex::new(sftp)),
            base: config.path.trim_end_matches('/').to_owned(),
        })
    }

    fn path(&self, name: &str) -> String {
        format!("{}/{name}", self.base)
    }

    fn with<T>(&self, f: impl FnOnce(&Ssh2Sftp) -> Result<T, DestError>) -> Result<T, DestError> {
        let sftp = self
            .sftp
            .lock()
            .map_err(|_| DestError::Other("the connection is broken".into()))?;
        f(&sftp)
    }

    fn make_folders(&self, name: &str) -> Result<(), DestError> {
        let mut parts: Vec<&str> = name.split('/').collect();
        parts.pop();
        let mut so_far = self.base.clone();
        self.with(|sftp| {
            for part in parts {
                so_far.push('/');
                so_far.push_str(part);
                // Already there is fine.
                let _ = sftp.mkdir(Path::new(&so_far), 0o755);
            }
            Ok(())
        })
    }
}

impl Destination for Sftp {
    fn put(&self, name: &str, data: &mut dyn Read) -> Result<u64, DestError> {
        check(name)?;
        self.make_folders(name)?;
        let temp = self.path(&format!("{name}.part"));
        let real = self.path(name);
        let written = self.with(|sftp| {
            let mut file = sftp
                .open_mode(
                    Path::new(&temp),
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                    0o644,
                    OpenType::File,
                )
                .map_err(other)?;
            let mut total = 0u64;
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = data.read(&mut buf).map_err(|e| {
                    DestError::Other(format!("cannot read what is to be stored: {}", e.kind()))
                })?;
                if n == 0 {
                    break;
                }
                file.write_all(&buf[..n]).map_err(other)?;
                total += n as u64;
            }
            file.flush().map_err(other)?;
            Ok(total)
        });
        match written {
            Ok(total) => {
                // Only a whole object is ever under its real name.
                self.with(|sftp| {
                    let _ = sftp.unlink(Path::new(&real));
                    sftp.rename(Path::new(&temp), Path::new(&real), None)
                        .map_err(other)
                })?;
                Ok(total)
            }
            Err(e) => {
                let _ = self.with(|sftp| Ok(sftp.unlink(Path::new(&temp)).is_ok()));
                Err(e)
            }
        }
    }

    fn get(&self, name: &str) -> Result<Box<dyn Read + Send>, DestError> {
        check(name)?;
        // The object is read through its own handle; the connection is shared, so reads
        // are taken in turns.
        let file = self.with(|sftp| {
            sftp.open(Path::new(&self.path(name)))
                .map_err(|_| DestError::NotFound)
        })?;
        Ok(Box::new(Reader {
            file,
            _sftp: self.sftp.clone(),
        }))
    }

    fn delete(&self, name: &str) -> Result<(), DestError> {
        check(name)?;
        self.with(|sftp| {
            let _ = sftp.unlink(Path::new(&self.path(name)));
            Ok(())
        })
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, DestError> {
        let mut out = Vec::new();
        let mut folders = vec![String::new()];
        self.with(|sftp| {
            while let Some(dir) = folders.pop() {
                let path = if dir.is_empty() {
                    self.base.clone()
                } else {
                    self.path(&dir)
                };
                let path = if path.is_empty() {
                    "/".to_owned()
                } else {
                    path
                };
                let Ok(entries) = sftp.readdir(Path::new(&path)) else {
                    continue;
                };
                for (entry, stat) in entries {
                    let Some(name) = entry.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if name == "." || name == ".." {
                        continue;
                    }
                    let full = if dir.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{dir}/{name}")
                    };
                    if stat.is_dir() {
                        if full.matches('/').count() < 4 {
                            folders.push(full);
                        }
                    } else if full.starts_with(prefix) && !full.ends_with(".part") {
                        out.push(full);
                    }
                }
            }
            Ok(())
        })?;
        out.sort();
        Ok(out)
    }
}

/// An open remote file, read as a stream.
struct Reader {
    file: ssh2::File,
    // The channel lives as long as the reader does.
    _sftp: Arc<Mutex<Ssh2Sftp>>,
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}
