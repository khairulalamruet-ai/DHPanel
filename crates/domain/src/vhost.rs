//! Web server configuration for a site (T06.03): the template an administrator
//! keeps, and what is made of it for one domain.
//!
//! The control plane renders; the node only writes what it is given. A template
//! is plain nginx configuration with four variables, `{{domain}}`,
//! `{{server_names}}`, `{{docroot}}` and `{{php_upstream}}`. Every value that goes
//! in comes from a checked type, so no domain, path or address can carry a
//! directive of its own into the file.

use crate::{Docroot, Fqdn};

/// Where a tenant's web container sees its files.
pub const WEB_ROOT: &str = "/var/www/html";
/// Longest template.
pub const MAX_TEMPLATE_BYTES: usize = 32 * 1024;
/// Most names (the domain and its aliases) in one `server_name`.
pub const MAX_SERVER_NAMES: usize = 50;
/// Variables a template may use.
pub const VARIABLES: [&str; 4] = ["domain", "server_names", "docroot", "php_upstream"];
/// Variables a template must use: without them it does not make a site.
const REQUIRED: [&str; 2] = ["server_names", "docroot"];

/// The template new installations start with.
pub const DEFAULT_NGINX_TEMPLATE: &str = r"# Managed by DHPanel for {{domain}}. Changes made here are overwritten.
server {
    listen 8080;
    server_name {{server_names}};
    root {{docroot}};
    index index.php index.html index.htm;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        try_files $uri =404;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_pass {{php_upstream}};
    }

    location ~ /\.(?!well-known) {
        deny all;
    }
}
";

/// What is wrong with a template or with a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct VhostError {
    /// Stable, for programs.
    pub code: &'static str,
    /// For people.
    pub message: String,
}

fn err(code: &'static str, message: impl Into<String>) -> VhostError {
    VhostError {
        code,
        message: message.into(),
    }
}

/// Directives no template may use: they read other files, load code or reach
/// outside the tenant's directory.
const FORBIDDEN: [&str; 8] = [
    "load_module",
    "alias",
    "perl",
    "perl_modules",
    "perl_require",
    "perl_set",
    "env",
    "ssi",
];

/// The directives (first word of each statement) in `text`, with their first argument.
fn directives(text: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    // Comments run to the end of the line (a `#` inside a quoted string is not
    // handled: such a template is refused as unbalanced or odd, never let through).
    let code: String = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    for statement in code.split([';', '{', '}']) {
        let mut words = statement.split_whitespace();
        if let Some(first) = words.next() {
            out.push((first.to_owned(), words.next().map(str::to_owned)));
        }
    }
    out
}

fn forbidden(name: &str, argument: Option<&str>) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "include" {
        // The one include a PHP site needs; any other file is out of reach.
        return argument != Some("fastcgi_params");
    }
    FORBIDDEN.contains(&lower.as_str())
        || lower.starts_with("js_")
        || lower.contains("_by_lua")
        || lower.starts_with("lua_")
}

/// Checks a template as an administrator saves it: well-formed, only known
/// variables, the required ones present, no forbidden directive, and the result
/// for a sample site is balanced.
///
/// # Errors
/// [`VhostError`] for the first problem.
pub fn check_template(body: &str) -> Result<(), VhostError> {
    if body.len() > MAX_TEMPLATE_BYTES {
        return Err(err("template_too_large", "The template is too large."));
    }
    if body.trim().is_empty() {
        return Err(err("template_empty", "The template is empty."));
    }
    if body
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(err(
            "template_characters",
            "The template holds control characters.",
        ));
    }
    let mut used = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(err("template_variable", "A {{ is never closed."));
        };
        let name = &after[..end];
        if !VARIABLES.contains(&name) {
            return Err(err(
                "unknown_variable",
                format!("{{{{{}}}}} is not a variable of a template.", name.trim()),
            ));
        }
        used.push(name);
        rest = &after[end + 2..];
    }
    if rest.contains("}}") {
        return Err(err("template_variable", "A }} has no {{."));
    }
    for required in REQUIRED {
        if !used.contains(&required) {
            return Err(err(
                "missing_variable",
                format!("The template must use {{{{{required}}}}}."),
            ));
        }
    }
    // Looked at with the variables filled in, so braces of the values count too.
    let sample = render_with(
        body,
        &Values {
            domain: "example.com".into(),
            server_names: "example.com www.example.com".into(),
            docroot: format!("{WEB_ROOT}/public_html"),
            php_upstream: "dh-sample-php:9000".into(),
        },
    );
    let mut depth: i32 = 0;
    for c in sample.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            break;
        }
    }
    if depth != 0 {
        return Err(err("template_braces", "The braces { } do not match."));
    }
    for (name, argument) in directives(&sample) {
        if forbidden(&name, argument.as_deref()) {
            return Err(err(
                "forbidden_directive",
                format!("The directive `{}` is not allowed.", name.trim()),
            ));
        }
    }
    Ok(())
}

struct Values {
    domain: String,
    server_names: String,
    docroot: String,
    php_upstream: String,
}

fn render_with(body: &str, values: &Values) -> String {
    body.replace("{{domain}}", &values.domain)
        .replace("{{server_names}}", &values.server_names)
        .replace("{{docroot}}", &values.docroot)
        .replace("{{php_upstream}}", &values.php_upstream)
}

/// A web container address, `name:port` (the PHP service of a tenant).
fn check_upstream(upstream: &str) -> Result<(), VhostError> {
    let ok = upstream.split_once(':').is_some_and(|(host, port)| {
        !host.is_empty()
            && host.len() <= 100
            && host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && port.parse::<u16>().is_ok_and(|p| p > 0)
    });
    if ok {
        Ok(())
    } else {
        Err(err("invalid_upstream", "The PHP address is not name:port."))
    }
}

/// One site, as far as its web server configuration goes.
#[derive(Debug, Clone)]
pub struct Site<'a> {
    /// The domain the file is named for.
    pub domain: &'a Fqdn,
    /// Other names that show the same site.
    pub aliases: &'a [Fqdn],
    /// Where its files are.
    pub docroot: &'a Docroot,
    /// The PHP service, `name:port`.
    pub php_upstream: &'a str,
}

/// Makes the configuration of one site from a template.
///
/// # Errors
/// [`VhostError`]: a bad template, too many names or a bad PHP address.
pub fn render(template: &str, site: &Site<'_>) -> Result<String, VhostError> {
    check_template(template)?;
    check_upstream(site.php_upstream)?;
    let mut names: Vec<&str> = vec![site.domain.as_str()];
    for alias in site.aliases {
        if !names.contains(&alias.as_str()) {
            names.push(alias.as_str());
        }
    }
    if names.len() > MAX_SERVER_NAMES {
        return Err(err("too_many_names", "A site has too many names."));
    }
    let docroot = if site.docroot.as_str().is_empty() {
        WEB_ROOT.to_owned()
    } else {
        format!("{WEB_ROOT}/{}", site.docroot.as_str())
    };
    Ok(render_with(
        template,
        &Values {
            domain: site.domain.as_str().to_owned(),
            server_names: names.join(" "),
            docroot,
            php_upstream: site.php_upstream.to_owned(),
        },
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fqdn(s: &str) -> Fqdn {
        Fqdn::parse(s).unwrap()
    }

    #[test]
    fn the_default_template_is_valid_and_makes_the_expected_file() {
        check_template(DEFAULT_NGINX_TEMPLATE).unwrap();
        let domain = fqdn("example.com");
        let aliases = [fqdn("www.example.com"), fqdn("example.net")];
        let docroot = Docroot::parse("public_html/shop").unwrap();
        let text = render(
            DEFAULT_NGINX_TEMPLATE,
            &Site {
                domain: &domain,
                aliases: &aliases,
                docroot: &docroot,
                php_upstream: "dh-alice-php:9000",
            },
        )
        .unwrap();
        // The whole file, so a change of the template shows up in review.
        assert_eq!(
            text,
            r"# Managed by DHPanel for example.com. Changes made here are overwritten.
server {
    listen 8080;
    server_name example.com www.example.com example.net;
    root /var/www/html/public_html/shop;
    index index.php index.html index.htm;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        try_files $uri =404;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_pass dh-alice-php:9000;
    }

    location ~ /\.(?!well-known) {
        deny all;
    }
}
"
        );
    }

    #[test]
    fn the_web_directory_itself_and_repeated_names() {
        let domain = fqdn("example.com");
        let aliases = [
            fqdn("example.com"),
            fqdn("a.example.com"),
            fqdn("a.example.com"),
        ];
        let text = render(
            DEFAULT_NGINX_TEMPLATE,
            &Site {
                domain: &domain,
                aliases: &aliases,
                docroot: &Docroot::default(),
                php_upstream: "dh-alice-php:9000",
            },
        )
        .unwrap();
        assert!(
            text.contains("server_name example.com a.example.com;"),
            "{text}"
        );
        assert!(text.contains("root /var/www/html;"), "{text}");
    }

    #[test]
    fn a_template_that_cannot_make_a_site_is_refused() {
        let ok = "server { server_name {{server_names}}; root {{docroot}}; }";
        check_template(ok).unwrap();
        let cases = [
            ("", "template_empty"),
            ("server { root {{docroot}}; }", "missing_variable"),
            (
                "server { server_name {{server_names}}; }",
                "missing_variable",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}}; x {{nope}}; }",
                "unknown_variable",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}};",
                "template_braces",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot",
                "template_variable",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}}; alias /etc; }",
                "forbidden_directive",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}}; include /etc/passwd; }",
                "forbidden_directive",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}}; content_by_lua_block { } }",
                "forbidden_directive",
            ),
            (
                "load_module modules/x.so; server { server_name {{server_names}}; root {{docroot}}; }",
                "forbidden_directive",
            ),
            (
                "server { server_name {{server_names}}; root {{docroot}}; \u{0} }",
                "template_characters",
            ),
        ];
        for (body, code) in cases {
            assert_eq!(check_template(body).unwrap_err().code, code, "{body}");
        }
        let big = format!("{ok}{}", "#".repeat(MAX_TEMPLATE_BYTES));
        assert_eq!(check_template(&big).unwrap_err().code, "template_too_large");
        // A commented-out directive is not a directive.
        check_template("server { server_name {{server_names}}; root {{docroot}}; # alias /x;\n}")
            .unwrap();
    }

    #[test]
    fn a_value_cannot_bring_a_directive_along() {
        let domain = fqdn("example.com");
        for bad in [
            "dh-a-php",
            "dh-a-php:0",
            "dh-a-php:9000; alias /etc",
            "DH:9000",
            ":9000",
            "a b:1",
        ] {
            let e = render(
                DEFAULT_NGINX_TEMPLATE,
                &Site {
                    domain: &domain,
                    aliases: &[],
                    docroot: &Docroot::default(),
                    php_upstream: bad,
                },
            )
            .unwrap_err();
            assert_eq!(e.code, "invalid_upstream", "{bad}");
        }
        let many: Vec<Fqdn> = (0..MAX_SERVER_NAMES)
            .map(|i| fqdn(&format!("a{i}.example.com")))
            .collect();
        let e = render(
            DEFAULT_NGINX_TEMPLATE,
            &Site {
                domain: &domain,
                aliases: &many,
                docroot: &Docroot::default(),
                php_upstream: "dh-a-php:9000",
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "too_many_names");
    }
}
