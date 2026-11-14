//! Asking the public DNS which name servers a domain has (T05.12), and what to
//! make of the answer.

use std::time::Duration;

use serde::Serialize;

use crate::{BoxFuture, DnsError};

/// Something that can ask the public DNS for the `NS` records of a name.
pub trait NsResolver: Send + Sync + 'static {
    /// The name servers the public DNS gives for `name` (absolute, lower-case,
    /// with the final dot), sorted. An empty list means the DNS answered that
    /// there are none (the domain does not exist, or is not delegated).
    ///
    /// # Errors
    /// [`DnsError::Unavailable`] when no answer could be had (timeout, no resolver).
    fn lookup_ns<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Vec<String>, DnsError>>;
}

/// The machine's own resolver (`/etc/resolv.conf`), with a short time limit.
///
/// It is set up on the first question, inside the running server, so making one
/// needs neither a runtime nor a readable configuration.
#[derive(Default)]
pub struct SystemResolver {
    inner: std::sync::OnceLock<Result<hickory_resolver::TokioResolver, String>>,
}

impl SystemResolver {
    /// A resolver that has not been set up yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn build() -> Result<hickory_resolver::TokioResolver, String> {
        let mut builder = hickory_resolver::Resolver::builder_tokio()
            .map_err(|e| format!("no system resolver: {e}"))?;
        let options = builder.options_mut();
        options.timeout = Duration::from_secs(4);
        options.attempts = 2;
        builder
            .build()
            .map_err(|e| format!("no system resolver: {e}"))
    }
}

impl NsResolver for SystemResolver {
    fn lookup_ns<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Vec<String>, DnsError>> {
        Box::pin(async move {
            let resolver = self
                .inner
                .get_or_init(Self::build)
                .as_ref()
                .map_err(|e| DnsError::Unavailable(e.clone()))?;
            let query = format!("{}.", name.trim_end_matches('.'));
            match resolver.ns_lookup(query).await {
                Ok(lookup) => {
                    let mut names: Vec<String> = lookup
                        .answers()
                        .iter()
                        .map(|record| record.data.to_string().to_ascii_lowercase())
                        .map(|n| if n.ends_with('.') { n } else { format!("{n}.") })
                        .collect();
                    names.sort();
                    names.dedup();
                    Ok(names)
                }
                Err(error) if error.is_no_records_found() || error.is_nx_domain() => Ok(Vec::new()),
                Err(error) => Err(DnsError::Unavailable(error.to_string())),
            }
        })
    }
}

/// How a domain's delegation compares with the panel's name servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delegation {
    /// The domain uses exactly the panel's name servers.
    Ok,
    /// Some of the panel's name servers are used, but not all, or others too.
    Partial,
    /// The domain uses other name servers than the panel's.
    Mismatch,
    /// The public DNS has no name servers for the domain.
    NotDelegated,
    /// The administrator has not set the panel's name servers.
    NotConfigured,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assessment {
    /// How the two compare.
    pub status: Delegation,
    /// The panel's name servers.
    pub expected: Vec<String>,
    /// What the public DNS says.
    pub found: Vec<String>,
    /// Panel name servers the domain does not use.
    pub missing: Vec<String>,
    /// Name servers the domain uses that are not the panel's.
    pub unexpected: Vec<String>,
}

/// Compares the panel's name servers with the public DNS's answer. Names are
/// compared without regard to case or the final dot.
#[must_use]
pub fn assess(expected: &[String], found: &[String]) -> Assessment {
    let norm = |n: &String| n.trim_end_matches('.').to_ascii_lowercase();
    let exp: Vec<String> = expected.iter().map(norm).collect();
    let fnd: Vec<String> = found.iter().map(norm).collect();
    let missing: Vec<String> = exp.iter().filter(|e| !fnd.contains(e)).cloned().collect();
    let unexpected: Vec<String> = fnd.iter().filter(|f| !exp.contains(f)).cloned().collect();
    let status = if exp.is_empty() {
        Delegation::NotConfigured
    } else if fnd.is_empty() {
        Delegation::NotDelegated
    } else if missing.is_empty() && unexpected.is_empty() {
        Delegation::Ok
    } else if missing.len() == exp.len() {
        Delegation::Mismatch
    } else {
        Delegation::Partial
    };
    Assessment {
        status,
        expected: exp,
        found: fnd,
        missing,
        unexpected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_answer_is_compared_with_the_panels_name_servers() {
        let ours = names(&["ns1.panel.net.", "ns2.panel.net."]);
        let case = |found: &[&str]| assess(&ours, &names(found));
        assert_eq!(
            case(&["NS2.panel.net", "ns1.panel.net."]).status,
            Delegation::Ok,
            "case and dots do not matter"
        );
        let partial = case(&["ns1.panel.net."]);
        assert_eq!(
            (partial.status, partial.missing.clone()),
            (Delegation::Partial, names(&["ns2.panel.net"]))
        );
        let extra = case(&["ns1.panel.net.", "ns2.panel.net.", "ns.other.org."]);
        assert_eq!(
            (extra.status, extra.unexpected.clone()),
            (Delegation::Partial, names(&["ns.other.org"]))
        );
        let other = case(&["ns.other.org."]);
        assert_eq!(other.status, Delegation::Mismatch);
        assert_eq!(other.missing.len(), 2);
        assert_eq!(case(&[]).status, Delegation::NotDelegated);
        assert_eq!(
            assess(&[], &names(&["ns.other.org."])).status,
            Delegation::NotConfigured
        );
        assert_eq!(assess(&[], &[]).status, Delegation::NotConfigured);
    }
}
