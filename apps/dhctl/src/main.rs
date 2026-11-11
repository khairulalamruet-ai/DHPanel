//! DHPanel admin CLI for bootstrap, migrations and recovery.
//!
//! Reads the same configuration as the control plane (`DHPANEL_CONFIG`,
//! `DHPANEL_*` env, secret files).

use std::{io::BufRead as _, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use dhpanel_auth_core::{
    password::{Argon2Params, PasswordHasher},
    policy::PasswordPolicy,
};
use dhpanel_config::ControlConfig;
use dhpanel_database::{
    PoolSettings,
    accounts::{InsertAccountError, NewAccount},
    sqlx::PgPool,
};
use dhpanel_domain::{AccountKind, Email, Username};
use secrecy::SecretString;

/// `EX_USAGE` from sysexits.h (invalid input).
const EXIT_USAGE: u8 = 64;
/// `EX_DATAERR` from sysexits.h (input conflicts with existing data).
const EXIT_DATAERR: u8 = 65;
/// `EX_UNAVAILABLE` from sysexits.h (database unreachable or migration failed).
const EXIT_UNAVAILABLE: u8 = 69;
/// `EX_SOFTWARE` from sysexits.h (internal error).
const EXIT_SOFTWARE: u8 = 70;
/// `EX_IOERR` from sysexits.h (files could not be read or written).
const EXIT_IOERR: u8 = 74;
/// `EX_CONFIG` from sysexits.h.
const EXIT_CONFIG: u8 = 78;
/// Where the control plane keeps its secrets, including the node CA.
const DEFAULT_CA_DIR: &str = "/etc/dhpanel/secrets";

#[derive(Debug, Parser)]
#[command(name = "dhctl", version, about = "DHPanel admin CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Apply pending database migrations.
    Migrate {
        #[command(subcommand)]
        action: Option<MigrateAction>,
    },
    /// Manage administrator accounts.
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
    /// Manage the internal certificate authority for node mTLS.
    Ca {
        #[command(subcommand)]
        action: CaAction,
    },
}

#[derive(Debug, Subcommand)]
enum CaAction {
    /// Create the CA (key `node_ca.key` mode 0600, certificate `node_ca.crt`).
    /// Refuses to overwrite an existing CA: replacing it would orphan every
    /// enrolled node.
    Init {
        /// Directory for the CA files (created with mode 0700 if missing).
        #[arg(long, default_value = DEFAULT_CA_DIR)]
        dir: PathBuf,
        /// Common name of the CA certificate.
        #[arg(long, default_value = "DHPanel Node CA")]
        name: String,
        /// Validity of the CA certificate in years (1..=20).
        #[arg(long, default_value_t = 10)]
        years: u32,
    },
    /// Show the CA's name, fingerprint and expiry.
    Show {
        /// Directory holding the CA files.
        #[arg(long, default_value = DEFAULT_CA_DIR)]
        dir: PathBuf,
    },
    /// Issue a server certificate for the control plane's agent port.
    IssueServer {
        /// Directory holding the CA files.
        #[arg(long, default_value = DEFAULT_CA_DIR)]
        dir: PathBuf,
        /// DNS name or IP address of the server; repeat for several. The
        /// first becomes the common name.
        #[arg(long = "dns", required = true)]
        names: Vec<String>,
        /// Directory to write `server.crt` and `server.key` to.
        #[arg(long)]
        out_dir: PathBuf,
        /// Validity in days (1..=400).
        #[arg(long, default_value_t = 365)]
        days: u32,
    },
}

#[derive(Debug, Subcommand)]
enum MigrateAction {
    /// List migrations and whether each is applied.
    Info,
}

#[derive(Debug, Subcommand)]
enum AdminAction {
    /// Create an administrator account. Prompts for the password (not echoed).
    Create {
        /// Login name: 3-32 characters, a-z, 0-9, '_' or '-', starting with a letter.
        #[arg(long)]
        username: String,
        /// Contact email address.
        #[arg(long)]
        email: String,
        /// Read the password from the first line of stdin instead of prompting
        /// (for automation). Passwords are never accepted as arguments.
        #[arg(long)]
        password_stdin: bool,
        /// Role to grant. The default gives full access; use `admin` or
        /// `support` for restricted administrators.
        #[arg(long, default_value = "super_admin")]
        role: String,
        /// Make two-factor authentication mandatory for this account: until it
        /// enrols a second factor, it can only use its own security pages.
        #[arg(long)]
        require_mfa: bool,
    },
}

/// A failure with its exit code. Messages never contain secrets.
#[derive(Debug)]
struct Failure(u8, String);

impl Failure {
    fn exit(self) -> ExitCode {
        eprintln!("dhctl: {}", self.1);
        ExitCode::from(self.0)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Migrate { action } => migrate(action).await,
        Command::Admin {
            action:
                AdminAction::Create {
                    username,
                    email,
                    password_stdin,
                    role,
                    require_mfa,
                },
        } => admin_create(&username, &email, password_stdin, &role, require_mfa).await,
        Command::Ca { action } => ca(action),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => f.exit(),
    }
}

fn pki_failure(err: dhpanel_pki::PkiError) -> Failure {
    use dhpanel_pki::PkiError as E;
    let code = match &err {
        E::AlreadyInitialised(_) => EXIT_DATAERR,
        E::InvalidInput(_) => EXIT_USAGE,
        E::InvalidCa(_) | E::InsecureKeyPermissions { .. } => EXIT_CONFIG,
        E::Io { .. } => EXIT_IOERR,
        _ => EXIT_SOFTWARE,
    };
    Failure(code, err.to_string())
}

fn print_ca(ca: &dhpanel_pki::CertificateAuthority) {
    println!("name:        {}", ca.common_name());
    println!("fingerprint: sha256:{}", ca.fingerprint_sha256());
    println!(
        "valid:       {} to {}",
        dhpanel_pki::format_rfc3339(ca.not_before()),
        dhpanel_pki::format_rfc3339(ca.not_after())
    );
}

/// `dhctl ca ...`: no database needed, only the CA files.
fn ca(action: CaAction) -> Result<(), Failure> {
    let now = time::OffsetDateTime::now_utc();
    match action {
        CaAction::Init { dir, name, years } => {
            if !(1..=20).contains(&years) {
                return Err(Failure(EXIT_USAGE, "--years must be 1..=20".to_owned()));
            }
            let ca = dhpanel_pki::init_ca_dir(
                &dir,
                &name,
                now,
                time::Duration::days(i64::from(years) * 365),
            )
            .map_err(pki_failure)?;
            println!("created the node CA in {}", dir.display());
            println!(
                "certificate: {}",
                dir.join(dhpanel_pki::CA_CERT_FILE).display()
            );
            println!(
                "private key: {} (mode 0600)",
                dir.join(dhpanel_pki::CA_KEY_FILE).display()
            );
            print_ca(&ca);
            println!(
                "Back the private key up somewhere safe and separate: it cannot be recovered, and losing it means re-enrolling every node."
            );
            Ok(())
        }
        CaAction::Show { dir } => {
            print_ca(&dhpanel_pki::load_ca_dir(&dir, now).map_err(pki_failure)?);
            Ok(())
        }
        CaAction::IssueServer {
            dir,
            names,
            out_dir,
            days,
        } => {
            if !(1..=400).contains(&days) {
                return Err(Failure(EXIT_USAGE, "--days must be 1..=400".to_owned()));
            }
            let ca = dhpanel_pki::load_ca_dir(&dir, now).map_err(pki_failure)?;
            let (issued, key) = ca
                .issue_server_certificate(&names, now, time::Duration::days(i64::from(days)))
                .map_err(pki_failure)?;
            if !out_dir.exists() {
                std::fs::create_dir_all(&out_dir)
                    .map_err(|e| Failure(EXIT_IOERR, format!("{}: {e}", out_dir.display())))?;
            }
            let (cert_path, key_path) = (out_dir.join("server.crt"), out_dir.join("server.key"));
            dhpanel_pki::write_private_file(&key_path, secrecy::ExposeSecret::expose_secret(&key))
                .map_err(pki_failure)?;
            dhpanel_pki::write_public_file(&cert_path, &issued.cert_pem).map_err(pki_failure)?;
            println!("certificate: {}", cert_path.display());
            println!("private key: {} (mode 0600)", key_path.display());
            println!("subject:     {}", issued.subject_cn);
            println!("fingerprint: sha256:{}", issued.fingerprint_sha256);
            println!(
                "expires:     {}",
                dhpanel_pki::format_rfc3339(issued.not_after)
            );
            Ok(())
        }
    }
}

fn load_config() -> Result<ControlConfig, Failure> {
    ControlConfig::load().map_err(|e| Failure(EXIT_CONFIG, e.to_string()))
}

async fn connect(config: &ControlConfig) -> Result<PgPool, Failure> {
    let url = config
        .database_url()
        .map_err(|e| Failure(EXIT_CONFIG, e.to_string()))?;
    let settings = PoolSettings {
        max_connections: 1,
        application_name: "dhctl".to_owned(),
    };
    dhpanel_database::connect(&url, &settings)
        .await
        .map_err(|e| Failure(EXIT_UNAVAILABLE, format!("cannot connect to database: {e}")))
}

async fn migrate(action: Option<MigrateAction>) -> Result<(), Failure> {
    let config = load_config()?;
    let pool = connect(&config).await?;

    if action.is_none() {
        dhpanel_database::migrate(&pool)
            .await
            .map_err(|e| Failure(EXIT_UNAVAILABLE, format!("migration failed: {e}")))?;
        println!("migrations applied");
    }

    let rows = dhpanel_database::migration_status(&pool)
        .await
        .map_err(|e| {
            Failure(
                EXIT_UNAVAILABLE,
                format!("cannot read migration status: {e}"),
            )
        })?;
    for m in rows {
        let mark = if m.applied { "applied" } else { "pending" };
        println!("{:>16}  {:<8} {}", m.version, mark, m.description);
    }
    Ok(())
}

/// Reads the password either from stdin (one line) or from the terminal twice.
fn read_password(from_stdin: bool) -> Result<SecretString, Failure> {
    if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| Failure(EXIT_USAGE, format!("cannot read password from stdin: {e}")))?;
        let trimmed = line.trim_end_matches(['\r', '\n']).to_owned();
        return Ok(SecretString::from(trimmed));
    }
    let first = rpassword::prompt_password("Password: ")
        .map_err(|e| Failure(EXIT_USAGE, format!("cannot read password: {e}")))?;
    let second = rpassword::prompt_password("Repeat password: ")
        .map_err(|e| Failure(EXIT_USAGE, format!("cannot read password: {e}")))?;
    if first != second {
        return Err(Failure(EXIT_USAGE, "passwords do not match".to_owned()));
    }
    Ok(SecretString::from(first))
}

async fn admin_create(
    username: &str,
    email: &str,
    password_stdin: bool,
    role: &str,
    require_mfa: bool,
) -> Result<(), Failure> {
    // Validate identifiers before asking for the password.
    let username =
        Username::parse(username).map_err(|e| Failure(EXIT_USAGE, format!("--username: {e}")))?;
    let email = Email::parse(email).map_err(|e| Failure(EXIT_USAGE, format!("--email: {e}")))?;

    let config = load_config()?;
    let password = read_password(password_stdin)?;
    if let Err(violations) =
        PasswordPolicy::default().check(&password, username.as_str(), email.as_str())
    {
        let reasons: Vec<String> = violations.iter().map(|v| v.message()).collect();
        return Err(Failure(
            EXIT_USAGE,
            format!("password rejected: {}", reasons.join(" ")),
        ));
    }

    let hasher = PasswordHasher::new(Argon2Params {
        memory_kib: config.auth.argon2_memory_kib,
        iterations: config.auth.argon2_iterations,
        parallelism: config.auth.argon2_parallelism,
    })
    .map_err(|e| Failure(EXIT_CONFIG, e.to_string()))?;
    let hash = hasher
        .hash_async(password)
        .await
        .map_err(|e| Failure(EXIT_SOFTWARE, e.to_string()))?;

    let pool = connect(&config).await?;
    let db_err = |e: dhpanel_database::sqlx::Error| {
        Failure(EXIT_UNAVAILABLE, format!("database error: {e}"))
    };
    // Checked before the account is created so a typo cannot leave an
    // administrator without any role.
    if !dhpanel_database::rbac::role_exists(&pool, role)
        .await
        .map_err(db_err)?
    {
        return Err(Failure(
            EXIT_USAGE,
            format!("--role: no role named '{role}'"),
        ));
    }
    let existing_admins = dhpanel_database::accounts::count_accounts(&pool, AccountKind::Admin)
        .await
        .map_err(|e| Failure(EXIT_UNAVAILABLE, format!("database error: {e}")))?;

    let id = dhpanel_database::accounts::insert_account(
        &pool,
        &NewAccount {
            kind: AccountKind::Admin,
            username: &username,
            email: &email,
            password_hash: &hash,
        },
    )
    .await
    .map_err(|e| match e {
        InsertAccountError::UsernameTaken | InsertAccountError::EmailTaken => {
            Failure(EXIT_DATAERR, e.to_string())
        }
        InsertAccountError::Database(db) => {
            Failure(EXIT_UNAVAILABLE, format!("database error: {db}"))
        }
    })?;

    let first = if existing_admins == 0 {
        " (first administrator)"
    } else {
        ""
    };
    match dhpanel_database::rbac::assign_role(&pool, id, role, None)
        .await
        .map_err(db_err)?
    {
        dhpanel_database::rbac::AssignRole::UnknownRole => {
            return Err(Failure(
                EXIT_SOFTWARE,
                format!(
                    "account {id} was created but role '{role}' vanished; grant a role before use"
                ),
            ));
        }
        dhpanel_database::rbac::AssignRole::Assigned
        | dhpanel_database::rbac::AssignRole::AlreadyAssigned => {}
    }
    if require_mfa {
        let mut conn = pool.acquire().await.map_err(db_err)?;
        dhpanel_database::admins::update_admin(&mut conn, id, None, None, Some(true))
            .await
            .map_err(|e| Failure(EXIT_UNAVAILABLE, format!("database error: {e}")))?;
    }
    let mfa = if require_mfa { " (2FA required)" } else { "" };
    println!("created admin '{username}' <{email}> id={id} role={role}{first}{mfa}");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("dhctl-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn ca_init_issue_and_show_work_end_to_end_and_never_overwrite() {
        let base = temp_dir("ca");
        let (dir, out) = (base.join("secrets"), base.join("tls"));
        let init = |years| {
            ca(CaAction::Init {
                dir: dir.clone(),
                name: "Test CA".into(),
                years,
            })
        };
        // Bad input is a usage error and creates nothing.
        for years in [0, 21] {
            assert_eq!(init(years).unwrap_err().0, EXIT_USAGE, "{years}");
        }
        assert!(!dir.exists());
        init(10).unwrap();
        assert!(dir.join(dhpanel_pki::CA_KEY_FILE).exists());
        // A second init is a data conflict and leaves the key alone.
        let key_before = std::fs::read(dir.join(dhpanel_pki::CA_KEY_FILE)).unwrap();
        assert_eq!(init(10).unwrap_err().0, EXIT_DATAERR);
        assert_eq!(
            std::fs::read(dir.join(dhpanel_pki::CA_KEY_FILE)).unwrap(),
            key_before
        );

        ca(CaAction::Show { dir: dir.clone() }).unwrap();
        let issue = |days, names: Vec<String>| {
            ca(CaAction::IssueServer {
                dir: dir.clone(),
                names,
                out_dir: out.clone(),
                days,
            })
        };
        assert_eq!(
            issue(0, vec!["a.example.com".into()]).unwrap_err().0,
            EXIT_USAGE
        );
        assert_eq!(
            issue(30, vec!["*.example.com".into()]).unwrap_err().0,
            EXIT_USAGE
        );
        assert!(
            !out.join("server.key").exists(),
            "nothing written on bad input"
        );
        issue(30, vec!["control.example.com".into(), "10.0.0.5".into()]).unwrap();
        assert!(out.join("server.crt").exists() && out.join("server.key").exists());
        // Never replaces existing server files either.
        assert_eq!(
            issue(30, vec!["control.example.com".into()]).unwrap_err().0,
            EXIT_IOERR
        );
        // No CA, no certificates.
        let missing = ca(CaAction::Show {
            dir: base.join("nowhere"),
        })
        .unwrap_err();
        assert_eq!(missing.0, EXIT_IOERR);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn password_is_never_a_cli_argument() {
        let cmd = Cli::command();
        let admin = cmd.find_subcommand("admin").expect("admin");
        let create = admin.find_subcommand("create").expect("create");
        assert!(
            create
                .get_arguments()
                .all(|a| a.get_id().as_str() != "password"),
            "a --password flag would expose secrets via ps/shell history"
        );
    }

    #[tokio::test]
    async fn invalid_username_is_rejected_before_any_io() {
        let Err(f) = admin_create("No Spaces", "a@b.co", true, "super_admin", false).await else {
            unreachable!("must fail");
        };
        assert_eq!(f.0, EXIT_USAGE);
        assert!(f.1.contains("--username"));
    }
}
