//! DHPanel admin CLI for bootstrap, migrations and recovery.
//!
//! Reads the same configuration as the control plane (`DHPANEL_CONFIG`,
//! `DHPANEL_*` env, secret files).

use std::{io::BufRead as _, process::ExitCode};

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
/// `EX_CONFIG` from sysexits.h.
const EXIT_CONFIG: u8 = 78;

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
    },
}

/// A failure with its exit code. Messages never contain secrets.
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
                },
        } => admin_create(&username, &email, password_stdin).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => f.exit(),
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

async fn admin_create(username: &str, email: &str, password_stdin: bool) -> Result<(), Failure> {
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
    println!("created admin '{username}' <{email}> id={id}{first}");
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
        let Err(f) = admin_create("No Spaces", "a@b.co", true).await else {
            unreachable!("must fail");
        };
        assert_eq!(f.0, EXIT_USAGE);
        assert!(f.1.contains("--username"));
    }
}
