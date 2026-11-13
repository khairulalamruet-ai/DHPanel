//! Domain names of hosted sites (T05.01).
//!
//! What may be added as a site, in one place for the API, the importer and the
//! command line, mirroring the CHECK constraints of
//! `migrations/20261111000020_domains.up.sql`.
//!
//! A name is stored **lower-case in its ASCII form** (internationalised names as
//! punycode, `xn--...`), because that is what DNS, certificates and the web
//! server see; [`Fqdn::unicode`] gives the form a person reads. A name must sit
//! below a **public suffix** (`example.com`, `site.co.uk`): `com` and `co.uk`
//! themselves are not sites. Names under a suffix the list does not know
//! (`localhost`, `.internal`, `.test`) are refused too.

use std::fmt;

/// Why a value was rejected. `code()` is stable for API clients.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    /// Not a host name: empty, wrong characters, bad labels, too long.
    #[error("this is not a valid domain name")]
    Invalid,
    /// An IP address, not a name.
    #[error("an IP address is not a domain name")]
    IpAddress,
    /// A public suffix itself (`com`, `co.uk`) or a single label.
    #[error("this is a public suffix or a single label, not a name someone can register")]
    NotRegistrable,
    /// The top-level part is not on the public suffix list.
    #[error("the ending of this name is not a known top-level domain")]
    UnknownSuffix,
    /// An administrator forbade this name or its parent.
    #[error("this domain is not allowed on this server")]
    Blocked,
    /// Not one of the known kinds.
    #[error("unknown domain kind")]
    UnknownKind,
    /// Not one of the known statuses.
    #[error("unknown domain status")]
    UnknownStatus,
    /// Not a plain relative directory path.
    #[error(
        "the document root must be a plain relative path (letters, digits, `.`, `_`, `-`, at most 6 levels)"
    )]
    InvalidDocroot,
}

impl DomainError {
    /// Stable machine code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid => "invalid_domain",
            Self::IpAddress => "domain_is_ip_address",
            Self::NotRegistrable => "domain_not_registrable",
            Self::UnknownSuffix => "domain_unknown_suffix",
            Self::Blocked => "domain_blocked",
            Self::UnknownKind => "unknown_domain_kind",
            Self::UnknownStatus => "unknown_domain_status",
            Self::InvalidDocroot => "invalid_docroot",
        }
    }
}

/// A fully qualified domain name, normalised: lower-case ASCII (punycode).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fqdn(String);

impl Fqdn {
    /// Checks and normalises a name a person typed.
    ///
    /// Accepts upper case, Unicode names and one trailing dot. Refuses schemes,
    /// ports, paths, wildcards, spaces, underscores, IP addresses, public
    /// suffixes and unknown endings.
    ///
    /// # Errors
    /// [`DomainError`].
    pub fn parse(input: &str) -> Result<Self, DomainError> {
        let trimmed = input.trim();
        let name = trimmed.strip_suffix('.').unwrap_or(trimmed);
        if name.is_empty() || name.len() > 253 * 4 {
            return Err(DomainError::Invalid);
        }
        // `idna` is lenient about some separators; a site name has none of these.
        if name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "/\\:@?#%*_".contains(c))
        {
            return Err(DomainError::Invalid);
        }
        let ascii = idna::domain_to_ascii_strict(name).map_err(|_| DomainError::Invalid)?;
        Self::from_ascii(ascii)
    }

    fn from_ascii(ascii: String) -> Result<Self, DomainError> {
        if ascii.is_empty() || ascii.len() > 253 {
            return Err(DomainError::Invalid);
        }
        let labels: Vec<&str> = ascii.split('.').collect();
        let ldh = |l: &str| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        if !labels.iter().all(|l| ldh(l)) {
            return Err(DomainError::Invalid);
        }
        // `1.2.3.4`, `0x7f.1`: the last label of a real name is never all digits.
        if labels
            .last()
            .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(DomainError::IpAddress);
        }
        let bytes = ascii.as_bytes();
        match psl::domain(bytes) {
            // Under a suffix with a registrable part.
            Some(domain) if domain.suffix().is_known() => {
                let _ = domain;
            }
            Some(_) => return Err(DomainError::UnknownSuffix),
            None => {
                return Err(match psl::suffix(bytes) {
                    Some(s) if s.is_known() => DomainError::NotRegistrable,
                    _ => DomainError::UnknownSuffix,
                });
            }
        }
        Ok(Self(ascii))
    }

    /// The stored form: lower-case ASCII.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The form a person reads (Unicode where the name is internationalised).
    #[must_use]
    pub fn unicode(&self) -> String {
        let (text, result) = idna::domain_to_unicode(&self.0);
        if result.is_ok() { text } else { self.0.clone() }
    }

    /// The part somebody registered: `example.com` for `www.example.com`.
    #[must_use]
    pub fn registrable(&self) -> &str {
        psl::domain_str(&self.0).unwrap_or(&self.0)
    }

    /// Whether this name is the registrable name itself (not a subdomain of it).
    #[must_use]
    pub fn is_registrable(&self) -> bool {
        self.registrable() == self.0
    }

    /// Whether this name lies strictly below `parent` (`www.example.com` below `example.com`).
    #[must_use]
    pub fn is_below(&self, parent: &Fqdn) -> bool {
        self.0.len() > parent.0.len()
            && self.0.ends_with(parent.0.as_str())
            && self.0.as_bytes()[self.0.len() - parent.0.len() - 1] == b'.'
    }
}

impl fmt::Display for Fqdn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a domain relates to its tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DomainKind {
    /// The tenant's main domain; exactly one per tenant.
    Primary,
    /// Another registrable domain with its own site and docroot.
    Addon,
    /// A subdomain of one of the tenant's domains, with its own docroot.
    Sub,
    /// Another name for the site of an existing domain (same content).
    Alias,
}

impl DomainKind {
    /// Database / wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Addon => "addon",
            Self::Sub => "sub",
            Self::Alias => "alias",
        }
    }

    /// Reads the representation.
    ///
    /// # Errors
    /// [`DomainError::UnknownKind`].
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Ok(match s {
            "primary" => Self::Primary,
            "addon" => Self::Addon,
            "sub" => Self::Sub,
            "alias" => Self::Alias,
            _ => return Err(DomainError::UnknownKind),
        })
    }

    /// Whether the kind points at another domain of the tenant (`parent`): a
    /// subdomain below it, an alias of it.
    #[must_use]
    pub fn needs_parent(self) -> bool {
        matches!(self, Self::Sub | Self::Alias)
    }
}

/// Whether a domain is being served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DomainStatus {
    /// Served.
    Active,
    /// Not served (the tenant is suspended, or the domain was).
    Suspended,
}

impl DomainStatus {
    /// Database / wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }

    /// Reads the representation.
    ///
    /// # Errors
    /// [`DomainError::UnknownStatus`].
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            _ => Err(DomainError::UnknownStatus),
        }
    }
}

/// Where a domain's files are, relative to the tenant's web directory. Empty
/// means the web directory itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Docroot(String);

impl Docroot {
    /// Checks a path: plain names (`A-Z a-z 0-9 . _ -`), no `.` or `..`, no
    /// leading or trailing `/`, at most 6 levels of 64 characters.
    ///
    /// # Errors
    /// [`DomainError::InvalidDocroot`].
    pub fn parse(input: &str) -> Result<Self, DomainError> {
        if input.is_empty() {
            return Ok(Self(String::new()));
        }
        let parts: Vec<&str> = input.split('/').collect();
        let plain = |p: &&str| {
            !p.is_empty()
                && p.len() <= 64
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        };
        if parts.len() > 6 || !parts.iter().all(plain) {
            return Err(DomainError::InvalidDocroot);
        }
        Ok(Self(input.to_owned()))
    }

    /// The path, empty for the web directory itself.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Names an administrator forbids: each entry blocks that name and every name
/// below it (`example.org` blocks `www.example.org` too). Entries may be
/// suffixes, so a whole ending such as `gov` can be closed.
#[derive(Debug, Clone, Default)]
pub struct Blocklist(Vec<String>);

impl Blocklist {
    /// Builds a list from entries as typed (any case, Unicode allowed). Blank
    /// entries are ignored.
    ///
    /// # Errors
    /// [`DomainError::Invalid`] for an entry that is not a name at all.
    pub fn new<'a>(entries: impl IntoIterator<Item = &'a str>) -> Result<Self, DomainError> {
        let mut list = Vec::new();
        for entry in entries {
            let entry = entry.trim().trim_end_matches('.');
            if entry.is_empty() {
                continue;
            }
            let ascii = idna::domain_to_ascii_strict(entry).map_err(|_| DomainError::Invalid)?;
            list.push(ascii);
        }
        Ok(Self(list))
    }

    /// Refuses a name that is, or lies below, an entry.
    ///
    /// # Errors
    /// [`DomainError::Blocked`].
    pub fn check(&self, name: &Fqdn) -> Result<(), DomainError> {
        let blocked = self.0.iter().any(|entry| {
            name.as_str() == entry
                || (name.as_str().len() > entry.len()
                    && name.as_str().ends_with(entry.as_str())
                    && name.as_str().as_bytes()[name.as_str().len() - entry.len() - 1] == b'.')
        });
        if blocked {
            Err(DomainError::Blocked)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ok(s: &str) -> String {
        Fqdn::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}")).0
    }

    #[test]
    fn real_names_are_accepted_and_normalised() {
        assert_eq!(ok("example.com"), "example.com");
        assert_eq!(ok("  WWW.Example.COM. "), "www.example.com");
        assert_eq!(ok("example.co.uk"), "example.co.uk");
        assert_eq!(ok("a.b.c.example.org"), "a.b.c.example.org");
        assert_eq!(ok("my-site.example.com"), "my-site.example.com");
        assert_eq!(ok("1site.example.com"), "1site.example.com");
        // Under a private suffix: the suffix itself is not for sale, a name below it is.
        assert_eq!(ok("someone.github.io"), "someone.github.io");
    }

    #[test]
    fn international_names_are_stored_as_punycode() {
        assert_eq!(ok("bücher.de"), "xn--bcher-kva.de");
        assert_eq!(ok("XN--BCHER-KVA.de"), "xn--bcher-kva.de");
        assert_eq!(ok("বাংলা.com"), "xn--54b7fta0cc.com");
        let name = Fqdn::parse("bücher.de").unwrap();
        assert_eq!(name.unicode(), "bücher.de");
        assert_eq!(
            Fqdn::parse("xn--54b7fta0cc.com").unwrap().unicode(),
            "বাংলা.com"
        );
        // Not valid punycode.
        assert_eq!(Fqdn::parse("xn--.example.com"), Err(DomainError::Invalid));
        assert!(Fqdn::parse("xn--a-.example.com").is_err());
    }

    #[test]
    fn suffixes_single_labels_and_unknown_endings_are_refused() {
        for suffix in ["com", "org", "co.uk", "github.io", "com.bd"] {
            assert_eq!(
                Fqdn::parse(suffix),
                Err(DomainError::NotRegistrable),
                "{suffix}"
            );
        }
        for unknown in [
            "localhost",
            "foo.localhost",
            "site.internal",
            "a.test",
            "x.invalid",
            "host.local",
            "nope.notatld",
        ] {
            assert_eq!(
                Fqdn::parse(unknown),
                Err(DomainError::UnknownSuffix),
                "{unknown}"
            );
        }
    }

    #[test]
    fn things_that_are_not_names_are_refused() {
        for bad in [
            "",
            " ",
            ".",
            "..",
            "a..com",
            ".example.com",
            "-a.example.com",
            "a-.example.com",
            "exa mple.com",
            "http://example.com",
            "example.com/path",
            "example.com:8080",
            "user@example.com",
            "*.example.com",
            "a_b.example.com",
            "example.com?x",
            "example.com#y",
            "exam\nple.com",
            "exam\u{0}ple.com",
        ] {
            assert!(Fqdn::parse(bad).is_err(), "{bad:?}");
        }
        for ip in [
            "1.2.3.4",
            "127.0.0.1",
            "0x7f.0.0.1",
            "999.999.999.999",
            "1.2.3",
        ] {
            assert!(Fqdn::parse(ip).is_err(), "{ip}");
        }
        assert_eq!(Fqdn::parse("1.2.3.4"), Err(DomainError::IpAddress));
        // Length limits: 63 per label, 253 in all.
        assert!(Fqdn::parse(&format!("{}.com", "a".repeat(63))).is_ok());
        assert!(Fqdn::parse(&format!("{}.com", "a".repeat(64))).is_err());
        let long = format!("{}.com", vec!["a".repeat(60); 4].join("."));
        assert!(long.len() > 240 && Fqdn::parse(&long).is_ok());
        let too_long = format!("{}.com", vec!["a".repeat(60); 5].join("."));
        assert!(too_long.len() > 253 && Fqdn::parse(&too_long).is_err());
    }

    #[test]
    fn registrable_part_and_ancestry() {
        let www = Fqdn::parse("www.shop.example.co.uk").unwrap();
        let apex = Fqdn::parse("example.co.uk").unwrap();
        assert_eq!(www.registrable(), "example.co.uk");
        assert!(apex.is_registrable() && !www.is_registrable());
        assert!(www.is_below(&apex));
        assert!(!apex.is_below(&apex), "strictly below");
        assert!(
            !Fqdn::parse("badexample.co.uk").unwrap().is_below(&apex),
            "a longer name is not below"
        );
        assert!(!apex.is_below(&www));
    }

    #[test]
    fn kinds_statuses_and_docroots() {
        for kind in [
            DomainKind::Primary,
            DomainKind::Addon,
            DomainKind::Sub,
            DomainKind::Alias,
        ] {
            assert_eq!(DomainKind::parse(kind.as_str()), Ok(kind));
        }
        assert_eq!(DomainKind::parse("main"), Err(DomainError::UnknownKind));
        assert!(DomainKind::Sub.needs_parent() && DomainKind::Alias.needs_parent());
        assert!(!DomainKind::Primary.needs_parent() && !DomainKind::Addon.needs_parent());
        assert_eq!(
            DomainStatus::parse("suspended"),
            Ok(DomainStatus::Suspended)
        );
        assert_eq!(DomainStatus::parse("gone"), Err(DomainError::UnknownStatus));

        assert_eq!(Docroot::parse("").unwrap().as_str(), "");
        assert_eq!(
            Docroot::parse("public_html/site-2").unwrap().as_str(),
            "public_html/site-2"
        );
        for bad in [
            "/abs",
            "a/",
            "a//b",
            "../x",
            "a/../b",
            ".",
            "a b",
            "a\\b",
            "a/b/c/d/e/f/g",
        ] {
            assert_eq!(
                Docroot::parse(bad),
                Err(DomainError::InvalidDocroot),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_blocklist_closes_a_name_and_everything_below_it() {
        let list = Blocklist::new(["Bad.Example.org", " ", "gov.", "bücher.de"]).unwrap();
        let blocked = |s: &str| list.check(&Fqdn::parse(s).unwrap());
        assert_eq!(blocked("bad.example.org"), Err(DomainError::Blocked));
        assert_eq!(blocked("www.bad.example.org"), Err(DomainError::Blocked));
        assert_eq!(
            blocked("agency.gov"),
            Err(DomainError::Blocked),
            "a whole ending can be closed"
        );
        assert_eq!(
            blocked("xn--bcher-kva.de"),
            Err(DomainError::Blocked),
            "compared in ASCII form"
        );
        assert!(blocked("good.example.org").is_ok());
        assert!(
            blocked("notbad.example.org").is_ok(),
            "a name that merely ends with the same letters"
        );
        assert!(
            blocked("example.org").is_ok(),
            "the parent is not blocked by a child entry"
        );
        assert!(Blocklist::new(["a b"]).is_err());
        assert!(
            Blocklist::default()
                .check(&Fqdn::parse("example.com").unwrap())
                .is_ok()
        );
    }
}
