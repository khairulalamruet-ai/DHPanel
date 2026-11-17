//! The authority's configuration (T17.01): its own database and its own file, separate from
//! the control plane's. Defaults, then `/etc/dhpanel/license-authority.toml` (or the file named
//! by `DHPANEL_LA_CONFIG`), then `DHPANEL_LA_*` environment variables (`__` between nested
//! keys).

use std::{net::SocketAddr, path::PathBuf};

use figment::{
    Figment,
    providers::{Env, Format as _, Serialized, Toml},
};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// The default place of the configuration file.
pub const DEFAULT_PATH: &str = "/etc/dhpanel/license-authority.toml";

/// Everything the authority reads at start.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaConfig {
    /// Where it listens.
    pub bind: SocketAddr,
    /// Its PostgreSQL (not the control plane's).
    #[serde(serialize_with = "serialize_secret")]
    pub database_url: SecretString,
    /// The file holding the licence signing key (mode 0600).
    pub signing_key_file: PathBuf,
    /// The key id written into what it signs.
    pub signing_kid: String,
    /// The name in the `iss` claim.
    pub issuer: String,
    /// The name in the `aud` claim.
    pub audience: String,
    /// How long an entitlement stays valid after it is issued, in hours (it should be longer
    /// than the offline grace the installations allow themselves).
    pub entitlement_hours: u32,
    /// How long an admin sign-in lasts, in hours.
    pub session_hours: u32,
    /// The reverse proxies in front of the authority (their addresses). The address of a
    /// request is read from `X-Forwarded-For` only when the connection is from one of these.
    pub trusted_proxies: Vec<std::net::IpAddr>,
    /// Whether an installation's first activation must also prove, from outside, that its panel
    /// answers at the domain it claims. Needs the panel to be reachable on https at the
    /// license's bound address.
    pub require_domain_proof: bool,
    /// Log format: `json` or `pretty`.
    pub log_format: String,
    /// Log filter directives.
    pub log_filter: String,
}

/// Only the defaults are ever serialised (to seed the layered configuration); nothing prints it.
fn serialize_secret<S: serde::Serializer>(v: &SecretString, s: S) -> Result<S::Ok, S::Error> {
    use secrecy::ExposeSecret as _;
    s.serialize_str(v.expose_secret())
}

impl Default for LaConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 9440)),
            database_url: SecretString::from("postgres://dhpanel_la@127.0.0.1/dhpanel_la"),
            signing_key_file: PathBuf::from("/etc/dhpanel/la-signing.key"),
            signing_kid: "license-1".to_owned(),
            issuer: "dhpanel-la".to_owned(),
            audience: "dhpanel".to_owned(),
            entitlement_hours: 96,
            session_hours: 8,
            trusted_proxies: vec![
                std::net::IpAddr::from([127, 0, 0, 1]),
                std::net::IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
            ],
            require_domain_proof: false,
            log_format: "json".to_owned(),
            log_filter: "info".to_owned(),
        }
    }
}

/// Why the configuration could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Unreadable or wrong values.
    #[error("{0}")]
    Read(String),
    /// A value out of range.
    #[error("{0}")]
    Invalid(String),
}

impl LaConfig {
    /// Loads and checks the configuration.
    ///
    /// # Errors
    /// [`ConfigError`].
    pub fn load() -> Result<Self, ConfigError> {
        let path = std::env::var_os("DHPANEL_LA_CONFIG")
            .map_or_else(|| PathBuf::from(DEFAULT_PATH), PathBuf::from);
        Self::load_from(&path)
    }

    /// [`Self::load`] with the file given.
    ///
    /// # Errors
    /// [`ConfigError`].
    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        let mut figment = Figment::from(Serialized::defaults(Self::default()));
        if path.exists() {
            figment = figment.merge(Toml::file(path));
        }
        let config: Self = figment
            .merge(Env::prefixed("DHPANEL_LA_").ignore(&["CONFIG"]).split("__"))
            .extract()
            .map_err(|e| ConfigError::Read(e.to_string()))?;
        config.check()?;
        Ok(config)
    }

    fn check(&self) -> Result<(), ConfigError> {
        use secrecy::ExposeSecret as _;
        let bad = |m: &str| Err(ConfigError::Invalid(m.to_owned()));
        if !(1..=24 * 365).contains(&self.entitlement_hours) {
            return bad("entitlement_hours: 1 to 8760");
        }
        if !(1..=24 * 30).contains(&self.session_hours) {
            return bad("session_hours: 1 to 720");
        }
        if !["json", "pretty"].contains(&self.log_format.as_str()) {
            return bad("log_format: json or pretty");
        }
        if !self.database_url.expose_secret().starts_with("postgres") {
            return bad("database_url: a postgres:// address");
        }
        if self.issuer.is_empty() || self.audience.is_empty() {
            return bad("issuer and audience may not be empty");
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::result_large_err)]
    fn the_defaults_are_valid_and_the_environment_overrides_them() {
        figment::Jail::expect_with(|jail| {
            let c = LaConfig::load_from(std::path::Path::new("/nonexistent")).unwrap();
            assert_eq!(c.entitlement_hours, 96);
            jail.set_env("DHPANEL_LA_ENTITLEMENT_HOURS", "48");
            jail.set_env("DHPANEL_LA_DATABASE_URL", "postgres://x@db/la");
            let c = LaConfig::load_from(std::path::Path::new("/nonexistent")).unwrap();
            assert_eq!(c.entitlement_hours, 48);
            jail.set_env("DHPANEL_LA_ENTITLEMENT_HOURS", "0");
            assert!(LaConfig::load_from(std::path::Path::new("/nonexistent")).is_err());
            Ok(())
        });
    }
}
