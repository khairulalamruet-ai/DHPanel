//! The node's shared database server (T07.01).
//!
//! One MariaDB container per node, named `dh-db`, with its data in the volume
//! `dh-db-data`. It is published on the node's loopback address only (the agent
//! reaches it there; nothing outside the node can), and it joins the network of
//! each tenant that may use databases, as `db`, so a tenant's containers reach it
//! and nobody else's do. Its root password is made here, kept in the agent's
//! secret store (`<state>/secrets/mariadb-root`, 0600) and never leaves the node.

use std::{
    io::Read as _,
    os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use dhpanel_docker_manager::{
    Capability, ContainerSpec, DockerError, Name, Network, PublishedPort, Restart, Tmpfs,
    VolumeMount,
};
use dhpanel_domain::TenantUsername;
use dhpanel_node_protocol::{AgentError, ErrorCode, Operation, Output};
use dhpanel_tenant_stack::{Allowlist, Role, network_name};

use super::tenant::{TenantContext, docker_error};

/// The database container.
pub const CONTAINER: &str = "dh-db";
/// Its network (the tenants' networks are added to it as it is used).
const NETWORK: &str = "dh-db-net";
/// Its data.
const VOLUME: &str = "dh-db-data";
/// The loopback port it is published on unless `DHPANEL_AGENT_DB_PORT` says another.
pub const DEFAULT_PORT: u16 = 3307;
/// The name tenants reach it by on their own network.
pub const ALIAS: &str = "db";
/// Longest wait for it to accept connections.
const READY_TIMEOUT: Duration = Duration::from_secs(120);

const MIB: u64 = 1024 * 1024;

/// The loopback port of the database server.
#[must_use]
pub fn port() -> u16 {
    std::env::var("DHPANEL_AGENT_DB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .filter(|p| *p >= 1024)
        .unwrap_or(DEFAULT_PORT)
}

/// Where the root password is kept: next to the agent's other secrets.
fn secrets_dir(context: &TenantContext) -> PathBuf {
    context.vhosts_root.with_file_name("secrets")
}

fn disk(what: &str, err: &std::io::Error) -> AgentError {
    AgentError::new(
        ErrorCode::Internal,
        format!("cannot {what}: {}", err.kind()),
    )
    .retryable()
}

/// 64 hex digits from the system's random source.
fn random_secret() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The root password: read from the secret store, made first if there is none.
///
/// # Errors
/// The disk failing (retryable).
pub fn root_password(dir: &Path) -> Result<String, AgentError> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| disk("prepare the secret store", &e))?;
    let path = dir.join("mariadb-root");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let text = text.trim().to_owned();
        if text.len() >= 32 {
            return Ok(text);
        }
    }
    let secret = random_secret().map_err(|e| disk("make a password", &e))?;
    let tmp = dir.join(".mariadb-root.tmp");
    let _ = std::fs::remove_file(&tmp);
    let write = || -> std::io::Result<()> {
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        std::io::Write::write_all(&mut out, secret.as_bytes())?;
        out.sync_all()?;
        std::fs::rename(&tmp, &path)
    };
    write().map_err(|e| disk("keep the password", &e))?;
    Ok(secret)
}

fn name(text: &str) -> Result<Name, AgentError> {
    Name::parse(text).map_err(|e| AgentError::new(ErrorCode::Internal, e.to_string()))
}

/// The connection options for the root account on the loopback port.
fn root_opts(password: &str) -> mysql_async::Opts {
    mysql_async::OptsBuilder::default()
        .ip_or_hostname("127.0.0.1")
        .tcp_port(port())
        .user(Some("root"))
        .pass(Some(password))
        .into()
}

/// Waits until the server answers a statement.
async fn wait_ready(password: &str) -> Result<(), AgentError> {
    use mysql_async::prelude::Queryable as _;
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    loop {
        let attempt = async {
            let mut conn = mysql_async::Conn::new(root_opts(password)).await?;
            conn.query_drop("SELECT 1").await?;
            conn.disconnect().await
        };
        if attempt.await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AgentError::new(
                ErrorCode::Internal,
                "the database server did not come up in time",
            )
            .retryable());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Makes sure the server runs. Returns whether this call made the container.
///
/// # Errors
/// Docker or the disk failing (retryable), or the server not coming up.
pub async fn ensure_server(context: &TenantContext) -> Result<bool, AgentError> {
    let password = root_password(&secrets_dir(context))?;
    let docker = &context.docker;
    let container = name(CONTAINER)?;
    let created = match docker.inspect(&container).await {
        Ok(_) => false,
        Err(DockerError::NotFound) => {
            let allowlist = Allowlist::embedded().map_err(|e| {
                AgentError::new(
                    ErrorCode::Internal,
                    format!("the image list is broken: {e}"),
                )
            })?;
            let image = allowlist
                .current(Role::Db)
                .map_err(|e| AgentError::new(ErrorCode::Internal, e.to_string()))?
                .clone();
            let network = name(NETWORK)?;
            docker
                .ensure_network(&network, false)
                .await
                .map_err(|e| docker_error(&e))?;
            let volume = name(VOLUME)?;
            docker
                .create_volume(&volume)
                .await
                .map_err(|e| docker_error(&e))?;
            docker
                .pull_image(&image)
                .await
                .map_err(|e| docker_error(&e))?;
            let mut spec = ContainerSpec::new(container.clone(), image);
            spec.command = vec![
                "--skip-name-resolve=1".into(),
                "--local-infile=0".into(),
                "--skip-symbolic-links".into(),
                "--max-connections=300".into(),
            ];
            spec.env
                .insert("MARIADB_ROOT_PASSWORD".into(), password.clone());
            spec.env.insert("MARIADB_AUTO_UPGRADE".into(), "1".into());
            // The image starts as root and gives its own files to `mysql`: it needs
            // exactly the capabilities for that, and nothing else.
            spec.cap_add = vec![
                Capability::Chown,
                Capability::DacOverride,
                Capability::Fowner,
                Capability::Setgid,
                Capability::Setuid,
            ];
            spec.memory_bytes = 1024 * MIB;
            spec.cpu_millis = 2000;
            spec.pids_limit = 512;
            spec.tmpfs = vec![
                Tmpfs {
                    target: "/tmp".into(),
                    size_bytes: 256 * MIB,
                },
                Tmpfs {
                    target: "/run/mysqld".into(),
                    size_bytes: 16 * MIB,
                },
            ];
            spec.volumes = vec![VolumeMount {
                volume,
                target: "/var/lib/mysql".into(),
                read_only: false,
            }];
            spec.network = Network::Named(network);
            spec.ports = vec![PublishedPort {
                container_port: 3306,
                host_port: port(),
                host_ip: "127.0.0.1"
                    .parse()
                    .map_err(|_| AgentError::new(ErrorCode::Internal, "bad loopback address"))?,
            }];
            spec.restart = Restart::UnlessStopped;
            match docker.create_container(&spec).await {
                Ok(_) | Err(DockerError::Conflict(_)) => {}
                Err(e) => return Err(docker_error(&e)),
            }
            true
        }
        Err(e) => return Err(docker_error(&e)),
    };
    docker
        .start(&container)
        .await
        .map_err(|e| docker_error(&e))?;
    wait_ready(&password).await?;
    Ok(created)
}

/// A connection as the server's root, from the agent's secret store.
async fn root_conn(context: &TenantContext) -> Result<mysql_async::Conn, AgentError> {
    let password = root_password(&secrets_dir(context))?;
    mysql_async::Conn::new(root_opts(&password))
        .await
        .map_err(|e| sql_error(&e))
}

/// What went wrong with a statement, said without quoting anything the server sent
/// back (it may echo values).
fn sql_error(err: &mysql_async::Error) -> AgentError {
    let (message, retryable) = match err {
        mysql_async::Error::Io(_) => ("the database server is not reachable", true),
        mysql_async::Error::Server(e) if e.code == 1045 => {
            ("the database server refused the agent's password", false)
        }
        mysql_async::Error::Server(_) => ("the database server refused the statement", false),
        _ => ("the database server failed", true),
    };
    let error = AgentError::new(ErrorCode::Internal, message.to_owned());
    if retryable { error.retryable() } else { error }
}

fn identifier(name: &str) -> Result<String, AgentError> {
    dhpanel_domain::databases::check_identifier(name)
        .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))?;
    // Checked as plain letters, digits and `_` above, so nothing in it needs
    // escaping; the quotes keep a word the server knows from being read as one.
    Ok(format!("`{name}`"))
}

/// `create_database`.
///
/// # Errors
/// [`AgentError`]: a name that is not a plain identifier (not retryable, nothing
/// is sent to the server); the server not reachable (retryable).
pub async fn create_database(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let Operation::CreateDatabase { name } = op else {
        return Err(super::wrong_operation());
    };
    let quoted = identifier(&name)?;
    let mut conn = root_conn(&context).await?;
    let existed: Option<String> = conn
        .exec_first(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
            (&name,),
        )
        .await
        .map_err(|e| sql_error(&e))?;
    if existed.is_none() {
        conn.query_drop(format!(
            "CREATE DATABASE {quoted} CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci"
        ))
        .await
        .map_err(|e| sql_error(&e))?;
    }
    let _ = conn.disconnect().await;
    Ok(Output::DatabaseCreated {
        created: existed.is_none(),
    })
}

/// `drop_database`.
///
/// # Errors
/// As [`create_database`].
pub async fn drop_database(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let Operation::DropDatabase { name } = op else {
        return Err(super::wrong_operation());
    };
    let quoted = identifier(&name)?;
    let mut conn = root_conn(&context).await?;
    let existed: Option<String> = conn
        .exec_first(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
            (&name,),
        )
        .await
        .map_err(|e| sql_error(&e))?;
    if existed.is_some() {
        conn.query_drop(format!("DROP DATABASE {quoted}"))
            .await
            .map_err(|e| sql_error(&e))?;
    }
    let _ = conn.disconnect().await;
    Ok(Output::DatabaseDropped {
        existed: existed.is_some(),
    })
}

/// A user's name as a statement writes it. (The same plain-identifier rule as for
/// databases; checked, then quoted.)
fn user_literal(name: &str) -> Result<String, AgentError> {
    dhpanel_domain::databases::check_identifier(name)
        .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))?;
    Ok(format!("'{name}'"))
}

/// A host part, `ip/netmask`, as a statement writes it: only digits, dots and one
/// slash (checked), so nothing in it can end the literal.
fn host_literal(host: &str) -> Result<String, AgentError> {
    let ok = !host.is_empty()
        && host.len() <= 40
        && host
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b'/');
    if !ok {
        return Err(AgentError::new(
            ErrorCode::Internal,
            "the network address is not usable",
        ));
    }
    Ok(format!("'{host}'"))
}

/// The server's own hash of a password, made with a bound parameter, and checked to
/// be what a hash looks like before it is put into a statement (the server does
/// not accept parameters in `CREATE USER`, and the password is never written into
/// a statement itself).
async fn password_hash(conn: &mut mysql_async::Conn, password: &str) -> Result<String, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let hash: Option<String> = conn
        .exec_first("SELECT PASSWORD(?)", (password,))
        .await
        .map_err(|e| sql_error(&e))?;
    hash.filter(|h| {
        h.len() == 41 && h.starts_with('*') && h[1..].bytes().all(|b| b.is_ascii_hexdigit())
    })
    .ok_or_else(|| {
        AgentError::new(
            ErrorCode::Internal,
            "the database server gave no password hash",
        )
    })
}

/// Every host a user exists for.
async fn hosts_of(conn: &mut mysql_async::Conn, name: &str) -> Result<Vec<String>, AgentError> {
    use mysql_async::prelude::Queryable as _;
    conn.exec("SELECT Host FROM mysql.user WHERE User = ?", (name,))
        .await
        .map_err(|e| sql_error(&e))
}

/// `upsert_db_user`.
///
/// # Errors
/// [`AgentError`]: a bad name or password, or a new user without one (not
/// retryable); the tenant's network not found; the server not reachable (retryable).
pub async fn upsert_db_user(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let Operation::UpsertDbUser {
        username,
        name,
        password,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let invalid = |m: &str| AgentError::new(ErrorCode::InvalidArguments, m.to_owned());
    let user = user_literal(&name)?;
    let tenant = TenantUsername::parse(&username).map_err(|e| invalid(&e.to_string()))?;
    if let Some(p) = &password {
        dhpanel_domain::databases::check_password(&p.0).map_err(|e| invalid(&e.to_string()))?;
    }
    // Where it may connect from: its own tenant's network, never `%`.
    let network = network_name(&tenant).map_err(|e| invalid(&e.to_string()))?;
    let subnet = context
        .docker
        .network_subnet(&network)
        .await
        .map_err(|e| match e {
            DockerError::NotFound => invalid("the tenant has no network on this node"),
            other => docker_error(&other),
        })?;
    let host = dhpanel_domain::databases::host_for_network(&subnet).ok_or_else(|| {
        AgentError::new(
            ErrorCode::Internal,
            "the tenant's network range is not usable",
        )
    })?;
    let quoted_host = host_literal(&host)?;
    let mut conn = root_conn(&context).await?;
    let existing = hosts_of(&mut conn, &name).await?;
    let here = existing.contains(&host);
    let created = existing.is_empty();
    match (&password, here) {
        (None, false) => {
            return Err(invalid("a password is needed for a user that is new here"));
        }
        (Some(p), false) => {
            let hash = password_hash(&mut conn, &p.0).await?;
            conn.query_drop(format!(
                "CREATE USER {user}@{quoted_host} IDENTIFIED BY PASSWORD '{hash}' WITH MAX_USER_CONNECTIONS 50"
            ))
            .await
            .map_err(|e| sql_error(&e))?;
        }
        (_, true) => {}
    }
    if let (Some(p), true) = (&password, here) {
        let hash = password_hash(&mut conn, &p.0).await?;
        for h in &existing {
            conn.query_drop(format!(
                "ALTER USER {user}@{} IDENTIFIED BY PASSWORD '{hash}'",
                host_or_pattern(h)?
            ))
            .await
            .map_err(|e| sql_error(&e))?;
        }
    }
    let _ = conn.disconnect().await;
    Ok(Output::DbUserSet { created, host })
}

/// A host the server holds, as a literal: the ones this agent made are digits,
/// dots and a slash; anything else (someone made it by hand) is left alone.
fn host_or_pattern(host: &str) -> Result<String, AgentError> {
    host_literal(host)
}

/// `grant_db`.
///
/// # Errors
/// [`AgentError`]: a bad name or access level (not retryable); the user does not
/// exist; the server not reachable (retryable).
pub async fn grant_db(context: Arc<TenantContext>, op: Operation) -> Result<Output, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let Operation::GrantDb {
        database,
        user,
        access,
    } = op
    else {
        return Err(super::wrong_operation());
    };
    let level = dhpanel_domain::databases::Access::parse(&access)
        .ok_or_else(|| AgentError::new(ErrorCode::InvalidArguments, "an unknown access level"))?;
    let db = identifier(&database)?;
    let who = user_literal(&user)?;
    let mut conn = root_conn(&context).await?;
    let hosts = hosts_of(&mut conn, &user).await?;
    if hosts.is_empty() {
        return Err(AgentError::new(
            ErrorCode::InvalidArguments,
            "the database user does not exist here",
        ));
    }
    let privileges = level.privileges().join(", ");
    for host in &hosts {
        let account = format!("{who}@{}", host_or_pattern(host)?);
        revoke_all(&mut conn, &db, &account).await?;
        conn.query_drop(format!("GRANT {privileges} ON {db}.* TO {account}"))
            .await
            .map_err(|e| sql_error(&e))?;
    }
    let _ = conn.disconnect().await;
    Ok(Output::DbGranted)
}

/// Takes everything the account had in the database away; having nothing is fine.
async fn revoke_all(
    conn: &mut mysql_async::Conn,
    db: &str,
    account: &str,
) -> Result<(), AgentError> {
    use mysql_async::prelude::Queryable as _;
    match conn
        .query_drop(format!("REVOKE ALL PRIVILEGES ON {db}.* FROM {account}"))
        .await
    {
        Ok(()) => Ok(()),
        // 1141: no such grant for that user and host; 1147: none on that table.
        Err(mysql_async::Error::Server(e)) if matches!(e.code, 1141 | 1147) => Ok(()),
        Err(e) => Err(sql_error(&e)),
    }
}

/// `revoke_db`.
///
/// # Errors
/// As [`grant_db`].
pub async fn revoke_db(context: Arc<TenantContext>, op: Operation) -> Result<Output, AgentError> {
    let Operation::RevokeDb { database, user } = op else {
        return Err(super::wrong_operation());
    };
    let db = identifier(&database)?;
    let who = user_literal(&user)?;
    let mut conn = root_conn(&context).await?;
    for host in hosts_of(&mut conn, &user).await? {
        revoke_all(
            &mut conn,
            &db,
            &format!("{who}@{}", host_or_pattern(&host)?),
        )
        .await?;
    }
    let _ = conn.disconnect().await;
    Ok(Output::DbRevoked)
}

/// `drop_db_user`.
///
/// # Errors
/// As [`grant_db`].
pub async fn drop_db_user(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    use mysql_async::prelude::Queryable as _;
    let Operation::DropDbUser { name } = op else {
        return Err(super::wrong_operation());
    };
    let who = user_literal(&name)?;
    let mut conn = root_conn(&context).await?;
    let hosts = hosts_of(&mut conn, &name).await?;
    for host in &hosts {
        conn.query_drop(format!("DROP USER {who}@{}", host_or_pattern(host)?))
            .await
            .map_err(|e| sql_error(&e))?;
    }
    let _ = conn.disconnect().await;
    Ok(Output::DbUserDropped {
        existed: !hosts.is_empty(),
    })
}

/// `ensure_db_server`.
///
/// # Errors
/// As [`ensure_server`].
pub async fn ensure_db_server(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::EnsureDbServer = op else {
        return Err(super::wrong_operation());
    };
    let created = ensure_server(&context).await?;
    Ok(Output::DbServerReady {
        port: port(),
        created,
    })
}

/// `allow_tenant_databases`.
///
/// # Errors
/// As [`ensure_server`]; a tenant that has no network here (not retryable).
pub async fn allow_tenant_databases(
    context: Arc<TenantContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::AllowTenantDatabases { username } = op else {
        return Err(super::wrong_operation());
    };
    let username = TenantUsername::parse(&username)
        .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))?;
    let network = network_name(&username)
        .map_err(|e| AgentError::new(ErrorCode::InvalidArguments, e.to_string()))?;
    ensure_server(&context).await?;
    match context
        .docker
        .connect_network(&name(CONTAINER)?, &network, ALIAS)
        .await
    {
        Ok(()) => {}
        Err(DockerError::NotFound) => {
            return Err(AgentError::new(
                ErrorCode::InvalidArguments,
                "the tenant has no network on this node",
            ));
        }
        Err(e) => return Err(docker_error(&e)),
    }
    Ok(Output::TenantDbAllowed {
        network: network.as_str().to_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn the_root_password_is_made_once_kept_private_and_read_back() {
        let dir = std::env::temp_dir().join(format!("dh-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = root_password(&dir).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(root_password(&dir).unwrap(), first, "kept, not made again");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join("mariadb-root")), 0o600);
        // A damaged file is replaced rather than used.
        std::fs::write(dir.join("mariadb-root"), "short").unwrap();
        let second = root_password(&dir).unwrap();
        assert_eq!(second.len(), 64);
        assert_ne!(second, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_port_is_a_loopback_port_above_1023() {
        assert_eq!(port(), DEFAULT_PORT);
    }
}
