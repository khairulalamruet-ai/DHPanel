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
    net::{IpAddr, SocketAddr},
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
    /// Reverse proxies (exact addresses, e.g. the local Caddy) whose
    /// `X-Forwarded-For` header is trusted for the client IP. Empty: the TCP
    /// peer is always the client.
    pub trusted_proxies: Vec<IpAddr>,
    /// Logging and tracing.
    pub log: LogConfig,
    /// PostgreSQL connection.
    pub database: DatabaseConfig,
    /// Redis connection.
    pub redis: RedisConfig,
    /// Authentication settings.
    pub auth: AuthConfig,
    /// Keys for secrets stored encrypted in the database.
    pub secrets: SecretsConfig,
    /// Node agents: how they reach the control plane.
    pub agent: AgentConfig,
}

/// Node agent settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    /// Address agents use to reach the control plane's agent port, e.g.
    /// `https://panel.example.com:8443`. Put into the enrol command shown when a
    /// node is added; without it the command is left out and only the token is
    /// shown. Plain `http` is accepted for localhost only.
    pub public_url: Option<String>,
    /// Minutes an enrollment token stays valid (1..=60).
    pub enrollment_token_minutes: u32,
}

/// Whether `url` is safe to paste into a shell command and points where agents
/// may be told to connect: `https://host[:port][/path]`, or `http://` for
/// loopback only (so a token never crosses a network in the clear). Only
/// letters, digits and `.-_~/:` are allowed, which rules out quotes, spaces,
/// `$`, `;`, `?` and user-info.
fn valid_agent_url(url: &str) -> bool {
    let allowed =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~' | '/' | ':');
    if url.len() > 200 || url.ends_with('/') || !url.chars().all(allowed) {
        return false;
    }
    if let Some(host) = url.strip_prefix("https://") {
        return !host.is_empty() && !host.starts_with(['/', ':']);
    }
    ["http://localhost", "http://127.0.0.1"]
        .iter()
        .any(|prefix| {
            url.strip_prefix(prefix)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with([':', '/']))
        })
}

/// Keys for secrets at rest.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecretsConfig {
    /// Master key: 32 random bytes, base64 (`openssl rand -base64 32`).
    /// Prefer `master_key_file`. Losing it makes stored TOTP secrets unreadable.
    #[serde(default, skip_serializing)]
    pub master_key: Option<SecretString>,
    /// File holding the master key (mode 0600).
    pub master_key_file: Option<PathBuf>,
}

/// Authentication settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Argon2id memory cost in KiB (minimum 19456 = 19 MiB).
    pub argon2_memory_kib: u32,
    /// Argon2id iterations (minimum 2).
    pub argon2_iterations: u32,
    /// Argon2id lanes (1..=8).
    pub argon2_parallelism: u32,
    /// Minutes of inactivity after which a session ends (5..=1440).
    pub session_idle_minutes: u32,
    /// Hours after login after which a session ends regardless of activity (1..=720).
    pub session_absolute_hours: u32,
    /// Minutes after authenticating during which sensitive actions need no
    /// re-authentication ("sudo mode", 1..=60).
    pub sudo_minutes: u32,
    /// Failed logins per account (or unknown login name) inside
    /// `lockout_window_minutes` that lock it (1..=100).
    pub lockout_threshold: u32,
    /// Sliding window for counting failed logins, minutes (1..=1440).
    pub lockout_window_minutes: u32,
    /// How long a lockout lasts, minutes (1..=1440).
    pub lockout_minutes: u32,
    /// Failed logins from one IP (IPv6: one /64) inside the window that lock
    /// that address (`lockout_threshold`..=100000). Higher than the per-account
    /// limit because many users can share one address (NAT).
    pub ip_lockout_threshold: u32,
    /// Name shown in authenticator apps for TOTP entries (1..=64 characters, no `:`).
    pub totp_issuer: String,
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
            trusted_proxies: Vec::new(),
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
            // OWASP argon2id baseline; see dhpanel_auth_core::password::Argon2Params.
            auth: AuthConfig {
                argon2_memory_kib: 19 * 1024,
                argon2_iterations: 2,
                argon2_parallelism: 1,
                session_idle_minutes: 30,
                session_absolute_hours: 12,
                sudo_minutes: 10,
                lockout_threshold: 5,
                lockout_window_minutes: 15,
                lockout_minutes: 15,
                ip_lockout_threshold: 50,
                totp_issuer: "DHPanel".to_owned(),
            },
            secrets: SecretsConfig {
                master_key: None,
                master_key_file: None,
            },
            agent: AgentConfig {
                public_url: None,
                enrollment_token_minutes: 15,
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
        if self.auth.argon2_memory_kib < 19 * 1024 {
            return Err(ConfigError::Invalid {
                key: "auth.argon2_memory_kib",
                reason: format!(
                    "must be at least 19456 (OWASP minimum), got {}",
                    self.auth.argon2_memory_kib
                ),
            });
        }
        if self.auth.argon2_iterations < 2 {
            return Err(ConfigError::Invalid {
                key: "auth.argon2_iterations",
                reason: format!("must be at least 2, got {}", self.auth.argon2_iterations),
            });
        }
        if !(1..=8).contains(&self.auth.argon2_parallelism) {
            return Err(ConfigError::Invalid {
                key: "auth.argon2_parallelism",
                reason: format!("must be 1..=8, got {}", self.auth.argon2_parallelism),
            });
        }
        if !(5..=1440).contains(&self.auth.session_idle_minutes) {
            return Err(ConfigError::Invalid {
                key: "auth.session_idle_minutes",
                reason: format!("must be 5..=1440, got {}", self.auth.session_idle_minutes),
            });
        }
        if !(1..=720).contains(&self.auth.session_absolute_hours) {
            return Err(ConfigError::Invalid {
                key: "auth.session_absolute_hours",
                reason: format!("must be 1..=720, got {}", self.auth.session_absolute_hours),
            });
        }
        if u64::from(self.auth.session_idle_minutes)
            > u64::from(self.auth.session_absolute_hours) * 60
        {
            return Err(ConfigError::Invalid {
                key: "auth.session_idle_minutes",
                reason: "must not exceed session_absolute_hours".to_owned(),
            });
        }
        let range_checks: [(&'static str, u32, std::ops::RangeInclusive<u32>); 6] = [
            ("auth.sudo_minutes", self.auth.sudo_minutes, 1..=60),
            (
                "agent.enrollment_token_minutes",
                self.agent.enrollment_token_minutes,
                1..=60,
            ),
            (
                "auth.lockout_threshold",
                self.auth.lockout_threshold,
                1..=100,
            ),
            (
                "auth.lockout_window_minutes",
                self.auth.lockout_window_minutes,
                1..=1440,
            ),
            ("auth.lockout_minutes", self.auth.lockout_minutes, 1..=1440),
            (
                "auth.ip_lockout_threshold",
                self.auth.ip_lockout_threshold,
                self.auth.lockout_threshold..=100_000,
            ),
        ];
        for (key, value, range) in range_checks {
            if !range.contains(&value) {
                return Err(ConfigError::Invalid {
                    key,
                    reason: format!("must be {}..={}, got {value}", range.start(), range.end()),
                });
            }
        }
        if let Some(url) = &self.agent.public_url
            && !valid_agent_url(url)
        {
            return Err(ConfigError::Invalid {
                key: "agent.public_url",
                reason: "must be https://host[:port] (http only for localhost) without a trailing slash, query or special characters".to_owned(),
            });
        }
        let issuer = self.auth.totp_issuer.trim();
        if issuer.is_empty() || issuer.chars().count() > 64 || issuer.contains(':') {
            return Err(ConfigError::Invalid {
                key: "auth.totp_issuer",
                reason: "must be 1..=64 characters and must not contain ':'".to_owned(),
            });
        }
        if self.secrets.master_key.is_some() && self.secrets.master_key_file.is_some() {
            return Err(ConfigError::Invalid {
                key: "secrets.master_key",
                reason: "set either `master_key` or `master_key_file`, not both".to_owned(),
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

    /// Resolves the base64 master key from `secrets.master_key` or
    /// `secrets.master_key_file`. Decoding and length checks are done by
    /// `dhpanel_auth_core::secretbox::MasterKey::from_base64`.
    ///
    /// # Errors
    /// Missing secret, unreadable file or unsafe permissions.
    pub fn master_key(&self) -> Result<SecretString, ConfigError> {
        resolve_secret(
            "secrets.master_key",
            self.secrets.master_key.as_ref(),
            self.secrets.master_key_file.as_deref(),
        )
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
    fn weak_argon2_settings_are_rejected() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("DHPANEL_AUTH__ARGON2_MEMORY_KIB", "4096");
            let msg = load(jail).expect_err("too little memory").to_string();
            assert!(msg.contains("auth.argon2_memory_kib"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn session_timeouts_are_validated() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            jail.set_env("DHPANEL_AUTH__SESSION_IDLE_MINUTES", "1");
            let msg = load(jail).expect_err("too short").to_string();
            assert!(msg.contains("auth.session_idle_minutes"), "{msg}");
            jail.set_env("DHPANEL_AUTH__SESSION_IDLE_MINUTES", "120");
            jail.set_env("DHPANEL_AUTH__SESSION_ABSOLUTE_HOURS", "1");
            let msg = load(jail).expect_err("idle > absolute").to_string();
            assert!(msg.contains("must not exceed"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn lockout_settings_are_validated() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            let cfg = load(jail).expect("defaults");
            assert_eq!(
                (
                    cfg.auth.lockout_threshold,
                    cfg.auth.lockout_window_minutes,
                    cfg.auth.lockout_minutes,
                    cfg.auth.ip_lockout_threshold
                ),
                (5, 15, 15, 50)
            );
            jail.set_env("DHPANEL_AUTH__LOCKOUT_THRESHOLD", "0");
            let msg = load(jail).expect_err("zero").to_string();
            assert!(msg.contains("auth.lockout_threshold"), "{msg}");
            jail.set_env("DHPANEL_AUTH__LOCKOUT_THRESHOLD", "10");
            jail.set_env("DHPANEL_AUTH__IP_LOCKOUT_THRESHOLD", "5");
            let msg = load(jail).expect_err("ip < account").to_string();
            assert!(msg.contains("auth.ip_lockout_threshold"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn master_key_is_resolved_validated_and_never_printed() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            let cfg = load(jail).expect("defaults");
            let msg = cfg.master_key().expect_err("missing").to_string();
            assert!(msg.contains("secrets.master_key"), "{msg}");

            jail.set_env(
                "DHPANEL_SECRETS__MASTER_KEY",
                "c2VjcmV0LWtleS1tYXRlcmlhbA==",
            );
            let cfg = load(jail).expect("inline");
            assert!(cfg.master_key().is_ok());
            assert!(!format!("{cfg:?}").contains("c2VjcmV0"), "{cfg:?}");

            jail.set_env("DHPANEL_SECRETS__MASTER_KEY_FILE", "/nonexistent");
            let msg = load(jail).expect_err("both").to_string();
            assert!(msg.contains("secrets.master_key"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn agent_settings_default_and_are_validated() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            let cfg = load(jail).expect("defaults");
            assert_eq!(cfg.agent.public_url, None);
            assert_eq!(cfg.agent.enrollment_token_minutes, 15);
            for ok in [
                "https://panel.example.com",
                "https://panel.example.com:8443",
                "https://10.0.0.5:8443/agent",
                "http://localhost:8443",
                "http://127.0.0.1",
            ] {
                jail.set_env("DHPANEL_AGENT__PUBLIC_URL", ok);
                assert!(load(jail).is_ok(), "{ok}");
            }
            for bad in [
                "http://panel.example.com",
                "http://localhost.evil.com",
                "ftp://x.example.com",
                "https://",
                "https://:8443",
                "https://a.example.com/",
                "https://a.example.com/x y",
                "https://a.example.com;reboot",
                "https://a.example.com/$(id)",
                "https://a.example.com?x=1",
                "https://user@a.example.com",
                "panel.example.com",
                &format!("https://{}", "a".repeat(200)),
            ] {
                jail.set_env("DHPANEL_AGENT__PUBLIC_URL", bad);
                let msg = load(jail).expect_err(bad).to_string();
                assert!(msg.contains("agent.public_url"), "{bad}: {msg}");
            }
            jail.set_env("DHPANEL_AGENT__PUBLIC_URL", "https://panel.example.com");
            for bad in ["0", "61"] {
                jail.set_env("DHPANEL_AGENT__ENROLLMENT_TOKEN_MINUTES", bad);
                let msg = load(jail).expect_err(bad).to_string();
                assert!(msg.contains("agent.enrollment_token_minutes"), "{msg}");
            }
            Ok(())
        });
    }

    #[test]
    fn sudo_minutes_default_and_range() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            assert_eq!(load(jail).expect("defaults").auth.sudo_minutes, 10);
            jail.set_env("DHPANEL_AUTH__SUDO_MINUTES", "0");
            let msg = load(jail).expect_err("zero").to_string();
            assert!(msg.contains("auth.sudo_minutes"), "{msg}");
            Ok(())
        });
    }

    #[test]
    fn totp_issuer_is_validated() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            assert_eq!(load(jail).expect("defaults").auth.totp_issuer, "DHPanel");
            for bad in ["", "My:Panel"] {
                jail.set_env("DHPANEL_AUTH__TOTP_ISSUER", bad);
                let msg = load(jail).expect_err("bad issuer").to_string();
                assert!(msg.contains("auth.totp_issuer"), "{msg}");
            }
            Ok(())
        });
    }

    #[test]
    fn shipped_example_config_is_valid() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            let example = include_str!("../../../deploy/config/control.example.toml");
            jail.create_file("control.toml", example)?;
            let cfg = load(jail).expect("example config must load");
            assert_eq!(cfg.auth.lockout_threshold, 5);
            Ok(())
        });
    }

    #[test]
    fn trusted_proxies_parse_from_file() {
        Jail::expect_with(|jail| {
            jail.clear_env();
            assert!(load(jail).expect("defaults").trusted_proxies.is_empty());
            jail.create_file("control.toml", r#"trusted_proxies = ["127.0.0.1", "::1"]"#)?;
            let cfg = load(jail).expect("valid");
            assert_eq!(cfg.trusted_proxies.len(), 2);
            jail.create_file("control.toml", r#"trusted_proxies = ["10.0.0.0/8"]"#)?;
            let msg = load(jail).expect_err("CIDR is not an address").to_string();
            assert!(msg.contains("trusted_proxies"), "{msg}");
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
