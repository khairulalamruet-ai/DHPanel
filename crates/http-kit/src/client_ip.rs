//! Client IP resolution behind trusted reverse proxies.
//!
//! The TCP peer is the client unless it is listed as a trusted proxy (e.g. the
//! local Caddy). Only then is `X-Forwarded-For` consulted, walking from the
//! right (the hop closest to us) and skipping further trusted proxies; the
//! first untrusted address is the client. Entries left of it are
//! client-supplied and therefore ignored, so a client cannot spoof its IP by
//! sending its own `X-Forwarded-For`.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ConnectInfo, Request, State, rejection::ExtensionRejection},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};

tokio::task_local! {
    static CLIENT_IP: IpAddr;
}

/// The current request's client address, if called inside the
/// [`client_ip_layer`] scope. Used by the audit log to record the actor's IP
/// without every handler passing it along.
#[must_use]
pub fn current() -> Option<IpAddr> {
    CLIENT_IP.try_with(|ip| *ip).ok()
}

/// Middleware: resolves the client address once ([`client_ip`]) and runs the
/// rest of the stack inside its scope, so [`current`] works in handlers.
/// Needs the server to provide `ConnectInfo<SocketAddr>`; without it the
/// request simply has no client address.
pub async fn client_ip_layer(
    State(trusted): State<Arc<[IpAddr]>>,
    // The extractor (not a raw extension lookup) so the test helper
    // `MockConnectInfo` works as well as a real server.
    connect: Result<ConnectInfo<SocketAddr>, ExtensionRejection>,
    request: Request,
    next: Next,
) -> Response {
    match connect {
        Ok(ConnectInfo(peer)) => {
            let ip = client_ip(peer.ip(), request.headers(), &trusted);
            CLIENT_IP.scope(ip, next.run(request)).await
        }
        Err(_) => next.run(request).await,
    }
}

/// Header appended to by reverse proxies.
pub const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// Resolves the client address.
///
/// * `peer` — the TCP peer address.
/// * `trusted` — proxies whose `X-Forwarded-For` is believed.
///
/// IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) are normalised to IPv4.
#[must_use]
pub fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpAddr]) -> IpAddr {
    let mut current = peer.to_canonical();
    if !trusted.contains(&current) {
        return current;
    }
    // All header lines in order, then all hops right to left.
    let lines: Vec<&str> = headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    for hop in lines.iter().rev().flat_map(|l| l.rsplit(',')) {
        let Ok(ip) = hop.trim().parse::<IpAddr>() else {
            // Garbage from an untrusted party: stop at the last address we trust.
            return current;
        };
        current = ip.to_canonical();
        if !trusted.contains(&current) {
            return current;
        }
    }
    current
}

/// The key under which per-address limits are counted: the address itself
/// for IPv4, the /64 network for IPv6 (one subscriber usually owns a whole /64,
/// so counting single IPv6 addresses would be trivially bypassed).
#[must_use]
pub fn rate_limit_key(ip: IpAddr) -> String {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn xff(values: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for v in values {
            h.append(X_FORWARDED_FOR, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    const PROXY: &str = "127.0.0.1";

    #[test]
    fn untrusted_peer_header_is_ignored() {
        let h = xff(&["1.2.3.4"]);
        assert_eq!(
            client_ip(ip("203.0.113.9"), &h, &[ip(PROXY)]),
            ip("203.0.113.9")
        );
        assert_eq!(client_ip(ip(PROXY), &h, &[]), ip(PROXY));
    }

    #[test]
    fn trusted_proxy_uses_rightmost_untrusted_hop() {
        // Client sent a spoofed header; Caddy appended the real peer.
        let h = xff(&["6.6.6.6, 198.51.100.7"]);
        assert_eq!(client_ip(ip(PROXY), &h, &[ip(PROXY)]), ip("198.51.100.7"));
    }

    #[test]
    fn chains_of_trusted_proxies_are_skipped() {
        let trusted = [ip(PROXY), ip("10.0.0.2")];
        let h = xff(&["6.6.6.6, 198.51.100.7", "10.0.0.2"]);
        assert_eq!(client_ip(ip(PROXY), &h, &trusted), ip("198.51.100.7"));
    }

    #[test]
    fn missing_or_garbage_header_falls_back_to_last_trusted_hop() {
        let trusted = [ip(PROXY)];
        assert_eq!(client_ip(ip(PROXY), &HeaderMap::new(), &trusted), ip(PROXY));
        let h = xff(&["not-an-ip"]);
        assert_eq!(client_ip(ip(PROXY), &h, &trusted), ip(PROXY));
    }

    #[test]
    fn mapped_ipv6_is_normalised() {
        assert_eq!(
            client_ip(ip("::ffff:192.0.2.1"), &HeaderMap::new(), &[]),
            ip("192.0.2.1")
        );
        let h = xff(&["::ffff:198.51.100.7"]);
        assert_eq!(
            client_ip(ip("::ffff:127.0.0.1"), &h, &[ip(PROXY)]),
            ip("198.51.100.7")
        );
    }

    #[tokio::test]
    async fn layer_exposes_the_resolved_client_ip_to_handlers() {
        use axum::{Router, body::Body, extract::connect_info::MockConnectInfo, routing::get};
        use tower::ServiceExt as _;

        async fn show() -> String {
            current().map_or_else(|| "none".to_owned(), |ip| ip.to_string())
        }
        let trusted: Arc<[IpAddr]> = Arc::from(vec![ip(PROXY)]);
        let build = |peer: &str| {
            Router::new()
                .route("/", get(show))
                .layer(axum::middleware::from_fn_with_state(
                    trusted.clone(),
                    client_ip_layer,
                ))
                .layer(MockConnectInfo(SocketAddr::new(ip(peer), 4000)))
        };
        let get_body = |app: Router, xff: Option<&'static str>| async move {
            let mut req = axum::http::Request::builder().uri("/");
            if let Some(v) = xff {
                req = req.header(X_FORWARDED_FOR, v);
            }
            let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
            String::from_utf8(bytes.to_vec()).unwrap()
        };
        // Behind the trusted proxy the forwarded address wins; a direct client cannot spoof it.
        assert_eq!(
            get_body(build(PROXY), Some("6.6.6.6, 198.51.100.7")).await,
            "198.51.100.7"
        );
        assert_eq!(
            get_body(build("203.0.113.9"), Some("6.6.6.6")).await,
            "203.0.113.9"
        );
        // Outside the layer there is no client address.
        assert_eq!(current(), None);
        // Without ConnectInfo the request just has no client address.
        let bare = Router::new()
            .route("/", get(show))
            .layer(axum::middleware::from_fn_with_state(
                trusted,
                client_ip_layer,
            ));
        assert_eq!(get_body(bare, None).await, "none");
    }

    #[test]
    fn rate_limit_keys_group_ipv6_by_64() {
        assert_eq!(rate_limit_key(ip("192.0.2.1")), "192.0.2.1");
        assert_eq!(
            rate_limit_key(ip("2001:db8:1:2:aaaa::1")),
            rate_limit_key(ip("2001:db8:1:2:bbbb::9"))
        );
        assert_eq!(rate_limit_key(ip("2001:db8:1:2::1")), "2001:db8:1:2::/64");
        assert_ne!(
            rate_limit_key(ip("2001:db8:1:2::1")),
            rate_limit_key(ip("2001:db8:1:3::1"))
        );
    }
}
