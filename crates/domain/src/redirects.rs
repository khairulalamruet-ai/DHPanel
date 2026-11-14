//! Redirects of a domain (T05.04): what may be written, and where it leads.
//!
//! A redirect applies to **one exact path** of a domain (`/old`, or `/` for the
//! front page), and sends the visitor to a path on the same site or to an
//! `http(s)` address. These rules mirror the CHECK constraints of
//! `migrations/20261111000020_domains.up.sql`, with a few more that are easier
//! to state in code (no credentials in an address, no protocol-relative paths).

use std::fmt;

/// Why a value was rejected. `code()` is stable for API clients.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RedirectError {
    /// Not an absolute path without query or fragment.
    #[error("the source must be an absolute path such as /old-page, without ? or #")]
    InvalidSource,
    /// Not a path on the same site, or an http(s) address.
    #[error("the target must be a path such as /new-page or an http(s) address")]
    InvalidTarget,
    /// The status code is not 301, 302, 307 or 308.
    #[error("the code must be 301, 302, 307 or 308")]
    InvalidCode,
}

impl RedirectError {
    /// Stable machine code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidSource => "invalid_redirect_source",
            Self::InvalidTarget => "invalid_redirect_target",
            Self::InvalidCode => "invalid_redirect_code",
        }
    }
}

/// Longest source path.
pub const MAX_SOURCE: usize = 512;
/// Longest target.
pub const MAX_TARGET: usize = 2048;

/// No whitespace or control characters, and none of `{`, `}`: the web server
/// replaces `{name}` in the values it sends (a placeholder such as
/// `{env.NAME}` would hand out the server's own settings).
fn plain(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '{' | '}'))
}

/// The path a redirect applies to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourcePath(String);

impl SourcePath {
    /// Checks a path: starts with `/`, no spaces, control characters, `?` or `#`.
    ///
    /// # Errors
    /// [`RedirectError::InvalidSource`].
    pub fn parse(input: &str) -> Result<Self, RedirectError> {
        if input.starts_with('/')
            && input.len() <= MAX_SOURCE
            && plain(input)
            // `*` would make the web server read the path as a pattern.
            && !input.contains(['?', '#', '*'])
        {
            Ok(Self(input.to_owned()))
        } else {
            Err(RedirectError::InvalidSource)
        }
    }

    /// The path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where a redirect sends the visitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A path on the same site (`/new`, `/new?x=1`).
    Path(String),
    /// An http(s) address.
    Url(String),
}

/// A place a redirect leads to: the host and the path (no query).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    /// Lower-case host without port.
    pub host: String,
    /// Path starting with `/`.
    pub path: String,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.host, self.path)
    }
}

impl Target {
    /// Checks a target.
    ///
    /// A path may carry a query; `//host` (which a browser reads as another
    /// site) is not a path. An address needs `http://` or `https://`, a host
    /// and no credentials.
    ///
    /// # Errors
    /// [`RedirectError::InvalidTarget`].
    pub fn parse(input: &str) -> Result<Self, RedirectError> {
        let bad = || RedirectError::InvalidTarget;
        if input.is_empty() || input.len() > MAX_TARGET || !plain(input) {
            return Err(bad());
        }
        if input.starts_with('/') {
            return if input.starts_with("//") || input.contains('\\') {
                Err(bad())
            } else {
                Ok(Self::Path(input.to_owned()))
            };
        }
        let rest = input
            .strip_prefix("https://")
            .or_else(|| input.strip_prefix("http://"))
            .ok_or_else(bad)?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.is_empty() || authority.contains('@') || input.contains('\\') {
            return Err(bad());
        }
        let host = split_port(authority).ok_or_else(bad)?;
        let host_ok = !host.is_empty()
            && !host.starts_with('.')
            && !host.ends_with("..")
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'));
        if !host_ok {
            return Err(bad());
        }
        Ok(Self::Url(input.to_owned()))
    }

    /// The text as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Path(s) | Self::Url(s) => s,
        }
    }

    /// Where this target leads when it is on the site `host`.
    #[must_use]
    pub fn place(&self, host: &str) -> Place {
        let path_of = |s: &str| {
            let p = s.split(['?', '#']).next().unwrap_or_default();
            if p.is_empty() {
                "/".to_owned()
            } else {
                p.to_owned()
            }
        };
        match self {
            Self::Path(p) => Place {
                host: host.to_owned(),
                path: path_of(p),
            },
            Self::Url(u) => {
                let rest = u
                    .strip_prefix("https://")
                    .or_else(|| u.strip_prefix("http://"))
                    .unwrap_or(u);
                let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
                let authority = &rest[..authority_end];
                let host = split_port(authority)
                    .unwrap_or(authority)
                    .trim_end_matches('.')
                    .to_ascii_lowercase();
                Place {
                    host,
                    path: path_of(&rest[authority_end..]),
                }
            }
        }
    }
}

/// The host of `host[:port]`, or `None` for a bad port.
fn split_port(authority: &str) -> Option<&str> {
    match authority.rsplit_once(':') {
        None => Some(authority),
        Some((host, port)) => {
            let ok = !port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u32>().is_ok_and(|p| (1..=65_535).contains(&p));
            ok.then_some(host)
        }
    }
}

/// The status of a redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectCode {
    /// Moved for good (may change a POST into a GET).
    MovedPermanently,
    /// Found: temporary (may change a POST into a GET).
    Found,
    /// Temporary, the method is kept.
    TemporaryRedirect,
    /// Permanent, the method is kept.
    PermanentRedirect,
}

impl RedirectCode {
    /// The number.
    #[must_use]
    pub fn number(self) -> i16 {
        match self {
            Self::MovedPermanently => 301,
            Self::Found => 302,
            Self::TemporaryRedirect => 307,
            Self::PermanentRedirect => 308,
        }
    }

    /// Reads the number.
    ///
    /// # Errors
    /// [`RedirectError::InvalidCode`].
    pub fn from_number(n: i64) -> Result<Self, RedirectError> {
        Ok(match n {
            301 => Self::MovedPermanently,
            302 => Self::Found,
            307 => Self::TemporaryRedirect,
            308 => Self::PermanentRedirect,
            _ => return Err(RedirectError::InvalidCode),
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_exact_absolute_paths() {
        for ok in ["/", "/old", "/a/b.html", "/ü"] {
            assert!(SourcePath::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "old",
            "/a b",
            "/a?x=1",
            "/a#f",
            "/a\nb",
            "/a*",
            "/*",
            "/{env.SECRET}",
            "/a}",
            &format!("/{}", "a".repeat(512)),
        ] {
            assert_eq!(
                SourcePath::parse(bad),
                Err(RedirectError::InvalidSource),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn targets_are_paths_or_plain_http_addresses() {
        for ok in [
            "/new",
            "/new?x=1&y=2",
            "/",
            "https://example.org",
            "http://example.org/a/b?c#d",
            "https://sub.example.org:8443/x",
            "https://127.0.0.1/x",
            "https://localhost/",
        ] {
            assert!(Target::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "new",
            "//evil.example/x",
            "/\\evil.example",
            "javascript:alert(1)",
            "data:text/html,x",
            "ftp://example.org/f",
            "https://",
            "https:///x",
            "https://user:pw@example.org/",
            "https://exa mple.org",
            "https://example.org:99999/",
            "https://example.org:/",
            "https://.example.org/",
            "https://exa_mple.org/",
            "https://[::1]/x",
            "/x?{env.SECRET}",
            "https://example.org/{http.request.host}",
            "/a{b}",
            &format!("/{}", "a".repeat(2048)),
        ] {
            assert_eq!(
                Target::parse(bad),
                Err(RedirectError::InvalidTarget),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_target_leads_to_a_host_and_a_path() {
        let place = |t: &str, host: &str| Target::parse(t).unwrap().place(host).to_string();
        assert_eq!(place("/new?x=1", "example.com"), "example.com/new");
        assert_eq!(place("/", "example.com"), "example.com/");
        assert_eq!(place("https://Example.ORG", "a.com"), "example.org/");
        assert_eq!(
            place("https://example.org:8443/a/b?c#d", "a.com"),
            "example.org/a/b"
        );
        assert_eq!(
            place("http://www.example.org./x", "a.com"),
            "www.example.org/x"
        );
        assert_eq!(place("https://example.org?q=1", "a.com"), "example.org/");
    }

    #[test]
    fn codes() {
        for n in [301, 302, 307, 308] {
            assert_eq!(RedirectCode::from_number(n).unwrap().number(), n as i16);
        }
        for n in [0, 300, 303, 304, 305, 306, 309, 200, -301] {
            assert_eq!(
                RedirectCode::from_number(n),
                Err(RedirectError::InvalidCode)
            );
        }
    }
}
