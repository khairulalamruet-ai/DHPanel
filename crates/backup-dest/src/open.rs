//! Opens a destination from its stored settings and secrets (shared by the node agent, which
//! backs up accounts, and the control plane, which backs up itself).

use crate::{
    DestError, Destination,
    local::Local,
    s3::{S3, S3Config},
    sftp::{Login, Sftp, SftpConfig},
};

/// Why a destination could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// A setting or a secret is missing or wrong (not worth trying again).
    #[error("the destination: {0}")]
    Settings(&'static str),
    /// The destination itself said no (worth trying again).
    #[error(transparent)]
    Destination(#[from] DestError),
}

fn text<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

/// Opens a destination of `kind` (`local`, `s3` or `sftp`) with its plain `settings` and the
/// JSON `secrets` (`null` when it has none). Blocking: an SFTP login happens here.
///
/// # Errors
/// [`OpenError`].
pub fn open(
    kind: &str,
    settings: &serde_json::Value,
    secrets: &serde_json::Value,
) -> Result<Box<dyn Destination>, OpenError> {
    let bad = OpenError::Settings;
    let s = settings;
    match kind {
        "local" => {
            let path = text(s, "path").ok_or_else(|| bad("no folder"))?;
            Ok(Box::new(Local::open(std::path::Path::new(path))?))
        }
        "s3" => {
            let config = S3Config {
                endpoint: text(s, "endpoint")
                    .ok_or_else(|| bad("no endpoint"))?
                    .to_owned(),
                bucket: text(s, "bucket")
                    .ok_or_else(|| bad("no bucket"))?
                    .to_owned(),
                region: text(s, "region").unwrap_or("us-east-1").to_owned(),
                prefix: text(s, "prefix").unwrap_or("").trim_matches('/').to_owned(),
                path_style: s
                    .get("path_style")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                access_key: text(secrets, "access_key")
                    .ok_or_else(|| bad("no access key"))?
                    .to_owned(),
                secret_key: text(secrets, "secret_key")
                    .ok_or_else(|| bad("no secret key"))?
                    .to_owned(),
            };
            Ok(Box::new(S3::new(config)?))
        }
        "sftp" => {
            let login = match (text(secrets, "password"), text(secrets, "private_key")) {
                (Some(p), None) => Login::Password(p.to_owned()),
                (None, Some(k)) => Login::Key(k.to_owned()),
                _ => return Err(bad("no password or key")),
            };
            let config = SftpConfig {
                host: text(s, "host").ok_or_else(|| bad("no host"))?.to_owned(),
                port: s
                    .get("port")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|p| u16::try_from(p).ok())
                    .unwrap_or(22),
                user: text(s, "user").ok_or_else(|| bad("no user"))?.to_owned(),
                path: text(s, "path").unwrap_or("/").to_owned(),
                host_key_sha256: text(s, "host_key_sha256").map(str::to_owned),
                login,
            };
            Ok(Box::new(Sftp::connect(&config)?))
        }
        _ => Err(bad("an unknown kind")),
    }
}
