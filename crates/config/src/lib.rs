//! Typed service configuration for DHPanel binaries.
//!
//! Layers, lowest to highest priority:
//! 1. built-in defaults,
//! 2. a TOML file (default `/etc/dhpanel/control.toml`, override with `DHPANEL_CONFIG`),
//! 3. environment variables prefixed `DHPANEL_`, with `__` separating nested keys
//!    (e.g. `DHPANEL_DATABASE__MAX_CONNECTIONS=20`).
//!
//! Secrets (database and Redis URLs) can be given inline or, preferably, as
//! `*_file` paths. Secret files must not be readable by group or others.
//! Secrets never appear in `Debug` output.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// Default config file path for the control plane.
pub const DEFAULT_CONTROL_CONFIG: &str = "/etc/dhpanel/control.toml";
/// Environment variable that overrides the config file path.
pub const CONFIG_PATH_ENV: &str = "DHPANEL_CONFIG";

/// Control plane (`control-api` and `worker`) configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlConfig {
    /// Address the HTTP API binds to.
    pub listen: SocketAddr,
    /// Logging and tracing.
    pub log: LogConfig,
    /// PostgreSQL connection.
    pub database: DatabaseConfig,
    /// Redis connection.
    pub redis: RedisConfig,
}

/// Logging settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// `json` (default) or `pretty`.
    pub format: LogFormat,
    /// `EnvFilter` directives, e.g. `info,dhpanel=debug`.
    pub filter: String,
    /// Optional OTLP/HTTP traces endpoint.
    pub otlp_endpoint: Option<String>,
}

/// Log line format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// One JSON object per line.
    Json,
    /// Human-readable lines.
    Pretty,
}

/// PostgreSQL settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Connection URL (secret). Prefer `url_file`.
    #[serde(default, skip_serializing)]
    pub url: Option<SecretString>,
    /// File holding the connection URL.
    pub url_file: Option<PathBuf>,
    /// Pool size.
    pub max_connections: u32,
}

/// Redis settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    /// Connection URL (secret). Prefer `url_file`.
    #[serde(default, skip_serializing)]
    pub url: Option<SecretString>,
    /// File holding the connection URL.
    pub url_file: Option<PathBuf>,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8080)),
            log: LogConfig {
                format: LogFormat::Json,
                filter: "info".to_owned(),
                otlp_endpoint: None,
            },
            database: DatabaseConfig {
                url: None,
                url_file: None,
                max_connections: 10,
            },
            redis: RedisConfig {
                url: None,
                url_file: None,
            },
        }
    }
}

/// Configuration errors. Messages name the offending key and never include secret values.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Parsing or type errors from any layer, one line per problem, each naming
    /// its key and the layer (file or environment) it came from.
    #[error("configuration error: {}", problems.join("; "))]
    Parse {
        /// Human-readable problems.
        problems: Vec<String>,
    },
    /// A value is present but not acceptable.
    #[error("invalid value for `{key}`: {reason}")]
    Invalid {
        /// Dotted key, e.g. `database.max_connections`.
        key: &'static str,
        /// Human-readable reason.
        reason: String,
    },
    /// A required secret is missing.
    #[error("missing secret `{key}`: set `{key}_file` (preferred) or `{key}`")]
    MissingSecret {
        /// Dotted key, e.g. `database.url`.
        key: &'static str,
    },
    /// A secret file could not be read or has unsafe permissions.
    #[error("secret file {path} for `{key}`: {reason}")]
    SecretFile {
        /// Dotted key the file belongs to.
        key: &'static str,
        /// File path.
        path: PathBuf,
        /// Reason.
        reason: String,
    },
}

impl ControlConfig {
    /// Loads from defaults, the config file named by `DHPANEL_CONFIG` (or the
    /// default path, if it exists) and `DHPANEL_*` env, then validates.
    ///
    /// # Errors
    /// Returns [`ConfigError`] for parse, validation or secret-file problems.
    pub fn load() -> Result<Self, ConfigError> {
        let explicit = std::env::var_os(CONFIG_PATH_ENV).map(PathBuf::from);
        let path = explicit
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONTROL_CONFIG));
        if explicit.is_some() && !path.exists() {
            return Err(ConfigError::Invalid {
                key: "DHPANEL_CONFIG",
                reason: format!("file {} does not exist", path.display()),
            });
        }
        Self::load_from(&path)
    }

    /// Like [`ControlConfig::load`] but with an explicit file path. A missing
    /// file is treated as empty (defaults + env only).
    ///
    /// # Errors
    /// Returns [`ConfigError`] for parse, validation or secret-file problems.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let config: Self = Figment::from(Serialized::defaults(Self::default()))
            .merge(Toml::file(path))
            .merge(Env::prefixed("DHPANEL_").ignore(&["CONFIG"]).split("__"))
            .extract()
            .map_err(describe_figment_error)?;
        config.validate()?;
        Ok(config)
    }

    /// Checks invariants that types alone cannot express.
    ///
    /// # Errors
    /// Returns the first problem found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.listen.port() == 0 {
            return Err(ConfigError::Invalid {
                key: "listen",
                reason: "port must not be 0".to_owned(),
            });
        }
        if self.log.filter.trim().is_empty() {
            return Err(ConfigError::Invalid {
                key: "log.filter",
                reason: "must not be empty".to_owned(),
            });
        }
        if !(1..=500).contains(&self.database.max_connections) {
            return Err(ConfigError::Invalid {
                key: "database.max_connections",
                reason: format!("must be 1..=500, got {}", self.database.max_connections),
            });
        }
        if self.database.url.is_some() && self.database.url_file.is_some() {
            return Err(ConfigError::Invalid {
                key: "database.url",
                reason: "set either `url` or `url_file`, not both".to_owned(),
            });
        }
        if self.redis.url.is_some() && self.redis.url_file.is_some() {
            return Err(ConfigError::Invalid {
                key: "redis.url",
                reason: "set either `url` or `url_file`, not both".to_owned(),
            });
        }
        Ok(())
    }

    /// Resolves the PostgreSQL URL from `database.url` or `database.url_file`.
    ///
    /// # Errors
    /// Missing secret, unreadable file, unsafe permissions or wrong scheme.
    pub fn database_url(&self) -> Result<SecretString, ConfigError> {
        let url = resolve_secret(
            "database.url",
            self.database.url.as_ref(),
            self.database.url_file.as_deref(),
        )?;
        let s = url.expose_secret();
        if !(s.starts_with("postgres://") || s.starts_with("postgresql://")) {
            return Err(ConfigError::Invalid {
                key: "database.url",
                reason: "must start with postgres:// or postgresql://".to_owned(),
            });
        }
        Ok(url)
    }

    /// Resolves the Redis URL from `redis.url` or `redis.url_file`.
    ///
    /// # Errors
    /// Missing secret, unreadable file, unsafe permissions or wrong scheme.
    pub fn redis_url(&self) -> Result<SecretString, ConfigError> {
        let url = resolve_secret(
            "redis.url",
            self.redis.url.as_ref(),
            self.redis.url_file.as_deref(),
        )?;
        let s = url.expose_secret();
        if !(s.starts_with("redis://") || s.starts_with("rediss://")) {
            return Err(ConfigError::Invalid {
                key: "redis.url",
                reason: "must start with redis:// or rediss://".to_owned(),
            });
        }
        Ok(url)
    }
}

/// Turns figment's error chain into one line per problem:
/// `` `database.max_connections`: invalid type: found string "lots", expected u32 (from `DHPANEL_` environment variable(s)) ``.
/// Key paths come from figment; the `default` profile prefix is dropped.
fn describe_figment_error(err: figment::Error) -> ConfigError {
    let problems = err
        .into_iter()
        .map(|e| {
            let key = e
                .path
                .iter()
                .skip_while(|seg| seg.as_str() == "default")
                .cloned()
                .collect::<Vec<_>>()
                .join(".");
            let key = if key.is_empty() {
                "<root>".to_owned()
            } else {
                key
            };
            let source = e
                .metadata
                .as_ref()
                .map(|m| {
                    let mut s = m.name.to_string();
                    if let Some(src) = &m.source {
                        s = format!("{s} {src}");
                    }
                    s
                })
                .unwrap_or_else(|| "unknown source".to_owned());
            format!("`{key}`: {} (from {source})", e.kind)
        })
        .collect();
    ConfigError::Parse { problems }
}

fn resolve_secret(
    key: &'static str,
    inline: Option<&SecretString>,
    file: Option<&Path>,
) -> Result<SecretString, ConfigError> {
    match (inline, file) {
        (Some(value), _) => Ok(value.clone()),
        (None, Some(path)) => read_secret_file(key, path),
        (None, None) => Err(ConfigError::MissingSecret { key }),
    }
}

/// Reads a secret file, trimming one trailing newline. On Unix the file must
/// not be readable or writable by group or others (mode & 0o077 == 0).
fn read_secret_file(key: &'static str, path: &Path) -> Result<SecretString, ConfigError> {
    let err = |reason: String| ConfigError::SecretFile {
        key,
        path: path.to_owned(),
        reason,
    };
    let meta = std::fs::metadata(path).map_err(|e| err(e.to_string()))?;
    if !meta.is_file() {
        return Err(err("not a regular file".to_owned()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(err(format!(
                "permissions {mode:o} are too open; use 0600 or 0400"
            )));
        }
    }
    let raw = std::fs::read_to_string(path).map_err(|e| err(e.to_string()))?;
    let value = raw.strip_suffix('\n').unwrap_or(&raw);
    let value = value.strip_suffix('\r').unwrap_or(value);
    if value.is_empty() {
        return Err(err("file is empty".to_owned()));
    }
    Ok(SecretString::from(value.to_owned()))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::result_large_err)] // tests
mod tests {
    use figment::Jail;

    use super::*;

    fn load(jail: &Jail) -> Result<ControlConfig, ConfigError> {
        ControlConfig::load_from(&jail.directory().join("control.toml"))
    }

    #[test]
    fn defaults_load_without_file_or_env() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            let cfg = load(jail).expect("defaults are valid");
            assert_eq!(cfg.listen.to_string(), "127.0.0.1:8080");
            assert_eq!(cfg.log.format, LogFormat::Json);
            assert_eq!(cfg.database.max_connections, 10);
            Ok(())
        });
    }

    #[test]
    fn file_then_env_override_order() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.create_file(
                "control.toml",
                r#"
                listen = "0.0.0.0:9000"
                [log]
                format = "pretty"
                [database]
                max_connections = 20
                "#,
            )?;
            jail.set_env("DHPANEL_DATABASE__MAX_CONNECTIONS", "30");
            let cfg = load(jail).expect("valid");
            assert_eq!(cfg.listen.port(), 9000); // from file
            assert_eq!(cfg.log.format, LogFormat::Pretty); // from file
            assert_eq!(cfg.database.max_connections, 30); // env wins
            Ok(())
        });
    }

    #[test]
    fn unknown_key_is_rejected_with_its_name() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.create_file("control.toml", "lisen = \"0.0.0.0:1\"")?;
            let msg = load(jail).expect_err("typo must fail").to_string();
            assert!(msg.contains("`lisen`"), "{msg}");
            assert!(msg.contains("TOML file"), "{msg}");
            assert!(!msg.contains("default."), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn wrong_type_is_rejected() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.set_env("DHPANEL_DATABASE__MAX_CONNECTIONS", "lots");
            let msg = load(jail).expect_err("must fail").to_string();
            assert!(msg.contains("`database.max_connections`"), "{msg}");
            assert!(msg.contains("environment"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn out_of_range_value_names_the_key() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.set_env("DHPANEL_DATABASE__MAX_CONNECTIONS", "0");
            let msg = load(jail).expect_err("must fail").to_string();
            assert!(msg.contains("database.max_connections"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn missing_secret_is_reported() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            let cfg = load(jail).expect("valid");
            let msg = cfg.database_url().expect_err("missing").to_string();
            assert!(msg.contains("database.url_file"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn secret_never_appears_in_debug_output() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.set_env("DHPANEL_DATABASE__URL", "postgres://u:hunter2@db/x");
            let cfg = load(jail).expect("valid");
            assert!(!format!("{cfg:?}").contains("hunter2"));
            assert_eq!(
                cfg.database_url().expect("ok").expose_secret(),
                "postgres://u:hunter2@db/x"
            );
            Ok(())
        });
    }

    #[test]
    fn wrong_url_scheme_is_rejected() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.set_env("DHPANEL_REDIS__URL", "http://cache");
            let cfg = load(jail).expect("valid");
            let msg = cfg.redis_url().expect_err("scheme").to_string();
            assert!(msg.contains("redis://"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn inline_and_file_together_is_rejected() {
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            jail.set_env("DHPANEL_REDIS__URL", "redis://a");
            jail.set_env("DHPANEL_REDIS__URL_FILE", "/tmp/x");
            assert!(load(jail).is_err());
            Ok(())
        });
    }

    #[cfg(unix)]
    #[test]
    fn secret_file_permissions_are_enforced() {
        use std::os::unix::fs::PermissionsExt;
        Jail::expect_with(|jail| {
            // Isolate from the developer's shell (e.g. `devenv.sh env` exports).
            jail.clear_env();
            let path = jail.directory().join("db_url");
            std::fs::write(&path, "postgres://u:p@db/x\n").expect("write");
            jail.set_env("DHPANEL_DATABASE__URL_FILE", path.display());

            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            let cfg = load(jail).expect("valid");
            let msg = cfg.database_url().expect_err("too open").to_string();
            assert!(msg.contains("too open"), "{msg}");

            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
            assert_eq!(
                cfg.database_url().expect("ok").expose_secret(),
                "postgres://u:p@db/x"
            );
            Ok(())
        });
    }
}
