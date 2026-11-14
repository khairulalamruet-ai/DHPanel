//! The web server operations against a real Caddy, with a real site behind it.
//! Ignored by default; needs a Caddy whose admin API and HTTP port the script
//! publishes (see deploy/dev/web-live.sh):
//!   DHPANEL_TEST_CADDY_ADMIN=http://127.0.0.1:12019
//!   DHPANEL_TEST_CADDY_URL=http://127.0.0.1:18080
//!   DHPANEL_TEST_CADDY_CONTAINER=<name of the caddy container>
//! and a container `dh-alice-web` (port 8080) on a network Caddy is also on.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{process::Command, time::Duration};

use dhpanel_node_agent::web::{Settings, WebContext, set_site_routes};
use dhpanel_node_protocol::{ErrorCode, Operation, Output, RouteRedirect, SiteRoute};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// One HTTP request to Caddy with a chosen `Host`: `(status, location, body)`.
async fn get(url: &str, host: &str, path: &str) -> (u16, Option<String>, String) {
    let (url, host, path) = (url.to_owned(), host.to_owned(), path.to_owned());
    tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let mut r = agent
            .get(&format!("{url}{path}"))
            .header("Host", &host)
            .call()
            .unwrap();
        let status = r.status().as_u16();
        let location = r
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        (
            status,
            location,
            r.body_mut().read_to_string().unwrap_or_default(),
        )
    })
    .await
    .unwrap()
}

fn site(hosts: &[&str], upstream: Option<&str>, redirects: Vec<RouteRedirect>) -> SiteRoute {
    SiteRoute {
        hosts: hosts.iter().map(|h| (*h).to_owned()).collect(),
        upstream: upstream.map(str::to_owned),
        redirects,
        force_https: false,
        hsts: false,
    }
}

fn set(username: &str, routes: Vec<SiteRoute>) -> Operation {
    Operation::SetSiteRoutes {
        username: username.into(),
        routes,
    }
}

#[tokio::test]
#[ignore = "needs a Caddy and a site behind it (see the file header)"]
async fn sites_are_served_suspended_redirected_and_put_back_after_caddy_forgets_them() {
    let (Some(admin), Some(url), Some(container)) = (
        env("DHPANEL_TEST_CADDY_ADMIN"),
        env("DHPANEL_TEST_CADDY_URL"),
        env("DHPANEL_TEST_CADDY_CONTAINER"),
    ) else {
        eprintln!("skipped: no Caddy (see the script)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("dh-web-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let web = WebContext::new(
        Settings {
            admin,
            listen: vec![":18080".into()],
            automatic_https: false,
            data_dir: None,
        },
        &dir,
    );

    // Nothing yet: any name gets the plain "nothing here" page.
    web.reconcile().await.unwrap();
    let (status, _, body) = get(&url, "example.com", "/").await;
    assert_eq!(status, 404);
    assert!(body.contains("No site is set up"), "{body}");

    // A site: the tenant's web container answers for both names.
    let out = set_site_routes(
        web.clone(),
        set(
            "alice",
            vec![site(
                &["example.com", "www.example.com"],
                Some("dh-alice-web:8080"),
                vec![
                    RouteRedirect {
                        path: "/old".into(),
                        target: "/new.html".into(),
                        code: 301,
                    },
                    RouteRedirect {
                        path: "/away".into(),
                        target: "https://example.org/there".into(),
                        code: 302,
                    },
                ],
            )],
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        out,
        Output::SiteRoutesSet {
            sites: 1,
            hosts: 2,
            applied: true
        }
    );
    for host in ["example.com", "www.example.com", "WWW.Example.COM"] {
        let (status, _, body) = get(&url, host, "/").await;
        assert_eq!((status, body.trim()), (200, "hello from alice"), "{host}");
    }
    // Redirects are answered before the site is asked, with the right code and Location.
    let (status, location, _) = get(&url, "example.com", "/old").await;
    assert_eq!((status, location.as_deref()), (301, Some("/new.html")));
    let (status, location, _) = get(&url, "example.com", "/away").await;
    assert_eq!(
        (status, location.as_deref()),
        (302, Some("https://example.org/there"))
    );
    // Only the exact path.
    assert_eq!(
        get(&url, "example.com", "/old/more").await.0,
        404,
        "the site has no such file"
    );
    // Other names still get nothing.
    assert_eq!(get(&url, "other.org", "/").await.0, 404);

    // Suspended: the page, not the files; and the right header.
    set_site_routes(
        web.clone(),
        set(
            "alice",
            vec![site(&["example.com", "www.example.com"], None, vec![])],
        ),
    )
    .await
    .unwrap();
    let (status, _, body) = get(&url, "example.com", "/").await;
    assert_eq!(status, 503);
    assert!(body.contains("suspended"), "{body}");
    // Resumed.
    set_site_routes(
        web.clone(),
        set(
            "alice",
            vec![site(
                &["example.com", "www.example.com"],
                Some("dh-alice-web:8080"),
                vec![],
            )],
        ),
    )
    .await
    .unwrap();
    assert_eq!(get(&url, "example.com", "/").await.0, 200);

    // Caddy forgets everything (restart without its saved configuration): the next
    // pass of the agent puts the sites back.
    let reset = Command::new("docker")
        .args([
            "exec",
            &container,
            "wget",
            "-q",
            "-O-",
            "--post-data",
            "",
            "--header=Content-Type: application/json",
            "--header=Origin: http://127.0.0.1:12019",
            "http://127.0.0.1:2019/stop",
        ])
        .output();
    let _ = reset;
    // Wait until a pass finds the routes gone and puts them back (`Ok(true)`): Caddy
    // is down for a moment while it restarts, then comes back with only its admin
    // settings.
    let mut put_back = false;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if web.reconcile().await == Ok(true) {
            put_back = true;
            break;
        }
    }
    assert!(
        put_back,
        "Caddy never lost the routes, or they were not put back"
    );
    assert_eq!(
        get(&url, "example.com", "/").await.0,
        200,
        "the site is back"
    );
    assert!(
        !web.reconcile().await.unwrap(),
        "nothing to do when everything is in place"
    );

    // A name belongs to one tenant; a route can only point at the tenant's own container.
    let err = set_site_routes(
        web.clone(),
        set(
            "bobby",
            vec![site(&["example.com"], Some("dh-bobby-web:8080"), vec![])],
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    let err = set_site_routes(
        web.clone(),
        set(
            "bobby",
            vec![site(&["bobs.net"], Some("dh-alice-web:8080"), vec![])],
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArguments);
    assert_eq!(
        get(&url, "example.com", "/").await.0,
        200,
        "refused changes change nothing"
    );

    // Removing a tenant's routes takes the sites down.
    let out = set_site_routes(web.clone(), set("alice", vec![]))
        .await
        .unwrap();
    assert_eq!(
        out,
        Output::SiteRoutesSet {
            sites: 0,
            hosts: 0,
            applied: true
        }
    );
    assert_eq!(get(&url, "example.com", "/").await.0, 404);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
#[ignore = "needs a Caddy (see the file header)"]
async fn routes_are_kept_when_caddy_cannot_be_reached_and_applied_later() {
    let dir = std::env::temp_dir().join(format!("dh-web-down-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let web = WebContext::new(
        Settings {
            admin: "http://127.0.0.1:9".into(),
            listen: vec![":18080".into()],
            automatic_https: false,
            data_dir: None,
        },
        &dir,
    );
    let out = set_site_routes(
        web.clone(),
        set(
            "alice",
            vec![site(&["example.com"], Some("dh-alice-web:8080"), vec![])],
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        out,
        Output::SiteRoutesSet {
            sites: 1,
            hosts: 1,
            applied: false
        },
        "kept, not applied"
    );
    assert!(dir.join("routes/alice.json").exists());
    assert!(web.reconcile().await.is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// One HTTPS request through `curl`, which can be told where a name lives and not to
/// trust Caddy's local CA: `(status, headers lower-cased, body)`.
fn https_get(port: &str, host: &str, path: &str) -> (u16, String, String) {
    let out = Command::new("curl")
        .args([
            "-sk",
            "--max-time",
            "10",
            "--resolve",
            &format!("{host}:{port}:127.0.0.1"),
            "-D",
            "-",
            &format!("https://{host}:{port}{path}"),
        ])
        .output()
        .expect("curl");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, head.to_ascii_lowercase(), body.to_owned())
}

#[tokio::test]
#[ignore = "needs a Caddy with HTTP and HTTPS ports (see the file header)"]
async fn force_https_redirects_plain_requests_and_hsts_is_sent_over_https_only() {
    let (Some(admin), Some(url), Some(tls_port)) = (
        env("DHPANEL_TEST_CADDY_ADMIN"),
        env("DHPANEL_TEST_CADDY_URL"),
        env("DHPANEL_TEST_CADDY_TLS_PORT"),
    ) else {
        eprintln!("skipped: no Caddy (see the script)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("dh-web-https-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let web = WebContext::new(
        Settings {
            admin,
            listen: vec![":18080".into(), ":18443".into()],
            automatic_https: true,
            data_dir: None,
        },
        &dir,
    );
    let secure = |hosts: &[&str], force: bool, hsts: bool| {
        let mut s = site(hosts, Some("dh-alice-web:8080"), vec![]);
        s.force_https = force;
        s.hsts = hsts;
        s
    };
    set_site_routes(
        web.clone(),
        set(
            "alice",
            vec![
                secure(&["forced.example.com"], true, true),
                secure(&["redirect-only.example.com"], true, false),
                secure(&["open.example.com"], false, false),
            ],
        ),
    )
    .await
    .unwrap();
    // Certificates are made when the first visitor arrives: give Caddy a moment.
    let mut first = (0, String::new(), String::new());
    for _ in 0..30 {
        first = https_get(&tls_port, "forced.example.com", "/");
        if first.0 == 200 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(first.0, 200, "{first:?}");
    assert_eq!(first.2.trim(), "hello from alice");
    // Over HTTPS the header is there, with a year.
    assert!(
        first
            .1
            .contains("strict-transport-security: max-age=31536000"),
        "{}",
        first.1
    );

    // Over plain HTTP the visitor is sent to the same address over HTTPS, method kept (308).
    let (status, location, _) = get(&url, "forced.example.com", "/some/page").await;
    assert_eq!(
        (status, location.as_deref()),
        (308, Some("https://forced.example.com/some/page"))
    );
    let (status, location, _) = get(&url, "redirect-only.example.com", "/x?y=1").await;
    assert_eq!(
        (status, location.as_deref()),
        (308, Some("https://redirect-only.example.com/x?y=1"))
    );
    // Redirect without HSTS: no header over HTTPS.
    let mut second = (0, String::new(), String::new());
    for _ in 0..30 {
        second = https_get(&tls_port, "redirect-only.example.com", "/");
        if second.0 == 200 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(second.0, 200, "{second:?}");
    assert!(
        !second.1.contains("strict-transport-security"),
        "{}",
        second.1
    );
    // A domain that did not ask is served over plain HTTP as before, never redirected.
    let (status, location, body) = get(&url, "open.example.com", "/").await;
    assert_eq!(
        (status, location, body.trim()),
        (200, None, "hello from alice")
    );

    // Turning it off takes effect on the next load.
    set_site_routes(
        web.clone(),
        set("alice", vec![secure(&["forced.example.com"], false, false)]),
    )
    .await
    .unwrap();
    assert_eq!(
        get(&url, "forced.example.com", "/").await.0,
        200,
        "no redirect any more"
    );
    let (status, headers, _) = https_get(&tls_port, "forced.example.com", "/");
    assert_eq!(status, 200);
    assert!(!headers.contains("strict-transport-security"), "{headers}");
    set_site_routes(web.clone(), set("alice", vec![]))
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
