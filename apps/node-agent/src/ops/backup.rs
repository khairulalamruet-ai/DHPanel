//! `create_backup` (T09.04): an account's files, databases and setup data, packed,
//! compressed, encrypted and stored at a destination.
//!
//! * The **files** are packed by the privileged helper as the tenant's own user (see
//!   `dhpanel_fileops::backup`) into a staged file in the agent's transfer folder, then
//!   stored and the staged file removed.
//! * Each **database** is dumped by `mariadb-dump` in the database container as a
//!   temporary read-only account (a consistent snapshot: `--single-transaction`) and flows
//!   dump -> zstd -> encryption -> the destination without touching the disk.
//! * **DNS** and **meta** are small JSON documents, encrypted like the rest.
//! * The **manifest** lists every part with its stored size and SHA-256 and is stored last:
//!   a backup without a manifest is not a backup, and a failure removes what was stored.
//!
//! The key and the destination's credentials arrive with the one operation that needs them.

use std::{
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt as _,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use dhpanel_backup::{
    KEY_LEN, Manifest, Part, PartKind, StreamReader, StreamWriter,
    manifest::{Counting, hex},
};
use dhpanel_backup_dest::{
    Destination,
    local::Local,
    s3::{S3, S3Config},
    sftp::{Login, Sftp, SftpConfig},
};
use dhpanel_node_protocol::{
    AgentError, BackupPartInfo, BackupTarget, DbPassword, Operation, Output,
};
use dhpanel_priv_helper::protocol::Action;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    db,
    sqlexport::{MAX_DURATION, drop_account, reader_account},
    sqlimport::{failed, invalid},
    tenant::{TenantContext, ask_helper, docker_error},
};

/// A destination shared by the pieces of one backup (used one object at a time).
pub(super) type Shared = Arc<Mutex<Box<dyn Destination>>>;

fn transfers_dir(context: &TenantContext) -> PathBuf {
    std::env::var_os("DHPANEL_AGENT_TRANSFERS").map_or_else(
        || context.vhosts_root.with_file_name("transfers"),
        PathBuf::from,
    )
}

fn text<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(serde_json::Value::as_str)
}

/// Opens the destination a backup goes to (blocking: an SFTP login happens here).
pub(super) fn open_destination(
    target: &BackupTarget,
    credentials: Option<&DbPassword>,
) -> Result<Box<dyn Destination>, AgentError> {
    let bad = |what: &str| invalid(&format!("the destination: {what}"));
    let secrets: serde_json::Value = credentials
        .map(|c| serde_json::from_str(&c.0).map_err(|_| bad("unreadable credentials")))
        .transpose()?
        .unwrap_or(serde_json::Value::Null);
    let dest_error = |e: dhpanel_backup_dest::DestError| failed(&e.to_string()).retryable();
    let s = &target.settings;
    match target.kind.as_str() {
        "local" => {
            let path = text(s, "path").ok_or_else(|| bad("no folder"))?;
            Ok(Box::new(
                Local::open(std::path::Path::new(path)).map_err(dest_error)?,
            ))
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
                access_key: text(&secrets, "access_key")
                    .ok_or_else(|| bad("no access key"))?
                    .to_owned(),
                secret_key: text(&secrets, "secret_key")
                    .ok_or_else(|| bad("no secret key"))?
                    .to_owned(),
            };
            Ok(Box::new(S3::new(config).map_err(dest_error)?))
        }
        "sftp" => {
            let login = match (text(&secrets, "password"), text(&secrets, "private_key")) {
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
            Ok(Box::new(Sftp::connect(&config).map_err(dest_error)?))
        }
        _ => Err(bad("an unknown kind")),
    }
}

/// Reads another reader and counts and hashes what passes (the stored size and checksum).
struct HashRead<R: Read> {
    inner: R,
    hasher: Sha256,
    size: u64,
}

impl<R: Read> Read for HashRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.size += n as u64;
        Ok(n)
    }
}

/// Stores what `reader` gives under `name`; returns its stored size and SHA-256.
fn store(dest: &Shared, name: &str, reader: impl Read) -> Result<(u64, String), AgentError> {
    let mut counted = HashRead {
        inner: reader,
        hasher: Sha256::new(),
        size: 0,
    };
    let guard = dest
        .lock()
        .map_err(|_| failed("the destination is not usable"))?;
    guard
        .put(name, &mut counted)
        .map_err(|e| failed(&e.to_string()).retryable())?;
    Ok((counted.size, hex(&counted.hasher.finalize())))
}

/// An encrypted JSON document, stored.
fn json_part(
    dest: &Shared,
    name: &str,
    key: &[u8; KEY_LEN],
    doc: &serde_json::Value,
) -> Result<(u64, String), AgentError> {
    let bytes = serde_json::to_vec(doc).map_err(|_| invalid("a document cannot be encoded"))?;
    let mut w = StreamWriter::new(Vec::new(), key).map_err(|e| failed(&e.to_string()))?;
    w.write_all(&bytes).map_err(|e| failed(&e.to_string()))?;
    let sealed = w.finish().map_err(|e| failed(&e.to_string()))?;
    store(dest, name, &sealed[..])
}

/// A writer into a bounded channel, so a stage can run in a thread of its own.
struct PipeWriter(std::sync::mpsc::SyncSender<Vec<u8>>);

impl Write for PipeWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.send(data.to_vec()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "nothing is taking it")
        })?;
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct PipeReader {
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    at: usize,
}

impl Read for PipeReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.at >= self.buf.len() {
            match self.rx.recv() {
                Ok(next) => {
                    self.buf = next;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// The key from its 64 hex digits.
pub(super) fn parse_key(hex_key: &DbPassword) -> Result<[u8; KEY_LEN], AgentError> {
    let text = hex_key.0.trim();
    if text.len() != KEY_LEN * 2 {
        return Err(invalid("the key is not 64 hex digits"));
    }
    let mut key = [0u8; KEY_LEN];
    for (i, b) in key.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
            .map_err(|_| invalid("the key is not hex"))?;
    }
    Ok(key)
}

/// One database, dumped, compressed, encrypted and stored while it is made.
async fn database_part(
    context: &TenantContext,
    dest: &Shared,
    object: String,
    database: &str,
    key: [u8; KEY_LEN],
) -> Result<(u64, String), AgentError> {
    let tag = Uuid::now_v7().simple().to_string()[..12].to_owned();
    let (account, password) = reader_account(context, database, &tag).await?;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let (pipe_tx, pipe_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
    let packer = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let encrypted = StreamWriter::new(PipeWriter(pipe_tx), &key)?;
        let mut zst = zstd::stream::Encoder::new(encrypted, 3)?;
        while let Some(piece) = rx.blocking_recv() {
            zst.write_all(&piece)?;
        }
        zst.finish()?.finish()?;
        Ok(())
    });
    let storer = {
        let (dest, object) = (dest.clone(), object.clone());
        tokio::task::spawn_blocking(move || {
            store(
                &dest,
                &object,
                PipeReader {
                    rx: pipe_rx,
                    buf: Vec::new(),
                    at: 0,
                },
            )
        })
    };
    let container =
        dhpanel_docker_manager::Name::parse(db::CONTAINER).map_err(|e| failed(&e.to_string()))?;
    let ran = context
        .docker
        .dump_database(&container, database, &account, &password, tx, MAX_DURATION)
        .await;
    drop_account(context, &account).await;
    let packed = packer
        .await
        .map_err(|_| failed("the packing was cancelled"))?;
    let stored = storer
        .await
        .map_err(|_| failed("the storing was cancelled"))?;
    let (code, stderr) = ran.map_err(|e| docker_error(&e))?;
    if code != 0 {
        let said: String = stderr.lines().take(3).collect::<Vec<_>>().join(" ");
        return Err(failed(&format!(
            "the dump of a database failed (exit {code}): {}",
            said.chars().take(300).collect::<String>()
        )));
    }
    packed.map_err(|e| failed(&format!("cannot pack the dump: {}", e.kind())))?;
    stored
}

/// `create_backup`.
///
/// # Errors
/// [`AgentError`]: bad arguments (not retryable); the destination, the database server or
/// the helper failing (retryable). On a failure what was stored is removed.
pub async fn create_backup(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::CreateBackup {
        tenant_id,
        uid,
        backup_id,
        account,
        target,
        credentials,
        key,
        databases,
        dns,
        meta,
        created_at,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    dhpanel_domain::TenantUsername::parse(&account).map_err(|e| invalid(&e.to_string()))?;
    for d in &databases {
        dhpanel_domain::databases::check_identifier(d).map_err(|e| invalid(&e.to_string()))?;
    }
    if databases.len() > 100 {
        return Err(invalid("too many databases"));
    }
    if serde_json::to_vec(&dns).map_or(usize::MAX, |v| v.len()) > 256 * 1024
        || serde_json::to_vec(&meta).map_or(usize::MAX, |v| v.len()) > 256 * 1024
    {
        return Err(invalid("the documents are too big"));
    }
    let key_text = key.ok_or_else(|| invalid("no key"))?;
    let key = parse_key(&key_text)?;
    let id = backup_id.simple().to_string();
    let dest: Shared = {
        let (target, credentials) = (target.clone(), credentials.clone());
        let opened =
            tokio::task::spawn_blocking(move || open_destination(&target, credentials.as_ref()))
                .await
                .map_err(|_| failed("the destination work was cancelled"))??;
        Arc::new(Mutex::new(opened))
    };
    let mut parts: Vec<Part> = Vec::new();
    let result = make_parts(
        &context, &dest, &id, tenant_id, uid, &key_text, key, &databases, &dns, &meta, &mut parts,
    )
    .await;
    let finish = match result {
        Ok(()) => finish_manifest(&dest, &id, &account, created_at, &parts, key).await,
        Err(e) => Err(e),
    };
    match finish {
        Ok((manifest_sha, manifest_size)) => {
            let size = parts.iter().map(|p| p.stored_bytes).sum::<u64>() + manifest_size;
            Ok(Output::BackupCreated {
                manifest_sha256: manifest_sha,
                size_bytes: size,
                parts: parts
                    .into_iter()
                    .map(|p| BackupPartInfo {
                        name: p.name,
                        kind: format!("{:?}", p.kind).to_ascii_lowercase(),
                        stored_bytes: p.stored_bytes,
                        sha256: p.sha256,
                        database: p.database,
                    })
                    .collect(),
            })
        }
        Err(e) => {
            // Nothing half done stays at the destination.
            let dest = dest.clone();
            let names: Vec<String> = parts
                .iter()
                .map(|p| format!("{id}/{}", p.name))
                .chain([format!("{id}/manifest.json.enc")])
                .collect();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(d) = dest.lock() {
                    for n in names {
                        let _ = d.delete(&n);
                    }
                }
            })
            .await;
            Err(e)
        }
    }
}

/// The tenant's files, packed by the helper into a staged file, stored as `object`.
async fn files_part(
    context: &TenantContext,
    dest: &Shared,
    object: &str,
    staged_name: &str,
    tenant_id: Uuid,
    uid: u32,
    key_text: &DbPassword,
) -> Result<(u64, String), AgentError> {
    let dir = transfers_dir(context);
    std::fs::create_dir_all(&dir)
        .map_err(|e| failed(&format!("cannot make the folder: {}", e.kind())).retryable())?;
    let staged = dir.join(staged_name);
    let _ = std::fs::remove_file(&staged);
    let socket = context.helper_socket.clone();
    let action = Action::BackupFiles {
        tenant_id: tenant_id.to_string(),
        uid,
        staged: staged_name.to_owned(),
        key: key_text.0.trim().to_owned(),
    };
    tokio::task::spawn_blocking(move || ask_helper(&socket, action))
        .await
        .map_err(|_| failed("the packing was cancelled"))??;
    let stored = {
        let (dest, staged2, object) = (dest.clone(), staged.clone(), object.to_owned());
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&staged2)
                .map_err(|e| failed(&format!("cannot read the packed files: {}", e.kind())))?;
            store(&dest, &object, file)
        })
        .await
        .map_err(|_| failed("the storing was cancelled"))?
    };
    let _ = std::fs::remove_file(&staged);
    stored
}

#[allow(clippy::too_many_arguments)]
async fn make_parts(
    context: &Arc<TenantContext>,
    dest: &Shared,
    id: &str,
    tenant_id: Uuid,
    uid: u32,
    key_text: &DbPassword,
    key: [u8; KEY_LEN],
    databases: &[String],
    dns: &serde_json::Value,
    meta: &serde_json::Value,
    parts: &mut Vec<Part>,
) -> Result<(), AgentError> {
    // 1. The files.
    let name = "files.tar.zst.enc";
    let (size, sha) = files_part(
        context,
        dest,
        &format!("{id}/{name}"),
        &format!("{id}.bk"),
        tenant_id,
        uid,
        key_text,
    )
    .await?;
    parts.push(Part {
        name: name.into(),
        kind: PartKind::Files,
        stored_bytes: size,
        sha256: sha,
        database: None,
    });
    // 2. The databases.
    for database in databases {
        let name = format!("db-{database}.sql.zst.enc");
        let (size, sha) =
            database_part(context, dest, format!("{id}/{name}"), database, key).await?;
        parts.push(Part {
            name,
            kind: PartKind::Database,
            stored_bytes: size,
            sha256: sha,
            database: Some(database.clone()),
        });
    }
    // 3. DNS and setup data.
    for (name, kind, doc) in [
        ("dns.json.enc", PartKind::Dns, dns),
        ("meta.json.enc", PartKind::Meta, meta),
    ] {
        let (dest2, object, doc) = (dest.clone(), format!("{id}/{name}"), doc.clone());
        let (size, sha) =
            tokio::task::spawn_blocking(move || json_part(&dest2, &object, &key, &doc))
                .await
                .map_err(|_| failed("the storing was cancelled"))??;
        parts.push(Part {
            name: name.into(),
            kind,
            stored_bytes: size,
            sha256: sha,
            database: None,
        });
    }
    Ok(())
}

async fn finish_manifest(
    dest: &Shared,
    id: &str,
    account: &str,
    created_at: i64,
    parts: &[Part],
    key: [u8; KEY_LEN],
) -> Result<(String, u64), AgentError> {
    let mut manifest = Manifest::new(id, account, created_at);
    manifest.parts = parts.to_vec();
    manifest.check().map_err(|e| failed(&e.to_string()))?;
    let json = manifest.to_json().map_err(|e| failed(&e.to_string()))?;
    let (dest, object) = (dest.clone(), format!("{id}/manifest.json.enc"));
    tokio::task::spawn_blocking(move || {
        let mut w = Counting::new(Vec::new());
        {
            let mut enc = StreamWriter::new(&mut w, &key).map_err(|e| failed(&e.to_string()))?;
            enc.write_all(&json).map_err(|e| failed(&e.to_string()))?;
            enc.finish().map_err(|e| failed(&e.to_string()))?;
        }
        let (sealed, size, sha) = w.finish();
        store(&dest, &object, &sealed[..])?;
        Ok((sha, size))
    })
    .await
    .map_err(|_| failed("the storing was cancelled"))?
}

/// Reads the backup's manifest (decrypted) from the destination.
fn read_manifest(dest: &Shared, id: &str, key: &[u8; KEY_LEN]) -> Result<Manifest, AgentError> {
    let guard = dest
        .lock()
        .map_err(|_| failed("the destination is not usable"))?;
    let raw = guard
        .get(&format!("{id}/manifest.json.enc"))
        .map_err(|e| failed(&format!("the manifest cannot be read: {e}")).retryable())?;
    let manifest = Manifest::from_reader(StreamReader::new(raw, key))
        .map_err(|e| invalid(&format!("the backup's manifest is not good: {e}")))?;
    if manifest.backup_id != id {
        return Err(invalid("the manifest belongs to another backup"));
    }
    Ok(manifest)
}

/// Reads one stored part and checks it against the manifest (size and SHA-256).
fn verify(dest: &Shared, id: &str, manifest: &Manifest, name: &str) -> Result<(), AgentError> {
    let guard = dest
        .lock()
        .map_err(|_| failed("the destination is not usable"))?;
    let stored = guard
        .get(&format!("{id}/{name}"))
        .map_err(|_| invalid(&format!("the part {name} is missing")))?;
    manifest
        .verify_part(name, stored)
        .map_err(|e| invalid(&format!("the backup is damaged: {e}")))
}

/// Puts a database part into a plain dump file (decrypted and decompressed), at most
/// `limit` bytes.
fn unpack_dump(
    dest: &Shared,
    object: &str,
    key: &[u8; KEY_LEN],
    path: &std::path::Path,
) -> Result<(), AgentError> {
    let guard = dest
        .lock()
        .map_err(|_| failed("the destination is not usable"))?;
    let stored = guard
        .get(object)
        .map_err(|e| failed(&format!("cannot read a part: {e}")).retryable())?;
    let mut plain = zstd::stream::Decoder::new(StreamReader::new(stored, key))
        .map_err(|e| failed(&format!("cannot open the dump: {}", e.kind())))?;
    let mut out = std::fs::File::create(path)
        .map_err(|e| failed(&format!("cannot write the dump: {}", e.kind())))?;
    std::io::copy(&mut plain, &mut out)
        .map_err(|e| failed(&format!("cannot unpack the dump: {}", e.kind())))?;
    Ok(())
}

/// `restore_backup`: see [`Operation::RestoreBackup`].
///
/// # Errors
/// [`AgentError`]: bad arguments or a damaged backup (not retryable, nothing was touched);
/// the destination, the database server or the helper failing (retryable).
pub async fn restore_backup(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::RestoreBackup {
        tenant_id,
        uid,
        backup_id,
        restore_id,
        account,
        target,
        credentials,
        key,
        files,
        folder,
        databases,
        snapshot,
        verify_only,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    dhpanel_domain::TenantUsername::parse(&account).map_err(|e| invalid(&e.to_string()))?;
    for d in &databases {
        dhpanel_domain::databases::check_identifier(d).map_err(|e| invalid(&e.to_string()))?;
    }
    if databases.len() > 100 {
        return Err(invalid("too many databases"));
    }
    if let Some(f) = &folder
        && (!files || f.len() > 1024 || f.contains(['\0', '\\']) || f.split('/').any(|p| p == ".."))
    {
        return Err(invalid("the folder is not usable"));
    }
    if !files && databases.is_empty() && !verify_only {
        return Err(invalid("nothing to restore"));
    }
    let key_text = key.ok_or_else(|| invalid("no key"))?;
    let key = parse_key(&key_text)?;
    let id = backup_id.simple().to_string();
    let dest: Shared = {
        let (target, credentials) = (target.clone(), credentials.clone());
        let opened =
            tokio::task::spawn_blocking(move || open_destination(&target, credentials.as_ref()))
                .await
                .map_err(|_| failed("the destination work was cancelled"))??;
        Arc::new(Mutex::new(opened))
    };
    // 1. The manifest, and every part that is needed, checked before anything is touched.
    let (manifest, needed) = {
        let (dest, id, account, databases) =
            (dest.clone(), id.clone(), account.clone(), databases.clone());
        tokio::task::spawn_blocking(move || -> Result<(Manifest, Vec<String>), AgentError> {
            let manifest = read_manifest(&dest, &id, &key)?;
            if manifest.account != account {
                return Err(invalid("the backup belongs to another account"));
            }
            let mut needed = Vec::new();
            for part in &manifest.parts {
                let wanted = match part.kind {
                    PartKind::Files => files || verify_only,
                    PartKind::Database => {
                        verify_only
                            || part
                                .database
                                .as_ref()
                                .is_some_and(|d| databases.contains(d))
                    }
                    PartKind::Dns | PartKind::Meta => verify_only,
                };
                if wanted {
                    needed.push(part.name.clone());
                }
            }
            if files && !manifest.parts.iter().any(|p| p.kind == PartKind::Files) {
                return Err(invalid("the backup has no files"));
            }
            for d in &databases {
                if !manifest
                    .parts
                    .iter()
                    .any(|p| p.database.as_deref() == Some(d.as_str()))
                {
                    return Err(invalid(&format!("the backup has no database {d}")));
                }
            }
            for name in &needed {
                verify(&dest, &id, &manifest, name)?;
            }
            Ok((manifest, needed))
        })
        .await
        .map_err(|_| failed("the check was cancelled"))??
    };
    let verified = u32::try_from(needed.len()).unwrap_or(u32::MAX);
    if verify_only {
        return Ok(Output::BackupRestored {
            verified_parts: verified,
            files_restored: false,
            databases: Vec::new(),
            snapshot: Vec::new(),
        });
    }
    // 2. The present state, kept (at the destination) so that a restore can be undone.
    let rid = restore_id.simple().to_string();
    let mut snap: Vec<BackupPartInfo> = Vec::new();
    if snapshot {
        let result = make_snapshot(
            &context, &dest, &rid, tenant_id, uid, &key_text, key, files, &databases, &mut snap,
        )
        .await;
        if let Err(e) = result {
            remove_snapshot(&dest, &rid, &snap).await;
            return Err(e);
        }
    }
    // 3. The files, over the present ones, as the tenant.
    if files {
        let name = manifest
            .parts
            .iter()
            .find(|p| p.kind == PartKind::Files)
            .map(|p| p.name.clone())
            .ok_or_else(|| invalid("the backup has no files"))?;
        restore_files_part(
            &context,
            &dest,
            &format!("{id}/{name}"),
            &rid,
            tenant_id,
            uid,
            &key_text,
            folder.as_deref(),
        )
        .await?;
    }
    // 4. The databases, replaced by their dumps.
    for database in &databases {
        let name = manifest
            .parts
            .iter()
            .find(|p| p.database.as_deref() == Some(database.as_str()))
            .map(|p| p.name.clone())
            .ok_or_else(|| invalid("the backup has no such database"))?;
        let dir = super::sqlimport::imports_dir(&context);
        std::fs::create_dir_all(&dir)
            .map_err(|e| failed(&format!("cannot prepare the folder: {}", e.kind())).retryable())?;
        let path = dir.join(format!("{rid}-{database}.sql"));
        let unpacked = {
            let (dest, object, path) = (dest.clone(), format!("{id}/{name}"), path.clone());
            tokio::task::spawn_blocking(move || unpack_dump(&dest, &object, &key, &path))
                .await
                .map_err(|_| failed("the unpacking was cancelled"))?
        };
        let ran = match unpacked {
            Ok(()) => {
                super::sqlimport::import_file(&context, database, restore_id, &path, false).await
            }
            Err(e) => Err(e),
        };
        let _ = std::fs::remove_file(&path);
        ran?;
    }
    Ok(Output::BackupRestored {
        verified_parts: verified,
        files_restored: files,
        databases,
        snapshot: snap,
    })
}

#[allow(clippy::too_many_arguments)]
async fn make_snapshot(
    context: &Arc<TenantContext>,
    dest: &Shared,
    rid: &str,
    tenant_id: Uuid,
    uid: u32,
    key_text: &DbPassword,
    key: [u8; KEY_LEN],
    files: bool,
    databases: &[String],
    snap: &mut Vec<BackupPartInfo>,
) -> Result<(), AgentError> {
    if files {
        let name = "files.tar.zst.enc";
        let (size, sha) = files_part(
            context,
            dest,
            &format!("pre-{rid}/{name}"),
            &format!("pre-{rid}.bk"),
            tenant_id,
            uid,
            key_text,
        )
        .await?;
        snap.push(BackupPartInfo {
            name: name.into(),
            kind: "files".into(),
            stored_bytes: size,
            sha256: sha,
            database: None,
        });
    }
    for database in databases {
        let name = format!("db-{database}.sql.zst.enc");
        let (size, sha) =
            database_part(context, dest, format!("pre-{rid}/{name}"), database, key).await?;
        snap.push(BackupPartInfo {
            name,
            kind: "database".into(),
            stored_bytes: size,
            sha256: sha,
            database: Some(database.clone()),
        });
    }
    Ok(())
}

async fn remove_snapshot(dest: &Shared, rid: &str, snap: &[BackupPartInfo]) {
    let dest = dest.clone();
    let names: Vec<String> = snap
        .iter()
        .map(|p| format!("pre-{rid}/{}", p.name))
        .chain([format!("pre-{rid}/files.tar.zst.enc")])
        .collect();
    let _ = tokio::task::spawn_blocking(move || {
        if let Ok(d) = dest.lock() {
            for n in names {
                let _ = d.delete(&n);
            }
        }
    })
    .await;
}

/// The stored files part, fetched to a staged file and put back by the helper.
#[allow(clippy::too_many_arguments)]
async fn restore_files_part(
    context: &TenantContext,
    dest: &Shared,
    object: &str,
    rid: &str,
    tenant_id: Uuid,
    uid: u32,
    key_text: &DbPassword,
    folder: Option<&str>,
) -> Result<(), AgentError> {
    let dir = transfers_dir(context);
    std::fs::create_dir_all(&dir)
        .map_err(|e| failed(&format!("cannot make the folder: {}", e.kind())).retryable())?;
    let staged_name = format!("{rid}.rs");
    let staged = dir.join(&staged_name);
    let _ = std::fs::remove_file(&staged);
    let fetched = {
        let (dest, object, staged) = (dest.clone(), object.to_owned(), staged.clone());
        tokio::task::spawn_blocking(move || -> Result<(), AgentError> {
            let guard = dest
                .lock()
                .map_err(|_| failed("the destination is not usable"))?;
            let mut reader = guard
                .get(&object)
                .map_err(|e| failed(&format!("cannot read the files: {e}")).retryable())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&staged)
                .map_err(|e| failed(&format!("cannot stage the files: {}", e.kind())))?;
            std::io::copy(&mut reader, &mut file)
                .map_err(|e| failed(&format!("cannot stage the files: {}", e.kind())))?;
            Ok(())
        })
        .await
        .map_err(|_| failed("the fetching was cancelled"))?
    };
    let result = match fetched {
        Ok(()) => {
            let socket = context.helper_socket.clone();
            let action = Action::RestoreFiles {
                tenant_id: tenant_id.to_string(),
                uid,
                staged: staged_name,
                key: key_text.0.trim().to_owned(),
                folder: folder.map(str::to_owned),
            };
            tokio::task::spawn_blocking(move || ask_helper(&socket, action))
                .await
                .map_err(|_| failed("the restoring was cancelled"))?
                .map(|_| ())
        }
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&staged);
    result
}

/// `delete_backup`: every object of the backup is removed from the destination.
///
/// # Errors
/// [`AgentError`]: bad arguments (not retryable); the destination failing (retryable).
pub async fn delete_backup(op: Operation) -> Result<Output, AgentError> {
    let Operation::DeleteBackup {
        backup_id,
        target,
        credentials,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let id = backup_id.simple().to_string();
    tokio::task::spawn_blocking(move || -> Result<Output, AgentError> {
        let dest = open_destination(&target, credentials.as_ref())?;
        let names = dest
            .list(&format!("{id}/"))
            .map_err(|e| failed(&e.to_string()).retryable())?;
        let mut removed = 0u32;
        for name in names {
            // Only what lies below this backup's own folder.
            if !name.starts_with(&format!("{id}/")) {
                continue;
            }
            dest.delete(&name)
                .map_err(|e| failed(&e.to_string()).retryable())?;
            removed += 1;
        }
        Ok(Output::BackupDeleted { removed })
    })
    .await
    .map_err(|_| failed("the removal was cancelled"))?
}
