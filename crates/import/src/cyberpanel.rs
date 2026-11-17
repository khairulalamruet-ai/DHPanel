//! CyberPanel website backups (T15.03).
//!
//! A backup is a tar.gz with a `meta.xml` describing the website (`<VirtualHost>`: its
//! domain, PHP selection, child domains, databases and mail accounts), a dump per database
//! (`<database>.sql`) and the website's files (a folder named after the domain, or
//! `public_html`).
//!
//! The reader only relies on `meta.xml` and on those names, and tells what it did not find
//! in a warning rather than guessing. It was written from the format's public description,
//! not from a real backup: a backup that differs in its details shows up as warnings.

use roxmltree::{Document, Node};

use crate::{
    ImportError,
    archive::Archive,
    model::{
        Account, ArchivePath, Database, DbUser, Domain, DomainKind, Engine, Files, Grant,
        ImportModel, Mailbox, Source, SourceKind, valid_domain, valid_identifier,
    },
};

/// Whether an archive looks like a CyberPanel backup.
#[must_use]
pub fn detect(a: &Archive) -> bool {
    a.text("meta.xml")
        .is_some_and(|x| x.contains("<VirtualHost") || x.contains("<metaVersion"))
}

/// No part of the home directory is of interest to this reader.
#[must_use]
pub fn keep_home(_path: &str) -> bool {
    false
}

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(name))
}

fn text<'a>(n: Node<'a, 'a>, name: &str) -> Option<String> {
    child(n, name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

fn php(s: &str) -> Option<String> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        2 => Some(format!("{}.{}", &digits[..1], &digits[1..])),
        _ => None,
    }
}

/// Reads the model out of an archive.
///
/// # Errors
/// [`ImportError::Invalid`] when `meta.xml` is missing or not the expected document.
pub fn parse(a: &Archive) -> Result<ImportModel, ImportError> {
    let xml = a
        .text("meta.xml")
        .ok_or_else(|| ImportError::Invalid("no meta.xml: not a CyberPanel backup".into()))?;
    let doc = Document::parse(&xml)
        .map_err(|e| ImportError::Invalid(format!("meta.xml cannot be read: {e}")))?;
    let root = doc.root_element();
    let domain = text(root, "domain")
        .map(|d| d.to_ascii_lowercase())
        .ok_or_else(|| ImportError::Invalid("meta.xml names no domain".into()))?;
    let username = text(root, "externalApp").unwrap_or_else(|| domain.clone());
    let mut model = ImportModel::new(
        Source {
            kind: SourceKind::Cyberpanel,
            version: text(root, "VERSION").or_else(|| text(root, "metaVersion")),
        },
        Account {
            username,
            email: text(root, "adminEmail").filter(|e| e.contains('@')),
            plan: text(root, "package"),
            locale: None,
            main_domain: Some(domain.clone()),
            created_at: None,
        },
    );
    if !valid_domain(&domain) {
        return Err(ImportError::Invalid(format!(
            "`{domain}` is not a domain name"
        )));
    }
    model.domains.push(Domain {
        name: domain.clone(),
        kind: DomainKind::Main,
        docroot: Some("public_html".to_owned()),
        php_version: text(root, "phpSelection").and_then(|p| php(&p)),
    });
    for list in ["ChildDomains", "child_domains"] {
        let Some(list) = child(root, list) else {
            continue;
        };
        for d in list.children().filter(Node::is_element) {
            let name = text(d, "domain").map(|n| n.to_ascii_lowercase());
            let Some(name) = name.filter(|n| valid_domain(n)) else {
                model.warn(
                    "domain_skipped",
                    "A child domain without a usable name was skipped.",
                );
                continue;
            };
            if model.domains.iter().any(|x| x.name == name) {
                continue;
            }
            let docroot = text(d, "path").and_then(|p| {
                p.rsplit_once("/public_html/")
                    .map(|(_, rest)| format!("public_html/{}", rest.trim_matches('/')))
                    .filter(|r| !r.split('/').any(|x| x == ".." || x.is_empty()))
            });
            model.domains.push(Domain {
                name,
                kind: DomainKind::Sub,
                docroot,
                php_version: text(d, "phpSelection").and_then(|p| php(&p)),
            });
        }
    }
    for list in ["Databases"] {
        let Some(list) = child(root, list) else {
            continue;
        };
        for d in list.children().filter(Node::is_element) {
            let Some(name) = text(d, "dbName").filter(|n| valid_identifier(n)) else {
                model.warn(
                    "database_skipped",
                    "A database without a usable name was skipped.",
                );
                continue;
            };
            let dump_name = format!("{name}.sql");
            let dump = a
                .has(&dump_name)
                .then(|| ArchivePath::new(&dump_name).ok())
                .flatten();
            model.databases.push(Database {
                name: name.clone(),
                engine: Engine::Mysql,
                dump_bytes: a.files.get(&dump_name).copied(),
                dump,
            });
            if let Some(user) = text(d, "dbUser").filter(|u| valid_identifier(u)) {
                // CyberPanel's backup holds the user's password hash only in some versions.
                let hash = text(d, "password").filter(|p| p.starts_with('*'));
                model.db_users.push(DbUser {
                    name: user,
                    engine: Engine::Mysql,
                    password_hash: hash,
                    grants: vec![Grant {
                        database: name,
                        all_privileges: true,
                        privileges: Vec::new(),
                    }],
                });
            }
        }
    }
    for list in ["Emails"] {
        let Some(list) = child(root, list) else {
            continue;
        };
        for m in list.children().filter(Node::is_element) {
            let Some((local, dom)) = text(m, "email")
                .map(|e| e.to_ascii_lowercase())
                .and_then(|e| e.split_once('@').map(|(l, d)| (l.to_owned(), d.to_owned())))
            else {
                continue;
            };
            if local.is_empty() || !valid_domain(&dom) {
                continue;
            }
            model.mailboxes.push(Mailbox {
                local,
                domain: dom,
                quota_mb: None,
                password_hash: text(m, "password").filter(|p| p.starts_with('$')),
                maildir: None,
            });
        }
    }
    if !model.mailboxes.is_empty() {
        model.warn(
            "mail_content_not_in_backup",
            "The backup lists mail accounts but does not hold their mail.",
        );
    }
    // The website's files: a folder of the domain's name, or public_html.
    for dir in [domain.as_str(), "public_html", "home"] {
        if a.has_dir(dir) {
            if let Ok(root) = ArchivePath::new(dir) {
                let prefix = format!("{dir}/");
                let (entries, bytes) = a
                    .files
                    .iter()
                    .filter(|(p, _)| p.starts_with(&prefix))
                    .fold((0u64, 0u64), |(n, b), (_, s)| (n + 1, b + s));
                model.files = Some(Files {
                    root,
                    entries,
                    bytes,
                });
            }
            break;
        }
    }
    if model.files.is_none() {
        model.warn(
            "no_files_found",
            "The backup has no folder of the website's files (a nested archive is not opened here).",
        );
    }
    Ok(model)
}
