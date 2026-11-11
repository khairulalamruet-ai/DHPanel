//! Client IP resolution behind trusted reverse proxies.
//!
//! The TCP peer is the client unless it is listed as a trusted proxy (e.g. the
//! local Caddy). Only then is `X-Forwarded-For` consulted, walking from the
//! right (the hop closest to us) and skipping further trusted proxies; the
//! first untrusted address is the client. Entries left of it are
//! client-supplied and therefore ignored, so a client cannot spoof its IP by
//! sending its own `X-Forwarded-For`.

use std::net::IpAddr;

use axum::http::HeaderMap;

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
