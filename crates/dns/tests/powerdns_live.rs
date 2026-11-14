//! The PowerDNS provider against a real PowerDNS (deploy/dev/compose.yml).
//! Ignored by default. Set DHPANEL_TEST_PDNS_URL (for example http://127.0.0.1:8053)
//! and DHPANEL_TEST_PDNS_KEY, then run with `--include-ignored`; skips without them.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use dhpanel_dns::{
    DnsError, DnsProvider, ZoneSpec,
    powerdns::{Config, PowerDns},
};
use dhpanel_domain::DnsRecord;
use secrecy::SecretString;

fn server() -> Option<PowerDns> {
    let url = std::env::var("DHPANEL_TEST_PDNS_URL").ok()?;
    let key = std::env::var("DHPANEL_TEST_PDNS_KEY").ok()?;
    Some(PowerDns::new(&Config {
        url,
        api_key: SecretString::from(key),
        server_id: "localhost".into(),
    }))
}

fn rec(name: &str, t: &str, content: &str, ttl: u32, prio: Option<u32>) -> DnsRecord {
    DnsRecord::parse(name, t, content, Some(ttl), prio).unwrap()
}

#[tokio::test]
#[ignore = "needs a PowerDNS (see the file header)"]
async fn zones_and_every_kind_of_record_round_trip_through_a_real_server() {
    let Some(dns) = server() else {
        eprintln!("skipped: no PowerDNS configured");
        return;
    };
    let zone = format!("dhpanel-live-{}.test", std::process::id());
    let spec = ZoneSpec {
        name: zone.clone(),
        nameservers: vec![format!("ns1.{zone}."), format!("ns2.{zone}.")],
    };
    // Not there yet; then created; creating again is fine.
    assert_eq!(dns.list_records(&zone).await.unwrap(), None);
    dns.ensure_zone(&spec).await.unwrap();
    dns.ensure_zone(&spec).await.unwrap();
    assert_eq!(
        dns.list_records(&zone).await.unwrap(),
        Some(vec![]),
        "the server's own SOA and NS are not listed"
    );

    let long_txt = "v=DKIM1; k=rsa; p=".to_owned() + &"A".repeat(500);
    let mut records = vec![
        rec("@", "A", "192.0.2.1", 300, None),
        rec("@", "A", "192.0.2.2", 300, None),
        rec("@", "AAAA", "2001:db8::1", 300, None),
        rec("@", "MX", "mail.example.org.", 300, Some(10)),
        rec("@", "TXT", "v=spf1 a mx ~all", 300, None),
        rec("@", "CAA", "0 issue \"letsencrypt.org\"", 300, None),
        rec("www", "CNAME", "example.org.", 300, None),
        rec("_sip._tcp", "SRV", "5 5060 sip.example.org.", 300, Some(10)),
        rec("sub", "NS", "ns.other.net.", 300, None),
        rec("*.wild", "A", "192.0.2.9", 300, None),
        rec("_domainkey", "TXT", &long_txt, 300, None),
        rec("padded", "TXT", &"0123456789".repeat(60), 300, None),
    ];
    dns.replace_records(&zone, &records).await.unwrap();
    let mut got = dns.list_records(&zone).await.unwrap().unwrap();
    let key = |r: &DnsRecord| (r.name.clone(), r.rtype, r.content.clone());
    records.sort_by_key(key);
    got.sort_by_key(key);
    assert_eq!(got, records, "what was written is what comes back");

    // Replacing with less removes the rest; the server's own records stay.
    let fewer = vec![rec("@", "A", "192.0.2.7", 600, None)];
    dns.replace_records(&zone, &fewer).await.unwrap();
    assert_eq!(dns.list_records(&zone).await.unwrap().unwrap(), fewer);
    dns.replace_records(&zone, &fewer).await.unwrap();
    dns.replace_records(&zone, &[]).await.unwrap();
    assert_eq!(dns.list_records(&zone).await.unwrap().unwrap(), vec![]);

    // The server's refusal is not retryable: a CNAME next to another record.
    let clash = vec![
        rec("x", "A", "192.0.2.1", 300, None),
        rec("x", "CNAME", "a.example.org.", 300, None),
    ];
    let error = dns.replace_records(&zone, &clash).await.unwrap_err();
    assert!(matches!(error, DnsError::Rejected(_)), "{error:?}");
    assert!(!error.is_retryable());

    // Removing, and removing again.
    dns.delete_zone(&zone).await.unwrap();
    dns.delete_zone(&zone).await.unwrap();
    assert_eq!(dns.list_records(&zone).await.unwrap(), None);
    // A zone that is not there cannot take records.
    assert!(matches!(
        dns.replace_records(&zone, &fewer).await.unwrap_err(),
        DnsError::Rejected(_)
    ));
}

#[tokio::test]
#[ignore = "needs a PowerDNS (see the file header)"]
async fn a_wrong_key_is_refused_and_a_dead_address_is_unavailable() {
    let Some(good) = server() else {
        eprintln!("skipped: no PowerDNS configured");
        return;
    };
    let _ = good;
    let url = std::env::var("DHPANEL_TEST_PDNS_URL").unwrap();
    let wrong = PowerDns::new(&Config {
        url,
        api_key: SecretString::from("not-the-key".to_owned()),
        server_id: "localhost".into(),
    });
    let error = wrong.list_records("example.com").await.unwrap_err();
    assert!(matches!(error, DnsError::Rejected(_)), "{error:?}");
    let dead = PowerDns::new(&Config {
        url: "http://127.0.0.1:9".into(),
        api_key: SecretString::from("k".to_owned()),
        server_id: "localhost".into(),
    });
    let error = dead.list_records("example.com").await.unwrap_err();
    assert!(error.is_retryable(), "{error:?}");
}
