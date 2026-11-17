//! The common model every source is read into (T15.01).
//!
//! A source (cPanel, CyberPanel, ...) turns its backup into an [`ImportModel`]; the
//! executor (T15.04) only ever sees this model. The model holds *what the account has*,
//! never file contents: files, dumps and mailboxes are named by an [`ArchivePath`] that
//! the executor streams out of the archive.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ImportError;

/// The version of the model's JSON form. Bump it when a field changes meaning.
pub const MODEL_VERSION: u32 = 1;

/// A path inside the backup archive: relative, no `..`, no empty parts, `/` separated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ArchivePath(String);

impl ArchivePath {
    /// Checks and wraps a path.
    ///
    /// # Errors
    /// [`ImportError::UnsafePath`] for an absolute path, `..`, an empty part or a NUL byte.
    pub fn new(path: &str) -> Result<Self, ImportError> {
        let bad = path.is_empty()
            || path.starts_with('/')
            || path.contains('\0')
            || path.contains('\\')
            || path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..");
        if bad {
            Err(ImportError::UnsafePath(path.chars().take(200).collect()))
        } else {
            Ok(Self(path.to_owned()))
        }
    }

    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArchivePath {
    type Error = ImportError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<ArchivePath> for String {
    fn from(p: ArchivePath) -> Self {
        p.0
    }
}

/// Which panel made the backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// cPanel & WHM (`cpmove`, full backup).
    Cpanel,
    /// CyberPanel (website backup).
    Cyberpanel,
}

/// Where the backup came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// The panel.
    pub kind: SourceKind,
    /// The version the archive says it was made by, when it says.
    pub version: Option<String>,
}

/// The account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Login / system name on the old panel.
    pub username: String,
    /// Contact email address, when known.
    pub email: Option<String>,
    /// The plan's name on the old panel (the executor maps it to one of ours).
    pub plan: Option<String>,
    /// Interface language, when known.
    pub locale: Option<String>,
    /// The main domain.
    pub main_domain: Option<String>,
    /// When the account was made (Unix seconds), when known.
    pub created_at: Option<i64>,
}

/// What a domain is to the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainKind {
    /// The main domain.
    Main,
    /// An added domain with its own document root.
    Addon,
    /// A name under one of the account's domains.
    Sub,
    /// A name that shows another domain's site.
    Alias,
}

/// A domain of the account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Domain {
    /// Lower-case name.
    pub name: String,
    /// What it is.
    pub kind: DomainKind,
    /// Document root relative to the home directory (`public_html`), when known.
    pub docroot: Option<String>,
    /// PHP version as `8.1`, when known.
    pub php_version: Option<String>,
}

/// The kind of database server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// MySQL or MariaDB.
    Mysql,
    /// PostgreSQL.
    Postgres,
}

/// A database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Database {
    /// Name on the old panel.
    pub name: String,
    /// Kind of server.
    pub engine: Engine,
    /// The SQL dump inside the archive, when the archive has one.
    pub dump: Option<ArchivePath>,
    /// Size of the dump, bytes.
    pub dump_bytes: Option<u64>,
}

/// What a database user may do on one database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// The database.
    pub database: String,
    /// Everything, or the listed privileges (upper case).
    pub all_privileges: bool,
    /// The privileges when not all.
    pub privileges: Vec<String>,
}

/// A database user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbUser {
    /// Name.
    pub name: String,
    /// Kind of server.
    pub engine: Engine,
    /// The password's hash as the old server stored it (the executor does not reuse
    /// it blindly: it sets a new password unless the server accepts the hash).
    pub password_hash: Option<String>,
    /// What it may do.
    pub grants: Vec<Grant>,
}

/// A mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mailbox {
    /// Part before the `@`.
    pub local: String,
    /// The domain.
    pub domain: String,
    /// Quota in MiB; `None` is unlimited or unknown.
    pub quota_mb: Option<u64>,
    /// The password hash as stored (crypt format), when the archive has it.
    pub password_hash: Option<String>,
    /// The mail directory inside the archive, when the archive has the mail.
    pub maildir: Option<ArchivePath>,
}

/// A forwarder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Forwarder {
    /// Part before the `@`; `*` is a catch-all.
    pub local: String,
    /// The domain.
    pub domain: String,
    /// Where the mail goes.
    pub destinations: Vec<String>,
}

/// The account's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Files {
    /// The home directory inside the archive.
    pub root: ArchivePath,
    /// How many entries it holds.
    pub entries: u64,
    /// How many bytes of file content.
    pub bytes: u64,
}

/// A scheduled job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronJob {
    /// Five fields (`*/5 * * * *`) or a `@` shortcut.
    pub schedule: String,
    /// The command line.
    pub command: String,
}

/// One DNS record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRecord {
    /// Name relative to the zone; `@` is the zone itself.
    pub name: String,
    /// `A`, `AAAA`, `CNAME`, `MX`, `TXT`, `NS`, `SRV` or `CAA`.
    pub rtype: String,
    /// Time to live, seconds, when the record has its own.
    pub ttl: Option<u32>,
    /// Priority (`MX`, `SRV`).
    pub priority: Option<u16>,
    /// The rest of the data.
    pub content: String,
}

/// A DNS zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsZone {
    /// The zone's name.
    pub name: String,
    /// Its records (without the SOA and the zone's own name servers).
    pub records: Vec<DnsRecord>,
}

/// Something the reader could not bring over, or had to guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// Short stable code (`custom_certificate_skipped`).
    pub code: String,
    /// What happened, for people.
    pub message: String,
}

/// Everything the executor needs to know about the backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportModel {
    /// [`MODEL_VERSION`].
    pub version: u32,
    /// Where it came from.
    pub source: Source,
    /// The account.
    pub account: Account,
    /// Domains.
    pub domains: Vec<Domain>,
    /// Databases.
    pub databases: Vec<Database>,
    /// Database users.
    pub db_users: Vec<DbUser>,
    /// Mailboxes.
    pub mailboxes: Vec<Mailbox>,
    /// Forwarders.
    pub forwarders: Vec<Forwarder>,
    /// The home directory.
    pub files: Option<Files>,
    /// Scheduled jobs.
    pub cron_jobs: Vec<CronJob>,
    /// DNS zones.
    pub dns_zones: Vec<DnsZone>,
    /// What was skipped or guessed.
    pub warnings: Vec<Warning>,
}

impl ImportModel {
    /// An empty model for `account`.
    #[must_use]
    pub fn new(source: Source, account: Account) -> Self {
        Self {
            version: MODEL_VERSION,
            source,
            account,
            domains: Vec::new(),
            databases: Vec::new(),
            db_users: Vec::new(),
            mailboxes: Vec::new(),
            forwarders: Vec::new(),
            files: None,
            cron_jobs: Vec::new(),
            dns_zones: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Adds a warning.
    pub fn warn(&mut self, code: &str, message: impl Into<String>) {
        self.warnings.push(Warning {
            code: code.to_owned(),
            message: message.into(),
        });
    }

    /// Checks that the model is something the executor may act on.
    ///
    /// # Errors
    /// [`ImportError::Invalid`] naming the first problem.
    pub fn validate(&self) -> Result<(), ImportError> {
        let bad = |what: String| Err(ImportError::Invalid(what));
        if self.version != MODEL_VERSION {
            return bad(format!(
                "model version {} is not {MODEL_VERSION}",
                self.version
            ));
        }
        if self.account.username.is_empty() || self.account.username.len() > 64 {
            return bad("the account has no usable username".into());
        }
        let mut seen = BTreeSet::new();
        for d in &self.domains {
            if !valid_domain(&d.name) {
                return bad(format!("`{}` is not a domain name", d.name));
            }
            if !seen.insert(d.name.as_str()) {
                return bad(format!("domain `{}` appears twice", d.name));
            }
        }
        if self
            .domains
            .iter()
            .filter(|d| d.kind == DomainKind::Main)
            .count()
            > 1
        {
            return bad("more than one main domain".into());
        }
        let mut dbs = BTreeSet::new();
        for d in &self.databases {
            if !valid_identifier(&d.name) {
                return bad(format!("`{}` is not a database name", d.name));
            }
            if !dbs.insert((d.engine as u8, d.name.as_str())) {
                return bad(format!("database `{}` appears twice", d.name));
            }
        }
        for u in &self.db_users {
            if !valid_identifier(&u.name) {
                return bad(format!("`{}` is not a database user name", u.name));
            }
            for g in &u.grants {
                if !valid_identifier(&g.database) {
                    return bad(format!("`{}` is not a database name", g.database));
                }
            }
        }
        for m in &self.mailboxes {
            if m.local.is_empty() || m.local.contains('@') || !valid_domain(&m.domain) {
                return bad(format!("`{}@{}` is not a mailbox", m.local, m.domain));
            }
        }
        for z in &self.dns_zones {
            if !valid_domain(&z.name) {
                return bad(format!("`{}` is not a zone name", z.name));
            }
        }
        Ok(())
    }
}

/// Whether `s` looks like a host name: labels of letters, digits and `-`, at least two labels.
#[must_use]
pub fn valid_domain(s: &str) -> bool {
    s.len() <= 253
        && s.contains('.')
        && s.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Whether `s` is a database or user name: 1-64 of letters, digits, `_`, `-`, `$`.
#[must_use]
pub fn valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'$'))
}
