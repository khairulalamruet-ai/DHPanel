//! DHPanel admin CLI for bootstrap, migrations and recovery.
//!
//! Reads the same configuration as the control plane (`DHPANEL_CONFIG`,
//! `DHPANEL_*` env, secret files).

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dhpanel_config::ControlConfig;
use dhpanel_database::PoolSettings;

/// `EX_CONFIG` from sysexits.h.
const EXIT_CONFIG: u8 = 78;
/// `EX_UNAVAILABLE` from sysexits.h (database unreachable or migration failed).
const EXIT_UNAVAILABLE: u8 = 69;

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
}

#[derive(Debug, Subcommand)]
enum MigrateAction {
    /// List migrations and whether each is applied.
    Info,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Migrate { action } => migrate(action).await,
    }
}

async fn migrate(action: Option<MigrateAction>) -> ExitCode {
    let config = match ControlConfig::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("dhctl: {err}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let url = match config.database_url() {
        Ok(u) => u,
        Err(err) => {
            eprintln!("dhctl: {err}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let settings = PoolSettings {
        max_connections: 1,
        application_name: "dhctl".to_owned(),
    };
    let pool = match dhpanel_database::connect(&url, &settings).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("dhctl: cannot connect to database: {err}");
            return ExitCode::from(EXIT_UNAVAILABLE);
        }
    };

    if action.is_none() {
        if let Err(err) = dhpanel_database::migrate(&pool).await {
            eprintln!("dhctl: migration failed: {err}");
            return ExitCode::from(EXIT_UNAVAILABLE);
        }
        println!("migrations applied");
    }

    match dhpanel_database::migration_status(&pool).await {
        Ok(rows) => {
            for m in rows {
                let mark = if m.applied { "applied" } else { "pending" };
                println!("{:>16}  {:<8} {}", m.version, mark, m.description);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("dhctl: cannot read migration status: {err}");
            ExitCode::from(EXIT_UNAVAILABLE)
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
