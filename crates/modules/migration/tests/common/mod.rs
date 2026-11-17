//! A small SSH and SFTP server for the tests: one user, one file.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use russh::{
    Channel, ChannelId,
    keys::{Algorithm, PrivateKey},
    server::{Auth, ChannelOpenHandle, Msg, Server as _, Session},
};
use russh_sftp::protocol::{
    Attrs, Data, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};
use tokio::sync::Mutex;

pub const PASSWORD: &str = "correct horse";
pub const PATH: &str = "/backups/cpmove-alice.tar.gz";

#[derive(Clone)]
struct Server {
    content: Arc<Vec<u8>>,
}

impl russh::server::Server for Server {
    type Handler = Ssh;
    fn new_client(&mut self, _: Option<SocketAddr>) -> Ssh {
        Ssh {
            content: self.content.clone(),
            channels: Arc::default(),
        }
    }
}

struct Ssh {
    content: Arc<Vec<u8>>,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl russh::server::Handler for Ssh {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "alice" && password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.lock().await.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            let channel = self.channels.lock().await.remove(&id).unwrap();
            session.channel_success(id)?;
            russh_sftp::server::run(
                channel.into_stream(),
                Sftp {
                    content: self.content.clone(),
                },
            )
            .await;
        } else {
            session.channel_failure(id)?;
        }
        Ok(())
    }
}

struct Sftp {
    content: Arc<Vec<u8>>,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en".into(),
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _: u32, _: HashMap<String, String>) -> Result<Version, StatusCode> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, _: String) -> Result<Name, StatusCode> {
        Ok(Name {
            id,
            files: vec![russh_sftp::protocol::File::dummy("/")],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        if path != PATH {
            return Err(StatusCode::NoSuchFile);
        }
        let attrs = FileAttributes {
            size: Some(self.content.len() as u64),
            ..FileAttributes::default()
        };
        Ok(Attrs { id, attrs })
    }

    async fn open(
        &mut self,
        id: u32,
        path: String,
        _: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        if path == PATH {
            Ok(Handle {
                id,
                handle: "h".into(),
            })
        } else {
            Err(StatusCode::NoSuchFile)
        }
    }

    async fn read(
        &mut self,
        id: u32,
        _: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= self.content.len() {
            return Err(StatusCode::Eof);
        }
        let end = (start + len as usize).min(self.content.len());
        Ok(Data {
            id,
            data: self.content[start..end].to_vec(),
        })
    }

    async fn close(&mut self, id: u32, _: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }
}

/// Starts the server; answers its port and the host key's fingerprint.
pub async fn serve(content: Vec<u8>) -> (u16, String) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let fingerprint = key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let config = russh::server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::from_millis(0)),
        ..Default::default()
    };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut server = Server {
        content: Arc::new(content),
    };
    tokio::spawn(async move {
        let _ = server.run_on_socket(Arc::new(config), &listener).await;
    });
    (port, fingerprint)
}
