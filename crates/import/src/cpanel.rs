//! cPanel backups: `cpmove-<user>.tar.gz` and the full-account backups (T15.02).
//!
//! The parts of the archive that matter:
//!
//! | In the archive | Is |
//! |---|---|
//! | `cp/<user>` | `KEY=value` lines about the account (`USER`, `DNS`, `DNS1`..., `CONTACTEMAIL`, `PLAN`) |
//! | `userdata/main` | main, addon, parked and sub domains |
//! | `userdata/<domain>` | one domain's document root and PHP version |
//! | `mysql.sql`, `mysql/<db>.sql` | grants, and a dump per database |
//! | `psql/<db>.sql` | a PostgreSQL dump per database |
//! | `dnszones/<zone>.db` | the DNS zones |
//! | `cron/<user>` | the crontab |
//! | `homedir/` | the files, `homedir/etc/<domain>/{shadow,quota}` and `homedir/mail/` the mail |
//! | `valiases/<domain>` | forwarders |

use std::collections::BTreeMap;

use crate::{
    ImportError,
    archive::Archive,
    model::{
        Account, ArchivePath, CronJob, Database, DbUser, Domain, DomainKind, Engine, Files,
        Forwarder, Grant, ImportModel, Mailbox, Source, SourceKind, valid_domain, valid_identifier,
    },
    zone::parse_zone,
};

/// Whether an archive looks like a cPanel backup.
#[must_use]
pub fn detect(a: &Archive) -> bool {
    a.files.keys().any(|f| {
        f.strip_prefix("cp/")
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
    }) && (a.has_dir("userdata") || a.has_dir("mysql") || a.has("version") || a.has_dir("dnszones"))
}

/// Which home paths the reader wants listed (the mail directories' top level).
#[must_use]
pub fn keep_home(path: &str) -> bool {
    path.strip_prefix("homedir/mail/")
        .is_some_and(|rest| rest.split('/').count() <= 2)
        || path.strip_prefix("homedir/etc/").is_some_and(|rest| {
            let mut parts = rest.split('/');
            parts.next().is_some() && matches!(parts.next(), Some("shadow" | "quota" | "passwd"))
        })
}

fn pairs(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect()
}

/// The flat keys, one-level maps and lists of a cPanel `userdata` file: just what is
/// needed here, not YAML.
#[derive(Debug, Default)]
struct UserData {
    scalars: BTreeMap<String, String>,
    lists: BTreeMap<String, Vec<String>>,
    maps: BTreeMap<String, BTreeMap<String, String>>,
}

fn userdata(text: &str) -> UserData {
    let mut out = UserData::default();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') || trimmed == "---" {
            continue;
        }
        let indent = trimmed.len() - trimmed.trim_start().len();
        let body = trimmed.trim_start();
        if indent == 0 {
            let Some((k, v)) = body.split_once(':') else {
                current = None;
                continue;
            };
            let (k, v) = (k.trim().to_owned(), unquote(v.trim()));
            if v.is_empty() {
                current = Some(k);
            } else {
                out.scalars.insert(k, v);
                current = None;
            }
        } else if let Some(key) = &current {
            if let Some(item) = body.strip_prefix("- ") {
                out.lists
                    .entry(key.clone())
                    .or_default()
                    .push(unquote(item.trim()));
            } else if let Some((k, v)) = body.split_once(':') {
                out.maps
                    .entry(key.clone())
                    .or_default()
                    .insert(unquote(k.trim()), unquote(v.trim()));
            }
        }
    }
    out
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| s.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .unwrap_or(s);
    if s == "[]" || s == "{}" {
        String::new()
    } else {
        s.to_owned()
    }
}

/// `/home/user/public_html/x` becomes `public_html/x`.
fn relative_docroot(path: &str, home: Option<&str>, user: &str) -> Option<String> {
    let candidates = [
        home.map(str::to_owned),
        Some(format!("/home/{user}")),
        Some(format!("/home2/{user}")),
        Some(format!("/home3/{user}")),
    ];
    for c in candidates.into_iter().flatten() {
        if let Some(rest) = path.strip_prefix(&format!("{}/", c.trim_end_matches('/'))) {
            let rest = rest.trim_matches('/');
            if !rest.is_empty() && !rest.split('/').any(|p| p == ".." || p == ".") {
                return Some(rest.to_owned());
            }
        }
    }
    None
}

/// `ea-php81` becomes `8.1`.
fn php_version(s: &str) -> Option<String> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    if !s.contains("php") || digits.len() < 2 || digits.len() > 3 {
        return None;
    }
    let (major, minor) = digits.split_at(1);
    Some(format!("{major}.{minor}"))
}

/// Reads the model out of an archive.
///
/// # Errors
/// [`ImportError::Invalid`] when the archive has no account.
pub fn parse(a: &Archive) -> Result<ImportModel, ImportError> {
    let (cp_name, _) = a
        .files_in("cp")
        .next()
        .ok_or_else(|| ImportError::Invalid("no cp/<user> file: not a cPanel backup".into()))?;
    let cp = pairs(&a.text(&format!("cp/{cp_name}")).unwrap_or_default());
    let username = cp
        .get("USER")
        .cloned()
        .unwrap_or_else(|| cp_name.to_owned());
    let main = userdata(&a.text("userdata/main").unwrap_or_default());
    let main_domain = main
        .scalars
        .get("main_domain")
        .or_else(|| cp.get("DNS"))
        .map(|d| d.to_ascii_lowercase());
    let version = a
        .text("version")
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let mut model = ImportModel::new(
        Source {
            kind: SourceKind::Cpanel,
            version,
        },
        Account {
            username: username.clone(),
            email: cp.get("CONTACTEMAIL").filter(|e| e.contains('@')).cloned(),
            plan: cp.get("PLAN").filter(|p| !p.is_empty()).cloned(),
            locale: cp.get("LOCALE").filter(|p| !p.is_empty()).cloned(),
            main_domain: main_domain.clone(),
            created_at: cp.get("STARTDATE").and_then(|s| s.parse().ok()),
        },
    );

    domains(a, &mut model, &cp, &main, main_domain.as_deref(), &username);
    databases(a, &mut model);
    dns(a, &mut model);
    cron(a, &mut model, &username);
    mail(a, &mut model);
    if a.home_entries > 0 {
        model.files = Some(Files {
            root: ArchivePath::new("homedir")?,
            entries: a.home_entries,
            bytes: a.home_bytes,
        });
    }
    if a.has_dir("sslkeys") || a.has_dir("ssl") || a.has_dir("sslcerts") {
        model.warn(
            "custom_certificate_skipped",
            "The backup has certificates; they are not carried over. Certificates are issued again.",
        );
    }
    if a.has_dir("mailman") {
        model.warn(
            "mailing_lists_skipped",
            "Mailing lists are not carried over.",
        );
    }
    Ok(model)
}

fn domains(
    a: &Archive,
    model: &mut ImportModel,
    cp: &BTreeMap<String, String>,
    main: &UserData,
    main_domain: Option<&str>,
    user: &str,
) {
    let mut seen = std::collections::BTreeSet::new();
    let mut add = |model: &mut ImportModel, name: &str, kind: DomainKind| {
        let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
        if name.is_empty() || !seen.insert(name.clone()) {
            return;
        }
        if !valid_domain(&name) {
            model.warn(
                "domain_skipped",
                format!("`{name}` is not a usable domain name."),
            );
            return;
        }
        let data = userdata(&a.text(&format!("userdata/{name}")).unwrap_or_default());
        let home = data.scalars.get("homedir").map(String::as_str);
        let docroot = data
            .scalars
            .get("documentroot")
            .and_then(|d| relative_docroot(d, home, user));
        let php = data.scalars.get("phpversion").and_then(|v| php_version(v));
        model.domains.push(Domain {
            name,
            kind,
            docroot,
            php_version: php,
        });
    };
    if let Some(m) = main_domain {
        add(model, m, DomainKind::Main);
    }
    for name in main
        .maps
        .get("addon_domains")
        .into_iter()
        .flat_map(BTreeMap::keys)
    {
        add(model, name, DomainKind::Addon);
    }
    for key in ["sub_domains", "subdomains"] {
        for name in main.lists.get(key).into_iter().flatten() {
            add(model, name, DomainKind::Sub);
        }
    }
    for name in main.lists.get("parked_domains").into_iter().flatten() {
        add(model, name, DomainKind::Alias);
    }
    // Older archives list every domain as DNS, DNS1, DNS2... in the account file.
    for (k, v) in cp {
        if k.starts_with("DNS") && k[3..].bytes().all(|b| b.is_ascii_digit()) {
            add(model, v, DomainKind::Alias);
        }
    }
}

fn sql_string(s: &str) -> Option<(&str, &str)> {
    let rest = s.strip_prefix('\'')?;
    let end = rest.find('\'')?;
    Some((&rest[..end], &rest[end + 1..]))
}

fn databases(a: &Archive, model: &mut ImportModel) {
    for (dir, engine) in [("mysql", Engine::Mysql), ("psql", Engine::Postgres)] {
        for (file, size) in a.files_in(dir) {
            let Some(name) = file.strip_suffix(".sql") else {
                continue;
            };
            if !valid_identifier(name) {
                model.warn(
                    "database_skipped",
                    format!("`{name}` is not a usable database name."),
                );
                continue;
            }
            let Ok(path) = ArchivePath::new(&format!("{dir}/{file}")) else {
                continue;
            };
            model.databases.push(Database {
                name: name.to_owned(),
                engine,
                dump: Some(path),
                dump_bytes: Some(size),
            });
        }
    }
    let Some(grants) = a.text("mysql.sql") else {
        return;
    };
    let mut users: BTreeMap<String, DbUser> = BTreeMap::new();
    for stmt in grants.split(";\n").map(str::trim) {
        let Some(rest) = stmt.strip_prefix("GRANT ") else {
            continue;
        };
        let Some((privs, rest)) = rest.split_once(" ON ") else {
            continue;
        };
        let Some((target, rest)) = rest.split_once(" TO ") else {
            continue;
        };
        let Some((user, rest)) = sql_string(rest.trim()) else {
            continue;
        };
        let hash = rest
            .find("IDENTIFIED BY PASSWORD ")
            .and_then(|i| sql_string(&rest[i + "IDENTIFIED BY PASSWORD ".len()..]))
            .map(|(h, _)| h.to_owned());
        if !valid_identifier(user) {
            continue;
        }
        let entry = users.entry(user.to_owned()).or_insert_with(|| DbUser {
            name: user.to_owned(),
            engine: Engine::Mysql,
            password_hash: None,
            grants: Vec::new(),
        });
        if hash.is_some() {
            entry.password_hash = hash;
        }
        let (db, table) = target.rsplit_once('.').unwrap_or((target, "*"));
        let db = db.trim_matches('`').replace("\\_", "_").replace("\\%", "%");
        if db == "*" || table != "*" || !valid_identifier(&db) {
            continue;
        }
        let all = privs.trim().eq_ignore_ascii_case("ALL PRIVILEGES");
        entry.grants.push(Grant {
            database: db,
            all_privileges: all,
            privileges: if all {
                Vec::new()
            } else {
                privs
                    .split(',')
                    .map(|p| p.trim().to_ascii_uppercase())
                    .filter(|p| !p.is_empty())
                    .collect()
            },
        });
    }
    model.db_users.extend(users.into_values());
}

fn dns(a: &Archive, model: &mut ImportModel) {
    for (file, _) in a.files_in("dnszones") {
        let Some(zone) = file.strip_suffix(".db") else {
            continue;
        };
        let Some(text) = a.text(&format!("dnszones/{file}")) else {
            model.warn(
                "dns_zone_skipped",
                format!("The zone `{zone}` is too big to read."),
            );
            continue;
        };
        let (z, skipped) = parse_zone(zone, &text);
        if !valid_domain(&z.name) {
            continue;
        }
        for s in skipped {
            model.warn("dns_record_skipped", format!("{zone}: {s}"));
        }
        model.dns_zones.push(z);
    }
}

fn cron(a: &Archive, model: &mut ImportModel, user: &str) {
    let Some(text) = a.text(&format!("cron/{user}")).or_else(|| {
        a.files_in("cron")
            .next()
            .and_then(|(f, _)| a.text(&format!("cron/{f}")))
    }) else {
        return;
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `MAILTO=me@example.com` and friends.
        if let Some((k, _)) = line.split_once('=')
            && !k.contains(char::is_whitespace)
            && !k.starts_with('*')
        {
            continue;
        }
        let (schedule, command) = if line.starts_with('@') {
            match line.split_once(char::is_whitespace) {
                Some((s, c)) => (s.to_owned(), c.trim().to_owned()),
                None => continue,
            }
        } else {
            let parts: Vec<&str> = line.splitn(6, char::is_whitespace).collect();
            if parts.len() < 6 {
                model.warn("cron_skipped", format!("Could not read `{line}`."));
                continue;
            }
            (parts[..5].join(" "), parts[5].trim().to_owned())
        };
        if command.is_empty() {
            continue;
        }
        model.cron_jobs.push(CronJob { schedule, command });
    }
}

fn colon_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        l.split_once(':').map(|(k, v)| (k.trim(), v.trim()))
    })
}

fn mail(a: &Archive, model: &mut ImportModel) {
    let mut domains = std::collections::BTreeSet::new();
    for path in a.home_paths.keys() {
        if let Some(rest) = path.strip_prefix("homedir/etc/")
            && let Some(domain) = rest.split('/').next()
        {
            domains.insert(domain.to_ascii_lowercase());
        }
    }
    for domain in domains {
        if !valid_domain(&domain) {
            continue;
        }
        let shadow = a
            .text(&format!("homedir/etc/{domain}/shadow"))
            .or_else(|| a.text(&format!("homedir/etc/{domain}/passwd")))
            .unwrap_or_default();
        let quota_text = a
            .text(&format!("homedir/etc/{domain}/quota"))
            .unwrap_or_default();
        let quota_of = |local: &str| {
            colon_lines(&quota_text)
                .find(|(k, _)| *k == local)
                .and_then(|(_, v)| v.parse::<u64>().ok())
                .filter(|b| *b > 0)
                .map(|b| b.div_ceil(1024 * 1024))
        };
        for (local, rest) in colon_lines(&shadow) {
            let local = local.to_ascii_lowercase();
            if local.is_empty() || local.contains('@') || local.contains('/') {
                continue;
            }
            let hash = rest
                .split(':')
                .next()
                .filter(|h| h.starts_with('$'))
                .map(str::to_owned);
            let maildir = format!("homedir/mail/{domain}/{local}");
            model.mailboxes.push(Mailbox {
                quota_mb: quota_of(&local),
                password_hash: hash,
                maildir: a
                    .has_dir(&maildir)
                    .then(|| ArchivePath::new(&maildir).ok())
                    .flatten(),
                local,
                domain: domain.clone(),
            });
        }
    }
    let alias_files: Vec<String> = a.files_in("valiases").map(|(f, _)| f.to_owned()).collect();
    for file in alias_files {
        let domain = file.to_ascii_lowercase();
        if !valid_domain(&domain) {
            continue;
        }
        for (local, dests) in colon_lines(&a.text(&format!("valiases/{file}")).unwrap_or_default())
        {
            let destinations: Vec<String> = dests
                .split(',')
                .map(|d| d.trim().to_owned())
                .filter(|d| d.contains('@') && !d.starts_with('|') && !d.starts_with(':'))
                .collect();
            if destinations.is_empty() {
                if !dests.is_empty() {
                    model.warn(
                        "forwarder_skipped",
                        format!("The forwarder `{local}@{domain}` is not a plain address list."),
                    );
                }
                continue;
            }
            model.forwarders.push(Forwarder {
                local: local.to_owned(),
                domain: domain.clone(),
                destinations,
            });
        }
    }
}
