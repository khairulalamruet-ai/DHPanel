//! A tenant's web server configuration, made from the panel's tables (T06.04b).

use dhpanel_database::{domain_policies::DomainPolicy, routes::RoutedDomain};
use dhpanel_domain::{Docroot, Fqdn, vhost};
use dhpanel_node_protocol::MAX_HOSTS_PER_TENANT;
use uuid::Uuid;

/// The configuration of every site of a tenant: `(domain, file content)`, in the
/// order of the domains' names.
///
/// * only an `active` or `suspended` tenant has any (the containers exist);
/// * every domain that is not an alias is a site, answering also for its aliases;
/// * a domain whose docroot or name does not pass the checks is left out (the
///   others still get theirs), and so is what does not fit the node's limits.
#[must_use]
pub fn build(
    tenant_status: &str,
    template: &str,
    php_upstream: &str,
    domains: &[RoutedDomain],
    policies: &[(Uuid, DomainPolicy)],
) -> Vec<(String, String)> {
    if !matches!(tenant_status, "active" | "suspended") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for d in domains.iter().filter(|d| d.kind != "alias") {
        let Ok(domain) = Fqdn::parse(&d.fqdn) else {
            continue;
        };
        let Ok(docroot) = Docroot::parse(&d.docroot) else {
            continue;
        };
        let aliases: Vec<Fqdn> = domains
            .iter()
            .filter(|a| a.kind == "alias" && a.parent_id == Some(d.id) && a.status != "suspended")
            .filter_map(|a| Fqdn::parse(&a.fqdn).ok())
            .collect();
        // A policy that does not pass the checks (it was checked when it was
        // saved) is left out rather than losing the whole site.
        let policy = policies
            .iter()
            .find(|(id, _)| *id == d.id)
            .and_then(|(_, p)| {
                vhost::Policy::parse(
                    p.rate_limit_per_second,
                    p.headers
                        .iter()
                        .map(|h| vhost::Header {
                            name: h.name.clone(),
                            value: h.value.clone(),
                        })
                        .collect(),
                )
                .ok()
            })
            .unwrap_or_default();
        let site = vhost::Site {
            domain: &domain,
            aliases: &aliases,
            docroot: &docroot,
            php_upstream,
            policy: &policy,
        };
        if let Ok(config) = vhost::render(template, &site) {
            out.push((domain.as_str().to_owned(), config));
        }
    }
    out.sort();
    out.truncate(MAX_HOSTS_PER_TENANT);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use dhpanel_domain::vhost::DEFAULT_NGINX_TEMPLATE;
    use uuid::Uuid;

    use super::*;

    fn dom(n: u128, fqdn: &str, kind: &str, parent: Option<u128>, docroot: &str) -> RoutedDomain {
        RoutedDomain {
            id: Uuid::from_u128(n),
            fqdn: fqdn.into(),
            kind: kind.into(),
            parent_id: parent.map(Uuid::from_u128),
            status: "active".into(),
            force_https: false,
            hsts: false,
            docroot: docroot.into(),
        }
    }

    const PHP: &str = "dh-alice-php:9000";

    #[test]
    fn every_site_gets_a_file_and_aliases_share_their_domains() {
        let domains = vec![
            dom(1, "example.com", "primary", None, "public_html"),
            dom(2, "www.example.com", "alias", Some(1), ""),
            dom(3, "shop.org", "addon", None, "shop"),
            dom(4, "blog.example.com", "sub", Some(1), "blog"),
        ];
        let files = build("active", DEFAULT_NGINX_TEMPLATE, PHP, &domains, &[]);
        let names: Vec<&str> = files.iter().map(|(d, _)| d.as_str()).collect();
        assert_eq!(names, ["blog.example.com", "example.com", "shop.org"]);
        let main = &files[1].1;
        assert!(
            main.contains("server_name example.com www.example.com;"),
            "{main}"
        );
        assert!(main.contains("root /var/www/html/public_html;"));
        assert!(main.contains("fastcgi_pass dh-alice-php:9000;"));
        assert!(files[2].1.contains("root /var/www/html/shop;"));
    }

    #[test]
    fn a_tenant_without_containers_has_no_files_and_a_bad_domain_is_skipped() {
        let domains = vec![dom(1, "example.com", "primary", None, "")];
        for status in ["provisioning", "failed", "deleting"] {
            assert!(build(status, DEFAULT_NGINX_TEMPLATE, PHP, &domains, &[]).is_empty());
        }
        assert_eq!(
            build("suspended", DEFAULT_NGINX_TEMPLATE, PHP, &domains, &[]).len(),
            1
        );
        let bad = vec![
            dom(1, "example.com", "primary", None, "../etc"),
            dom(2, "ok.org", "primary", None, ""),
        ];
        let files = build("active", DEFAULT_NGINX_TEMPLATE, PHP, &bad, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "ok.org");
    }

    #[test]
    fn a_suspended_alias_stops_answering() {
        let mut alias = dom(2, "www.example.com", "alias", Some(1), "");
        alias.status = "suspended".into();
        let domains = vec![dom(1, "example.com", "primary", None, ""), alias];
        let files = build("active", DEFAULT_NGINX_TEMPLATE, PHP, &domains, &[]);
        assert!(files[0].1.contains("server_name example.com;"));
    }

    #[test]
    fn a_domains_policy_goes_into_its_file_only() {
        use dhpanel_database::domain_policies::StoredHeader;
        let domains = vec![
            dom(1, "example.com", "primary", None, ""),
            dom(2, "other.org", "addon", None, ""),
        ];
        let policy = DomainPolicy {
            rate_limit_per_second: Some(5),
            headers: vec![StoredHeader {
                name: "X-Test".into(),
                value: "yes".into(),
            }],
        };
        let files = build(
            "active",
            DEFAULT_NGINX_TEMPLATE,
            PHP,
            &domains,
            &[(Uuid::from_u128(1), policy)],
        );
        assert!(files[0].1.contains("add_header X-Test \"yes\" always;"));
        assert!(files[0].1.contains("rate=5r/s"));
        assert!(!files[1].1.contains("limit_req"));
        // A policy that cannot pass the checks is left out, the site stays.
        let broken = DomainPolicy {
            rate_limit_per_second: Some(0),
            headers: vec![],
        };
        let files = build(
            "active",
            DEFAULT_NGINX_TEMPLATE,
            PHP,
            &domains,
            &[(Uuid::from_u128(1), broken)],
        );
        assert_eq!(files.len(), 2);
        assert!(!files[0].1.contains("limit_req"));
    }
}
