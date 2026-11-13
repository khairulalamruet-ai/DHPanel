//! `module.toml`: what a module is, in one file (T03.01).
//!
//! Every module ships a manifest at `modules/<name>/module.toml`. It is
//! compiled into the binary and checked at startup, so a mistake stops the
//! service with a message that names the module, the field and the problem —
//! all problems at once, not one per restart.
//!
//! ```toml
//! [module]
//! name       = "domains"
//! version    = "1.0.0"
//! title      = { en = "Domains", bn = "ডোমেইন" }
//! category   = "web"
//! core       = false
//! depends_on = ["webserver", "dns"]
//!
//! [permissions]
//! keys = ["domains.read", "domains.create"]
//!
//! [features]
//! keys = ["domains", "redirects"]
//!
//! [limits]
//! max_domains = { type = "int", default = 10, unlimited = -1 }
//!
//! [ui]
//! admin_nav  = { group = "Web", order = 10, icon = "globe" }
//! client_nav = { group = "Domains", order = 10, icon = "globe" }
//!
//! [agent]
//! operations = ["AttachDomain", "DetachDomain"]
//! ```
//!
//! Unknown tables and keys are errors: a typo must not silently do nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Largest manifest accepted, in bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// What kind of thing a module is (used to group it in the interface).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Always on: accounts, roles, nodes, settings, ...
    Core,
    /// Web serving: domains, DNS, TLS, web server, PHP.
    Web,
    /// Databases.
    Db,
    /// E-mail.
    Mail,
    /// Files, FTP, backups.
    Files,
    /// Firewall, WAF, malware scanning.
    Security,
    /// Services, cron, logs, monitoring.
    System,
    /// Application installers and runtimes.
    Apps,
}

impl Category {
    /// The value used in manifests and in the API.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Web => "web",
            Self::Db => "db",
            Self::Mail => "mail",
            Self::Files => "files",
            Self::Security => "security",
            Self::System => "system",
            Self::Apps => "apps",
        }
    }
}

/// The `[module]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleInfo {
    /// Unique name: `a-z`, digits and `-`, 2-32 characters, starting with a letter.
    pub name: String,
    /// `MAJOR.MINOR.PATCH`.
    pub version: String,
    /// Display name by language code; `en` is required.
    pub title: BTreeMap<String, String>,
    /// Grouping in the interface.
    pub category: Category,
    /// A core module cannot be switched off.
    #[serde(default)]
    pub core: bool,
    /// Modules that must be enabled for this one to work.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// A table that is just a list of keys (`[permissions]`, `[features]`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    /// The keys.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Kind of a plan limit. Only whole numbers so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitType {
    /// A count (domains, databases, megabytes, ...).
    Int,
}

/// One entry of `[limits]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitDef {
    /// Kind of value.
    #[serde(rename = "type")]
    pub kind: LimitType,
    /// Value a new plan starts with (zero or more).
    pub default: i64,
    /// The value that means "no limit"; if present it must be `-1`.
    #[serde(default)]
    pub unlimited: Option<i64>,
}

/// Where a module appears in a navigation menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nav {
    /// Menu group title.
    pub group: String,
    /// Position inside the group (0-10000, lower first).
    pub order: u32,
    /// Icon name (`a-z`, digits, `-`).
    pub icon: String,
}

/// The `[ui]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    /// Entry in the administrator's menu.
    #[serde(default)]
    pub admin_nav: Option<Nav>,
    /// Entry in the customer's menu.
    #[serde(default)]
    pub client_nav: Option<Nav>,
}

/// The `[agent]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    /// Node operations this module uses, in `PascalCase`.
    #[serde(default)]
    pub operations: Vec<String>,
}

/// A parsed and validated manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Identity.
    pub module: ModuleInfo,
    /// RBAC permission keys the module defines.
    #[serde(default)]
    pub permissions: Keys,
    /// Feature keys a plan can switch on or off.
    #[serde(default)]
    pub features: Keys,
    /// Numeric plan limits.
    #[serde(default)]
    pub limits: BTreeMap<String, LimitDef>,
    /// Navigation.
    #[serde(default)]
    pub ui: Ui,
    /// Node operations.
    #[serde(default)]
    pub agent: Agent,
}

/// Why a manifest was refused: every problem found, each naming its field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid manifest for {origin}:{}", .problems.iter().map(|p| format!("\n  - {p}")).collect::<String>())]
pub struct ManifestError {
    /// Which manifest: the module name when known, else what the caller called it.
    pub origin: String,
    /// One line per problem.
    pub problems: Vec<String>,
}

fn valid_module_name(name: &str) -> bool {
    (2..=32).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 9
                && p.chars().all(|c| c.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'))
        })
}

/// `[a-z][a-z0-9_]*`, at most `max` characters.
fn valid_word(word: &str, max: usize) -> bool {
    (1..=max).contains(&word.len())
        && word.starts_with(|c: char| c.is_ascii_lowercase())
        && word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Same rule as the RBAC tables: at least two dot-separated words.
fn valid_permission_key(key: &str) -> bool {
    key.len() <= 96 && key.split('.').count() >= 2 && key.split('.').all(|s| valid_word(s, 48))
}

fn valid_operation_name(name: &str) -> bool {
    (2..=48).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

fn printable(text: &str) -> String {
    text.chars()
        .take(64)
        .flat_map(char::escape_default)
        .collect()
}

impl Manifest {
    /// Parses and validates manifest text. `origin` says where the text came
    /// from (a module name or a path) and is used in the error.
    ///
    /// # Errors
    /// [`ManifestError`] listing every problem.
    pub fn parse(origin: &str, text: &str) -> Result<Self, ManifestError> {
        let fail = |problems: Vec<String>| ManifestError {
            origin: origin.to_owned(),
            problems,
        };
        if text.len() > MAX_MANIFEST_BYTES {
            return Err(fail(vec![format!(
                "the file is larger than {MAX_MANIFEST_BYTES} bytes"
            )]));
        }
        let manifest: Self = toml::from_str(text).map_err(|e| {
            // toml's message names the line, the key and what was expected.
            fail(vec![e.to_string().trim_end().replace('\n', "\n    ")])
        })?;
        let problems = manifest.problems();
        if problems.is_empty() {
            Ok(manifest)
        } else {
            Err(ManifestError {
                origin: if valid_module_name(&manifest.module.name) {
                    format!("module `{}`", manifest.module.name)
                } else {
                    origin.to_owned()
                },
                problems,
            })
        }
    }

    /// Every rule a manifest breaks, as `field: problem` lines.
    fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let m = &self.module;
        if !valid_module_name(&m.name) {
            p.push(format!(
                "module.name: `{}` must be 2-32 characters of a-z, 0-9 and `-`, starting with a letter",
                printable(&m.name)
            ));
        }
        if !valid_version(&m.version) {
            p.push(format!(
                "module.version: `{}` must be MAJOR.MINOR.PATCH, for example 1.0.0",
                printable(&m.version)
            ));
        }
        if !m.title.contains_key("en") {
            p.push("module.title: an English title (`en`) is required".to_owned());
        }
        for (lang, title) in &m.title {
            if !(lang.len() == 2 && lang.chars().all(|c| c.is_ascii_lowercase())) {
                p.push(format!(
                    "module.title: `{}` is not a two-letter language code",
                    printable(lang)
                ));
            }
            let length = title.chars().count();
            if !(1..=64).contains(&length) || title.chars().any(char::is_control) {
                p.push(format!(
                    "module.title.{}: must be 1-64 characters",
                    printable(lang)
                ));
            }
        }
        if m.core && m.category != Category::Core {
            p.push("module.core: only a module of category `core` can be core".to_owned());
        }
        let mut seen = BTreeSet::new();
        for dep in &m.depends_on {
            if !valid_module_name(dep) {
                p.push(format!(
                    "module.depends_on: `{}` is not a module name",
                    printable(dep)
                ));
            } else if dep == &m.name {
                p.push("module.depends_on: a module cannot depend on itself".to_owned());
            } else if !seen.insert(dep) {
                p.push(format!("module.depends_on: `{dep}` is listed twice"));
            }
        }
        check_keys(
            &mut p,
            "permissions.keys",
            &self.permissions.keys,
            valid_permission_key,
            "must be dot-separated lower-case words, for example domains.read",
        );
        check_keys(
            &mut p,
            "features.keys",
            &self.features.keys,
            |k| valid_word(k, 48),
            "must be a lower-case word of a-z, 0-9 and `_`",
        );
        for (key, limit) in &self.limits {
            if !valid_word(key, 48) {
                p.push(format!(
                    "limits: `{}` must be a lower-case word of a-z, 0-9 and `_`",
                    printable(key)
                ));
            }
            if let Some(unlimited) = limit.unlimited
                && unlimited != -1
            {
                p.push(format!(
                    "limits.{key}.unlimited: must be -1 when present, got {unlimited}"
                ));
            }
            let default_ok = limit.default >= 0 || limit.unlimited == Some(limit.default);
            if !default_ok {
                p.push(format!(
                    "limits.{key}.default: must be zero or more (or -1 if the limit can be unlimited), got {}",
                    limit.default
                ));
            }
        }
        for (field, nav) in [
            ("ui.admin_nav", &self.ui.admin_nav),
            ("ui.client_nav", &self.ui.client_nav),
        ] {
            let Some(nav) = nav else { continue };
            let group_len = nav.group.chars().count();
            if !(1..=32).contains(&group_len) || nav.group.chars().any(char::is_control) {
                p.push(format!("{field}.group: must be 1-32 characters"));
            }
            if nav.order > 10_000 {
                p.push(format!(
                    "{field}.order: must be between 0 and 10000, got {}",
                    nav.order
                ));
            }
            let icon_ok = (1..=32).contains(&nav.icon.len())
                && nav
                    .icon
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !icon_ok {
                p.push(format!(
                    "{field}.icon: `{}` must be 1-32 characters of a-z, 0-9 and `-`",
                    printable(&nav.icon)
                ));
            }
        }
        check_keys(
            &mut p,
            "agent.operations",
            &self.agent.operations,
            valid_operation_name,
            "must be PascalCase, for example AttachDomain",
        );
        p
    }
}

fn check_keys(
    problems: &mut Vec<String>,
    field: &str,
    keys: &[String],
    valid: impl Fn(&str) -> bool,
    rule: &str,
) {
    if keys.len() > 256 {
        problems.push(format!("{field}: at most 256 entries"));
    }
    let mut seen = BTreeSet::new();
    for key in keys {
        if !valid(key) {
            problems.push(format!("{field}: `{}` {rule}", printable(key)));
        } else if !seen.insert(key) {
            problems.push(format!("{field}: `{key}` is listed twice"));
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[module]
name       = "domains"
version    = "1.0.0"
title      = { en = "Domains", bn = "ডোমেইন" }
category   = "web"
core       = false
depends_on = ["webserver", "dns"]

[permissions]
keys = ["domains.read", "domains.create", "domains.update", "domains.delete"]

[features]
keys = ["domains", "redirects", "docroot", "temporary_links"]

[limits]
max_domains    = { type = "int", default = 10, unlimited = -1 }
max_subdomains = { type = "int", default = 20 }

[ui]
admin_nav  = { group = "Web", order = 10, icon = "globe" }
client_nav = { group = "Domains", order = 10, icon = "globe" }

[agent]
operations = ["AttachDomain", "DetachDomain", "SetDocroot", "SetVhostPolicy"]
"#;

    #[test]
    fn the_documented_example_parses() {
        let m = Manifest::parse("domains", GOOD).unwrap();
        assert_eq!(m.module.name, "domains");
        assert_eq!(m.module.category, Category::Web);
        assert_eq!(m.module.title["bn"], "ডোমেইন");
        assert_eq!(m.module.depends_on, ["webserver", "dns"]);
        assert_eq!(m.permissions.keys.len(), 4);
        assert_eq!(m.limits["max_domains"].unlimited, Some(-1));
        assert_eq!(m.limits["max_subdomains"].default, 20);
        assert_eq!(m.ui.admin_nav.as_ref().unwrap().icon, "globe");
        assert_eq!(m.agent.operations[0], "AttachDomain");
    }

    #[test]
    fn a_minimal_manifest_is_enough() {
        let m = Manifest::parse(
            "auth",
            "[module]\nname = \"auth\"\nversion = \"0.1.0\"\ntitle = { en = \"Sign-in\" }\ncategory = \"core\"\ncore = true\n",
        )
        .unwrap();
        assert!(m.module.core && m.module.depends_on.is_empty());
        assert!(m.permissions.keys.is_empty() && m.limits.is_empty() && m.ui.admin_nav.is_none());
    }

    #[test]
    fn every_problem_is_reported_at_once_with_its_field() {
        let bad = r#"
[module]
name       = "Bad Name"
version    = "1.0"
title      = { bn = "শিরোনাম", english = "x" }
category   = "web"
core       = true
depends_on = ["dns", "dns", "Bad", "ok-dep"]

[permissions]
keys = ["read", "domains.Read", "domains.read", "domains.read"]

[features]
keys = ["Has Space", "fine"]

[limits]
"Bad-Key"  = { type = "int", default = 1 }
max_things = { type = "int", default = -5, unlimited = 0 }

[ui]
admin_nav = { group = "", order = 99999, icon = "Globe!" }

[agent]
operations = ["attachDomain", "Attach Domain", "Fine", "Fine"]
"#;
        let err = Manifest::parse("modules/bad/module.toml", bad).unwrap_err();
        assert_eq!(
            err.origin, "modules/bad/module.toml",
            "the bad name is not used as origin"
        );
        let text = err.to_string();
        for expected in [
            "module.name: `Bad Name`",
            "module.version: `1.0` must be MAJOR.MINOR.PATCH",
            "module.title: an English title",
            "module.title: `english` is not a two-letter language code",
            "module.core: only a module of category `core`",
            "module.depends_on: `dns` is listed twice",
            "module.depends_on: `Bad` is not a module name",
            "permissions.keys: `read` must be dot-separated",
            "permissions.keys: `domains.Read`",
            "permissions.keys: `domains.read` is listed twice",
            "features.keys: `Has Space`",
            "limits: `Bad-Key`",
            "limits.max_things.unlimited: must be -1 when present, got 0",
            "limits.max_things.default: must be zero or more",
            "ui.admin_nav.group: must be 1-32 characters",
            "ui.admin_nav.order: must be between 0 and 10000, got 99999",
            "ui.admin_nav.icon: `Globe!`",
            "agent.operations: `attachDomain` must be PascalCase",
            "agent.operations: `Attach Domain`",
            "agent.operations: `Fine` is listed twice",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
        assert_eq!(err.problems.len(), 20, "{text}");
        assert!(text.starts_with("invalid manifest for modules/bad/module.toml:\n  - "));
    }

    #[test]
    fn typos_and_wrong_types_are_errors_that_name_the_place() {
        // An unknown key (a typo) must not be ignored.
        let typo = GOOD.replace("depends_on", "depend_on");
        let err = Manifest::parse("domains", &typo).unwrap_err();
        assert!(err.to_string().contains("depend_on"), "{err}");
        // An unknown table.
        let extra = format!("{GOOD}\n[hooks]\nrun = \"rm\"\n");
        assert!(
            Manifest::parse("domains", &extra)
                .unwrap_err()
                .to_string()
                .contains("hooks")
        );
        // A wrong type, with the line.
        let wrong = GOOD.replace(
            "order = 10, icon = \"globe\" }\nclient",
            "order = \"first\", icon = \"globe\" }\nclient",
        );
        let err = Manifest::parse("domains", &wrong).unwrap_err().to_string();
        assert!(err.contains("line") && err.contains("order"), "{err}");
        // An unknown category lists nothing dangerous and says what is allowed.
        let category = GOOD.replace("category   = \"web\"", "category   = \"internet\"");
        let err = Manifest::parse("domains", &category)
            .unwrap_err()
            .to_string();
        assert!(err.contains("internet") && err.contains("web"), "{err}");
        // Missing required parts.
        assert!(
            Manifest::parse("empty", "")
                .unwrap_err()
                .to_string()
                .contains("module")
        );
        assert!(
            Manifest::parse("x", "[module]\nname = \"ab\"\n")
                .unwrap_err()
                .to_string()
                .contains("version")
        );
        // Not TOML at all, and too large.
        assert!(Manifest::parse("x", "this is = = not toml").is_err());
        let huge = format!("{GOOD}\n# {}", "x".repeat(MAX_MANIFEST_BYTES));
        assert!(
            Manifest::parse("x", &huge)
                .unwrap_err()
                .to_string()
                .contains("larger than")
        );
    }

    #[test]
    fn names_versions_and_keys_follow_their_rules() {
        for ok in ["ab", "db-mysql", "apps-wordpress", "a1"] {
            assert!(valid_module_name(ok), "{ok}");
        }
        for bad in [
            "",
            "a",
            "-a",
            "a-",
            "a--b",
            "A",
            "1a",
            "a_b",
            "a.b",
            &"a".repeat(33),
        ] {
            assert!(!valid_module_name(bad), "{bad:?}");
        }
        for ok in ["0.1.0", "1.0.0", "12.34.56"] {
            assert!(valid_version(ok), "{ok}");
        }
        for bad in [
            "1",
            "1.0",
            "1.0.0.0",
            "1.0.x",
            "01.0.0",
            "1.0.0-beta",
            "v1.0.0",
            "",
        ] {
            assert!(!valid_version(bad), "{bad:?}");
        }
        for ok in ["domains.read", "admin.nodes.manage", "a.b_c.d1"] {
            assert!(valid_permission_key(ok), "{ok}");
        }
        for bad in [
            "domains",
            "domains.",
            ".read",
            "domains..read",
            "Domains.read",
            "domains.*",
            "a.1b",
        ] {
            assert!(!valid_permission_key(bad), "{bad:?}");
        }
        // A limit that can be unlimited may default to unlimited.
        let ok = "[module]\nname = \"ab\"\nversion = \"1.0.0\"\ntitle = { en = \"x\" }\ncategory = \"web\"\n[limits]\nmax = { type = \"int\", default = -1, unlimited = -1 }\n";
        assert!(Manifest::parse("ab", ok).is_ok());
    }
}
