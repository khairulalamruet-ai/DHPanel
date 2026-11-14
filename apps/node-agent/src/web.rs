//! The web server on a node: Caddy, driven through its admin API (T05.14).
//!
//! The control plane sends **one tenant's sites at a time** (`set_site_routes`):
//! names, where the tenant's web container listens, whether the site is
//! suspended, and its redirects. The agent
//!
//! 1. checks them again (names, upstream, redirects, limits),
//! 2. keeps them in a file of its own (`<state>/routes/<tenant>.json`),
//! 3. compiles **all** tenants' files into one Caddy configuration and loads it.
//!
//! The files are the agent's memory: whenever Caddy has lost the configuration
//! (it restarted without its autosave, someone reset it) the next reconcile pass
//! loads it again, and a restart of the agent does the same. Only a hostname that
//! the control plane assigned to one tenant is routed to that tenant's own
//! containers (`dh-<tenant>-...`), so one tenant's route can never point at
//! another's.
//!
//! In what Caddy sends back to visitors there is no placeholder (`{...}`): the
//! values come from the control plane and Caddy would expand them.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use dhpanel_domain::{Fqdn, SourcePath, Target, TenantUsername};
use dhpanel_node_protocol::{
    AgentError, CertificateInfo, ErrorCode, MAX_CERTIFICATES_PER_REPORT, MAX_HOSTS_PER_TENANT,
    MAX_REDIRECTS_PER_TENANT, MAX_SITES_PER_TENANT, Operation, Output, SiteRoute,
};
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// The name of the server inside Caddy's configuration.
const SERVER: &str = "dhpanel";
/// Time allowed for one call to Caddy.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How to reach and configure Caddy.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Caddy's admin API, `http://127.0.0.1:2019`.
    pub admin: String,
    /// Where the web server listens for visitors (`:80`, `:443`).
    pub listen: Vec<String>,
    /// Let Caddy fetch certificates for the names it serves.
    pub automatic_https: bool,
    /// Caddy's data directory (where it keeps its certificates), so they can be
    /// listed. `DHPANEL_AGENT_CADDY_DATA`; without it no certificates are reported.
    pub data_dir: Option<PathBuf>,
}

impl Settings {
    /// From the environment: `DHPANEL_AGENT_CADDY_ADMIN` turns the web server
    /// on; `DHPANEL_AGENT_CADDY_LISTEN` (comma separated, default `:80,:443`) and
    /// `DHPANEL_AGENT_CADDY_AUTO_HTTPS` (`false` to turn it off, for tests) tune it.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let admin = std::env::var("DHPANEL_AGENT_CADDY_ADMIN").ok()?;
        let listen = std::env::var("DHPANEL_AGENT_CADDY_LISTEN")
            .unwrap_or_else(|_| ":80,:443".to_owned())
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        let automatic_https = std::env::var("DHPANEL_AGENT_CADDY_AUTO_HTTPS")
            .map_or(true, |v| !matches!(v.as_str(), "false" | "0" | "no"));
        Some(Self {
            admin: admin.trim_end_matches('/').to_owned(),
            listen,
            automatic_https,
            data_dir: std::env::var_os("DHPANEL_AGENT_CADDY_DATA").map(PathBuf::from),
        })
    }
}

/// Everything the route operations need.
pub struct WebContext {
    settings: Settings,
    dir: PathBuf,
    agent: ureq::Agent,
    /// One change at a time: files and Caddy move together.
    busy: Mutex<()>,
}

impl WebContext {
    /// A context keeping its files in `<state_dir>/routes`.
    #[must_use]
    pub fn new(settings: Settings, state_dir: &Path) -> Arc<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .build()
            .into();
        Arc::new(Self {
            settings,
            dir: state_dir.join("routes"),
            agent,
            busy: Mutex::new(()),
        })
    }
}

// --------------------------------------------------------------- the checks

fn invalid(message: impl Into<String>) -> AgentError {
    AgentError::new(ErrorCode::InvalidArguments, message.into())
}

/// Checks one tenant's sites.
fn validate(username: &TenantUsername, routes: &[SiteRoute]) -> Result<(), AgentError> {
    if routes.len() > MAX_SITES_PER_TENANT {
        return Err(invalid("too many sites for one tenant"));
    }
    let mut hosts = BTreeSet::new();
    let mut redirects = 0usize;
    let prefix = format!("dh-{username}-");
    for site in routes {
        if site.hosts.is_empty() || site.hosts.len() > 100 {
            return Err(invalid("a site needs 1 to 100 names"));
        }
        if site.hsts && !site.force_https {
            return Err(invalid("hsts needs force_https"));
        }
        for host in &site.hosts {
            let canonical = Fqdn::parse(host).map_err(|_| invalid("a name is not valid"))?;
            if canonical.as_str() != host {
                return Err(invalid("names must be lower-case ASCII"));
            }
            if !hosts.insert(host.clone()) {
                return Err(invalid("a name is listed twice"));
            }
        }
        if let Some(upstream) = &site.upstream {
            let ok = upstream.split_once(':').is_some_and(|(name, port)| {
                name.starts_with(&prefix)
                    && name.len() <= 100
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && port.parse::<u16>().is_ok_and(|p| p > 0)
                    && !port.starts_with('+')
            });
            if !ok {
                return Err(invalid(
                    "the upstream must be one of the tenant's own containers, `dh-<tenant>-<name>:<port>`",
                ));
            }
        }
        let mut paths = BTreeSet::new();
        for redirect in &site.redirects {
            SourcePath::parse(&redirect.path)
                .map_err(|_| invalid("a redirect path is not valid"))?;
            Target::parse(&redirect.target)
                .map_err(|_| invalid("a redirect target is not valid"))?;
            if !matches!(redirect.code, 301 | 302 | 307 | 308) {
                return Err(invalid("a redirect code must be 301, 302, 307 or 308"));
            }
            if !paths.insert(redirect.path.clone()) {
                return Err(invalid("a redirect path is listed twice"));
            }
            redirects += 1;
        }
    }
    if hosts.len() > MAX_HOSTS_PER_TENANT || redirects > MAX_REDIRECTS_PER_TENANT {
        return Err(invalid("too many names or redirects for one tenant"));
    }
    Ok(())
}

// ------------------------------------------------------------ the compiling

const SUSPENDED_PAGE: &str = "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Site suspended</title></head><body style=\"font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#f5f6f8;color:#1d2330\"><main style=\"text-align:center;padding:2rem\"><h1>This site is suspended</h1><p>The owner of this address has been asked to contact their hosting provider.</p></main></body></html>";
const NOT_FOUND_PAGE: &str = "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Not found</title></head><body style=\"font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#f5f6f8;color:#1d2330\"><main style=\"text-align:center;padding:2rem\"><h1>Nothing here</h1><p>No site is set up for this address.</p></main></body></html>";

fn page(status: u16, body: &str, extra: &[(&str, &str)]) -> Value {
    let mut headers = serde_json::Map::new();
    headers.insert("Content-Type".into(), json!(["text/html; charset=utf-8"]));
    headers.insert("Cache-Control".into(), json!(["no-store"]));
    for (k, v) in extra {
        headers.insert((*k).to_owned(), json!([v]));
    }
    json!({"handler": "static_response", "status_code": status, "headers": headers, "body": body})
}

/// One site as a Caddy route.
fn site_route(site: &SiteRoute) -> Value {
    let mut inner: Vec<Value> = Vec::new();
    if site.force_https {
        // Visitors on plain HTTP go to the same address over HTTPS. The
        // placeholders are Caddy's own and fixed here: nothing the control plane
        // supplies is ever read as one.
        inner.push(json!({
            "match": [{"protocol": "http"}],
            "handle": [{
                "handler": "static_response",
                "status_code": 308,
                "headers": {"Location": ["https://{http.request.host}{http.request.uri}"]}
            }],
            "terminal": true
        }));
    }
    if site.hsts {
        // Not terminal: the header is added to whatever answers next.
        inner.push(json!({
            "match": [{"protocol": "https"}],
            "handle": [{
                "handler": "headers",
                "response": {"set": {"Strict-Transport-Security": ["max-age=31536000"]}}
            }]
        }));
    }
    inner.extend(site.redirects.iter().map(|r| {
        json!({
            "match": [{"path": [r.path]}],
            "handle": [{
                "handler": "static_response",
                "status_code": r.code,
                "headers": {"Location": [r.target]}
            }],
            "terminal": true
        })
    }));
    inner.push(match &site.upstream {
        Some(upstream) => json!({
            "handle": [{"handler": "reverse_proxy", "upstreams": [{"dial": upstream}]}]
        }),
        None => json!({
            "handle": [page(503, SUSPENDED_PAGE, &[("Retry-After", "3600")])]
        }),
    });
    json!({
        "match": [{"host": site.hosts}],
        "handle": [{"handler": "subroute", "routes": inner}],
        "terminal": true
    })
}

/// The whole Caddy configuration for every tenant's sites. Tenants and sites
/// come in a fixed order, so the same input gives the same output.
fn compile(all: &BTreeMap<String, Vec<SiteRoute>>, settings: &Settings) -> Value {
    let mut routes: Vec<Value> = all.values().flatten().map(site_route).collect();
    // Anything else: a plain "no such site", never another tenant's page.
    routes.push(json!({"handle": [page(404, NOT_FOUND_PAGE, &[])]}));
    let mut server = json!({"listen": settings.listen, "routes": routes});
    server["automatic_https"] = if settings.automatic_https {
        // The sites that want HTTPS say so themselves (`force_https`); Caddy's own
        // blanket redirect would make that choice for everyone.
        json!({"disable_redirects": true})
    } else {
        json!({"disable": true})
    };
    json!({"apps": {"http": {"servers": {SERVER: server}}}})
}

// ----------------------------------------------------------- files on disk

fn file_of(dir: &Path, username: &str) -> PathBuf {
    dir.join(format!("{username}.json"))
}

fn save(dir: &Path, username: &str, routes: &[SiteRoute]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = file_of(dir, username);
    if routes.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let tmp = dir.join(format!(".{username}.json.tmp"));
    std::fs::write(
        &tmp,
        serde_json::to_vec(routes).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(tmp, path)
}

/// Every tenant's saved sites. A file that cannot be read is skipped (and named
/// on stderr) rather than taking everybody's sites down with it.
fn load_all(dir: &Path) -> BTreeMap<String, Vec<SiteRoute>> {
    let mut all = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return all;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(user) = name.strip_suffix(".json") else {
            continue;
        };
        let Ok(username) = TenantUsername::parse(user) else {
            continue;
        };
        match std::fs::read(entry.path())
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<Vec<SiteRoute>>(&b).map_err(|e| e.to_string()))
        {
            Ok(routes) if validate(&username, &routes).is_ok() => {
                all.insert(user.to_owned(), routes);
            }
            Ok(_) | Err(_) => eprintln!("dhpanel-agent: ignoring unusable routes file {name}"),
        }
    }
    all
}

// --------------------------------------------------------------- Caddy calls

impl WebContext {
    /// One call to Caddy's admin API on the blocking pool: `(status, body text)`.
    async fn call(
        self: &Arc<Self>,
        method: &'static str,
        path: &'static str,
        body: Option<Value>,
    ) -> Result<(u16, String), String> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            let url = format!("{}{path}", this.settings.admin);
            let mut response = match (method, &body) {
                ("GET", _) => this.agent.get(&url).call(),
                ("POST", Some(b)) => this.agent.post(&url).send_json(b),
                ("PUT", Some(b)) => this.agent.put(&url).send_json(b),
                _ => return Err("unsupported call".to_owned()),
            }
            .map_err(|e| e.to_string())?;
            let status = response.status().as_u16();
            let text = response
                .body_mut()
                .with_config()
                .limit(32 << 20)
                .read_to_string()
                .map_err(|e| e.to_string())?;
            Ok((status, text))
        })
        .await
        .map_err(|_| "the call was cancelled".to_owned())?
    }

    /// What Caddy runs now for our server, if anything.
    async fn live(self: &Arc<Self>) -> Result<Option<Value>, String> {
        let (status, text) = self.call("GET", "/config/", None).await?;
        if status != 200 {
            return Err(format!("Caddy answered {status}"));
        }
        let config: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(config
            .pointer(&format!("/apps/http/servers/{SERVER}"))
            .cloned())
    }

    /// Makes Caddy run `server` as our server, leaving the rest of its
    /// configuration (its admin settings, anything else the machine's owner put
    /// there) as it is: `PUT` creates it, `POST` replaces it.
    async fn load(self: &Arc<Self>, server: Value, exists: bool) -> Result<(), String> {
        let method = if exists { "POST" } else { "PUT" };
        let (status, text) = self
            .call(method, "/config/apps/http/servers/dhpanel", Some(server))
            .await?;
        if status == 200 {
            Ok(())
        } else {
            let reason: String = text.chars().filter(|c| !c.is_control()).take(300).collect();
            Err(format!(
                "Caddy refused the configuration ({status}): {reason}"
            ))
        }
    }

    /// Compiles the saved sites and loads them if Caddy does not already run
    /// exactly that. Returns whether it had to load.
    ///
    /// # Errors
    /// Caddy could not be reached or refused the configuration.
    pub async fn reconcile(self: &Arc<Self>) -> Result<bool, String> {
        let _guard = self.busy.lock().await;
        self.reconcile_locked().await
    }

    async fn reconcile_locked(self: &Arc<Self>) -> Result<bool, String> {
        let config = compile(&load_all(&self.dir), &self.settings);
        let wanted = config
            .pointer(&format!("/apps/http/servers/{SERVER}"))
            .cloned()
            .ok_or_else(|| "no server to load".to_owned())?;
        let live = self.live().await?;
        if live.as_ref() == Some(&wanted) {
            return Ok(false);
        }
        self.load(wanted, live.is_some()).await?;
        Ok(true)
    }

    /// Checks the sites again every `every`, loading them when Caddy has lost
    /// them. The first pass runs at once, so a restart of the agent (or of the
    /// machine) brings the sites back.
    pub async fn keep_in_sync(self: Arc<Self>, every: Duration) {
        let mut tick = tokio::time::interval(every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            match self.reconcile().await {
                Ok(true) => eprintln!("dhpanel-agent: the web server's routes were put back"),
                Ok(false) => {}
                Err(err) => eprintln!("dhpanel-agent: web server: {err}"),
            }
        }
    }
}

/// `set_site_routes`.
///
/// # Errors
/// [`AgentError`]: bad arguments or names that already belong to another tenant
/// (not retryable); the tenant's routes are kept and applied later when Caddy
/// cannot be reached (the answer says `applied: false`).
pub async fn set_site_routes(
    context: Arc<WebContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::SetSiteRoutes { username, routes } = op else {
        return Err(crate::ops::wrong_operation());
    };
    let username = TenantUsername::parse(&username).map_err(|e| invalid(e.to_string()))?;
    validate(&username, &routes)?;
    let _guard = context.busy.lock().await;
    // A name belongs to one tenant.
    let others = load_all(&context.dir);
    for (other, sites) in &others {
        if other == username.as_str() {
            continue;
        }
        let taken: BTreeSet<&String> = sites.iter().flat_map(|s| &s.hosts).collect();
        if routes
            .iter()
            .flat_map(|s| &s.hosts)
            .any(|h| taken.contains(h))
        {
            return Err(invalid("a name already belongs to another tenant"));
        }
    }
    save(&context.dir, username.as_str(), &routes).map_err(|e| {
        AgentError::new(ErrorCode::Internal, format!("cannot keep the routes: {e}")).retryable()
    })?;
    let applied = context.reconcile_locked().await;
    let hosts = routes.iter().map(|s| s.hosts.len()).sum();
    match applied {
        Ok(_) => Ok(Output::SiteRoutesSet {
            sites: routes.len(),
            hosts,
            applied: true,
        }),
        // Refused: our own data is wrong, saying "done" would be a lie.
        Err(message) if message.starts_with("Caddy refused") => {
            Err(AgentError::new(ErrorCode::Internal, message))
        }
        // Not reachable: kept, and put in place by the next pass.
        Err(_) => Ok(Output::SiteRoutesSet {
            sites: routes.len(),
            hosts,
            applied: false,
        }),
    }
}

// ------------------------------------------------------------- certificates

/// The common name (or, failing that, the whole name) of an issuer.
fn issuer_name(cert: &x509_parser::certificate::X509Certificate<'_>) -> String {
    let issuer = cert.issuer();
    let name = issuer
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map_or_else(|| issuer.to_string(), str::to_owned);
    name.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// Reads one certificate file (the first certificate in it).
fn read_certificate(path: &Path, host: &str) -> Option<CertificateInfo> {
    let bytes = std::fs::read(path).ok()?;
    let (_, pem) = x509_parser::pem::parse_x509_pem(&bytes).ok()?;
    let cert = pem.parse_x509().ok()?;
    Some(CertificateInfo {
        host: host.to_owned(),
        issuer: issuer_name(&cert),
        not_before: cert.validity().not_before.timestamp(),
        not_after: cert.validity().not_after.timestamp(),
    })
}

/// The certificates Caddy keeps under `<data>/certificates/<issuer>/<host>/<host>.crt`.
/// A name that has certificates from several issuers is listed once, with the one
/// that lasts longest; wildcard certificates and anything unreadable are skipped.
fn scan_certificates(data_dir: &Path) -> Vec<CertificateInfo> {
    let mut best: BTreeMap<String, CertificateInfo> = BTreeMap::new();
    let Ok(issuers) = std::fs::read_dir(data_dir.join("certificates")) else {
        return Vec::new();
    };
    for issuer in issuers.flatten() {
        let Ok(hosts) = std::fs::read_dir(issuer.path()) else {
            continue;
        };
        for entry in hosts.flatten() {
            let host = entry.file_name().to_string_lossy().into_owned();
            // Only names this panel could have: plain host names.
            if Fqdn::parse(&host).map_or(true, |f| f.as_str() != host) {
                continue;
            }
            let Some(info) = read_certificate(&entry.path().join(format!("{host}.crt")), &host)
            else {
                continue;
            };
            match best.get(&host) {
                Some(have) if have.not_after >= info.not_after => {}
                _ => {
                    best.insert(host, info);
                }
            }
        }
    }
    best.into_values()
        .take(MAX_CERTIFICATES_PER_REPORT)
        .collect()
}

/// `report_certificates`.
///
/// # Errors
/// [`AgentError`] `unsupported_operation` when no data directory is configured.
pub async fn report_certificates(
    context: Arc<WebContext>,
    op: Operation,
) -> Result<Output, AgentError> {
    let Operation::ReportCertificates = op else {
        return Err(crate::ops::wrong_operation());
    };
    let Some(dir) = context.settings.data_dir.clone() else {
        return Err(AgentError::new(
            ErrorCode::UnsupportedOperation,
            "this node does not tell where its web server keeps certificates",
        ));
    };
    let items = tokio::task::spawn_blocking(move || scan_certificates(&dir))
        .await
        .map_err(|_| AgentError::new(ErrorCode::Internal, "the scan was cancelled").retryable())?;
    Ok(Output::Certificates { items })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use dhpanel_node_protocol::RouteRedirect;

    use super::*;

    fn site(hosts: &[&str], upstream: Option<&str>) -> SiteRoute {
        SiteRoute {
            hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
            upstream: upstream.map(str::to_owned),
            redirects: vec![],
            force_https: false,
            hsts: false,
        }
    }

    fn alice() -> TenantUsername {
        TenantUsername::parse("alice").unwrap()
    }

    fn settings() -> Settings {
        Settings {
            admin: "http://127.0.0.1:2019".into(),
            listen: vec![":80".into(), ":443".into()],
            automatic_https: true,
            data_dir: None,
        }
    }

    #[test]
    fn good_sites_pass_and_bad_ones_say_no() {
        let ok = vec![
            site(
                &["example.com", "www.example.com"],
                Some("dh-alice-web:8080"),
            ),
            site(&["paused.org"], None),
        ];
        assert!(validate(&alice(), &ok).is_ok());
        for bad in [
            site(&[], Some("dh-alice-web:8080")),
            site(&["Example.com"], Some("dh-alice-web:8080")),
            site(&["com"], Some("dh-alice-web:8080")),
            site(&["a b.com"], None),
            site(&["a.com", "a.com"], None),
            // The upstream must be the tenant's own.
            site(&["a.com"], Some("dh-bobby-web:8080")),
            site(&["a.com"], Some("dh-alice-web")),
            site(&["a.com"], Some("dh-alice-web:0")),
            site(&["a.com"], Some("dh-alice-web:99999")),
            site(&["a.com"], Some("dh-alice-web:+80")),
            site(&["a.com"], Some("evil.example.org:80")),
            site(&["a.com"], Some("169.254.169.254:80")),
            site(&["a.com"], Some("dh-alice-WEB:80")),
            site(&["a.com"], Some("dh-alice-web/x:80")),
        ] {
            assert!(
                validate(&alice(), std::slice::from_ref(&bad)).is_err(),
                "{bad:?}"
            );
        }
        // The same name in two sites is refused too.
        assert!(validate(&alice(), &[site(&["a.com"], None), site(&["a.com"], None)]).is_err());
        let redirect = |path: &str, target: &str, code: u16| {
            let mut s = site(&["a.com"], Some("dh-alice-web:8080"));
            s.redirects.push(RouteRedirect {
                path: path.into(),
                target: target.into(),
                code,
            });
            s
        };
        assert!(validate(&alice(), &[redirect("/old", "https://b.org/", 301)]).is_ok());
        for (path, target, code) in [
            ("old", "/x", 301),
            ("/a*", "/x", 301),
            ("/{env.SECRET}", "/x", 301),
            ("/a", "/{env.SECRET}", 301),
            ("/a", "javascript:alert(1)", 301),
            ("/a", "//evil.example", 301),
            ("/a", "/x", 303),
        ] {
            assert!(
                validate(&alice(), &[redirect(path, target, code)]).is_err(),
                "{path} {target} {code}"
            );
        }
        let mut twice = redirect("/a", "/x", 301);
        twice.redirects.push(twice.redirects[0].clone());
        assert!(validate(&alice(), &[twice]).is_err());
    }

    #[test]
    fn the_configuration_has_a_route_per_site_and_a_plain_404_last() {
        let mut all = BTreeMap::new();
        let mut a = site(
            &["example.com", "www.example.com"],
            Some("dh-alice-web:8080"),
        );
        a.redirects.push(RouteRedirect {
            path: "/old".into(),
            target: "https://example.org/new".into(),
            code: 301,
        });
        all.insert("alice".to_owned(), vec![a, site(&["paused.org"], None)]);
        all.insert(
            "bobby".to_owned(),
            vec![site(&["bobs.net"], Some("dh-bobby-web:8080"))],
        );
        let config = compile(&all, &settings());
        let server = &config["apps"]["http"]["servers"]["dhpanel"];
        assert_eq!(server["listen"], json!([":80", ":443"]));
        let routes = server["routes"].as_array().unwrap();
        assert_eq!(routes.len(), 4, "three sites and the last-resort 404");
        assert_eq!(
            routes[0]["match"][0]["host"],
            json!(["example.com", "www.example.com"])
        );
        let inner = &routes[0]["handle"][0]["routes"];
        assert_eq!(inner[0]["match"][0]["path"], json!(["/old"]));
        assert_eq!(inner[0]["handle"][0]["status_code"], 301);
        assert_eq!(
            inner[0]["handle"][0]["headers"]["Location"],
            json!(["https://example.org/new"])
        );
        assert_eq!(
            inner[1]["handle"][0]["upstreams"][0]["dial"],
            "dh-alice-web:8080"
        );
        assert_eq!(
            routes[1]["handle"][0]["routes"][0]["handle"][0]["status_code"], 503,
            "suspended"
        );
        assert_eq!(
            routes[2]["handle"][0]["routes"][0]["handle"][0]["upstreams"][0]["dial"],
            "dh-bobby-web:8080"
        );
        assert!(routes[3].get("match").is_none());
        assert_eq!(routes[3]["handle"][0]["status_code"], 404);
        // Nothing the control plane supplies can be read as a placeholder: the
        // fixed pages have none and the supplied values were checked for braces.
        let fixed = format!("{SUSPENDED_PAGE}{NOT_FOUND_PAGE}");
        assert!(!fixed.contains('{') && !fixed.contains('}'));
        // The same input gives the same output.
        assert_eq!(compile(&all, &settings()), compile(&all, &settings()));
        // Automatic HTTPS can be turned off.
        let off = compile(
            &all,
            &Settings {
                automatic_https: false,
                ..settings()
            },
        );
        assert_eq!(
            off["apps"]["http"]["servers"]["dhpanel"]["automatic_https"]["disable"],
            true
        );
        // Caddy's blanket redirect is off: each site decides for itself.
        assert_eq!(
            config["apps"]["http"]["servers"]["dhpanel"]["automatic_https"],
            json!({"disable_redirects": true})
        );
    }

    #[test]
    fn force_https_and_hsts_become_a_redirect_and_a_header_in_front_of_the_site() {
        let mut secure = site(&["example.com"], Some("dh-alice-web:8080"));
        secure.force_https = true;
        secure.hsts = true;
        secure.redirects.push(RouteRedirect {
            path: "/old".into(),
            target: "/new".into(),
            code: 301,
        });
        let mut only_redirect = site(&["plain.org"], Some("dh-alice-web:8080"));
        only_redirect.force_https = true;
        let all = BTreeMap::from([(
            "alice".to_owned(),
            vec![secure, only_redirect, site(&["http-only.net"], None)],
        )]);
        let config = compile(&all, &settings());
        let routes = config["apps"]["http"]["servers"]["dhpanel"]["routes"]
            .as_array()
            .unwrap();
        let inner = |i: usize| routes[i]["handle"][0]["routes"].as_array().unwrap().clone();
        // Order: the HTTP redirect, the header on HTTPS answers, the site's redirects, the site.
        let first = inner(0);
        assert_eq!(first.len(), 4);
        assert_eq!(first[0]["match"][0]["protocol"], "http");
        assert_eq!(first[0]["handle"][0]["status_code"], 308);
        assert_eq!(
            first[0]["handle"][0]["headers"]["Location"],
            json!(["https://{http.request.host}{http.request.uri}"])
        );
        assert_eq!(first[0]["terminal"], true);
        assert_eq!(first[1]["match"][0]["protocol"], "https");
        assert_eq!(
            first[1]["handle"][0]["response"]["set"]["Strict-Transport-Security"],
            json!(["max-age=31536000"])
        );
        assert!(
            first[1].get("terminal").is_none(),
            "the header is added to what answers next"
        );
        assert_eq!(first[2]["match"][0]["path"], json!(["/old"]));
        assert_eq!(first[3]["handle"][0]["handler"], "reverse_proxy");
        // Redirect without the header; and a site with neither has neither.
        assert_eq!(inner(1).len(), 2);
        assert!(
            inner(1)
                .iter()
                .all(|r| r["match"][0]["protocol"] != "https")
        );
        assert_eq!(inner(2).len(), 1);
        // HSTS without the redirect is refused.
        let mut lone = site(&["a.com"], None);
        lone.hsts = true;
        assert!(validate(&alice(), &[lone]).is_err());
    }

    fn write_cert(dir: &Path, issuer_dir: &str, host: &str, days: i64, issuer_cn: &str) {
        let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params
            .distinguished_name
            .push(rcgen::DnType::CommonName, issuer_cn);
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let ca = rcgen::Issuer::new(ca_params, ca_key);
        let mut params = rcgen::CertificateParams::new(vec![host.to_owned()]).unwrap();
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(days);
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.signed_by(&key, &ca).unwrap();
        let folder = dir.join("certificates").join(issuer_dir).join(host);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{host}.crt")), cert.pem()).unwrap();
        // The private key sits beside it; it must never be read or reported.
        std::fs::write(folder.join(format!("{host}.key")), key.serialize_pem()).unwrap();
    }

    #[test]
    fn certificates_are_listed_from_caddys_storage_without_touching_keys() {
        let dir = std::env::temp_dir().join(format!("dh-certs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_cert(
            &dir,
            "acme-v02.api.letsencrypt.org-directory",
            "example.com",
            60,
            "R11",
        );
        write_cert(
            &dir,
            "acme-v02.api.letsencrypt.org-directory",
            "www.example.com",
            5,
            "R11",
        );
        // The same name from another issuer: the one that lasts longer is reported.
        write_cert(
            &dir,
            "acme.zerossl.com-v2-DV90",
            "example.com",
            90,
            "ZeroSSL ECC Domain Secure Site CA",
        );
        // Things that are not ours or not usable.
        write_cert(&dir, "local", "localhost", 30, "Caddy Local Authority");
        std::fs::create_dir_all(dir.join("certificates/local/wildcard_.example.org")).unwrap();
        std::fs::create_dir_all(dir.join("certificates/local/broken.org")).unwrap();
        std::fs::write(
            dir.join("certificates/local/broken.org/broken.org.crt"),
            b"not a certificate",
        )
        .unwrap();
        let found = scan_certificates(&dir);
        let hosts: Vec<&str> = found.iter().map(|c| c.host.as_str()).collect();
        assert_eq!(hosts, ["example.com", "www.example.com"], "{found:?}");
        let main = &found[0];
        assert_eq!(main.issuer, "ZeroSSL ECC Domain Secure Site CA");
        let days = (main.not_after - main.not_before) / 86_400;
        assert!((90..=92).contains(&days), "{days}");
        assert_eq!(found[1].issuer, "R11");
        // Nothing there at all is an empty list, not an error.
        assert!(scan_certificates(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn routes_are_kept_in_files_and_unusable_files_are_ignored() {
        let dir = std::env::temp_dir().join(format!("dh-routes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let routes = vec![site(&["example.com"], Some("dh-alice-web:8080"))];
        save(&dir, "alice", &routes).unwrap();
        save(&dir, "bobby", &[site(&["bobs.net"], None)]).unwrap();
        assert_eq!(load_all(&dir)["alice"], routes);
        assert_eq!(load_all(&dir).len(), 2);
        // Emptying removes the file; removing twice is fine.
        save(&dir, "bobby", &[]).unwrap();
        save(&dir, "bobby", &[]).unwrap();
        assert_eq!(load_all(&dir).keys().collect::<Vec<_>>(), ["alice"]);
        // Garbage, a file that is not valid for its tenant, and strangers are skipped.
        std::fs::write(dir.join("carol.json"), b"not json").unwrap();
        std::fs::write(
            dir.join("dave.json"),
            serde_json::to_vec(&[site(&["d.com"], Some("dh-alice-web:8080"))]).unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("README"), b"x").unwrap();
        std::fs::write(dir.join("../escape.json"), b"[]").ok();
        assert_eq!(load_all(&dir).keys().collect::<Vec<_>>(), ["alice"]);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(std::env::temp_dir().join("escape.json"));
    }
}
