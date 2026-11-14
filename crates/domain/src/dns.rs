//! DNS records of a hosted domain (T05.08, used again by T05.10).
//!
//! What may be written into a zone, checked once for the API, the zone
//! templates and the importer. Names are **relative to the zone** (`@` is the
//! zone itself, `www` is `www.<zone>`); targets of `CNAME`, `MX`, `NS` and `SRV`
//! are absolute host names, written with the final dot (`mail.example.com.`;
//! one without it is read as absolute and gets the dot). The zone's `SOA` and
//! its own `NS` records belong to the DNS server and cannot be written here.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Why a record was rejected. `field` says which part to fix.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RecordError {
    /// Which input is wrong: `name`, `type`, `content`, `ttl` or `priority`.
    pub field: &'static str,
    /// Stable machine code.
    pub code: &'static str,
    /// What is wrong, for a person.
    pub message: String,
}

fn err(field: &'static str, code: &'static str, message: impl Into<String>) -> RecordError {
    RecordError {
        field,
        code,
        message: message.into(),
    }
}

/// The record types a zone may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RecordType {
    /// IPv4 address.
    A,
    /// IPv6 address.
    Aaaa,
    /// Alias to another name.
    Cname,
    /// Mail exchanger.
    Mx,
    /// Text.
    Txt,
    /// Service location.
    Srv,
    /// Certificate authority authorisation.
    Caa,
    /// Delegation of a sub-zone.
    Ns,
}

impl RecordType {
    /// The usual written form (`A`, `AAAA`, ...).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::Aaaa => "AAAA",
            Self::Cname => "CNAME",
            Self::Mx => "MX",
            Self::Txt => "TXT",
            Self::Srv => "SRV",
            Self::Caa => "CAA",
            Self::Ns => "NS",
        }
    }

    /// Reads the written form (any case).
    ///
    /// # Errors
    /// An unknown or unsupported type (`SOA` and `PTR` are not writable here).
    pub fn parse(text: &str) -> Result<Self, RecordError> {
        Ok(match text.trim().to_ascii_uppercase().as_str() {
            "A" => Self::A,
            "AAAA" => Self::Aaaa,
            "CNAME" => Self::Cname,
            "MX" => Self::Mx,
            "TXT" => Self::Txt,
            "SRV" => Self::Srv,
            "CAA" => Self::Caa,
            "NS" => Self::Ns,
            _ => {
                return Err(err(
                    "type",
                    "unsupported_record_type",
                    "Supported types: A, AAAA, CNAME, MX, TXT, SRV, CAA, NS.",
                ));
            }
        })
    }

    /// Whether the record carries a priority (`MX`, `SRV`).
    #[must_use]
    pub fn has_priority(self) -> bool {
        matches!(self, Self::Mx | Self::Srv)
    }
}

/// Smallest and largest time to live, in seconds.
pub const MIN_TTL: u32 = 60;
/// See [`MIN_TTL`].
pub const MAX_TTL: u32 = 604_800;
/// The time to live when none is given.
pub const DEFAULT_TTL: u32 = 3600;
/// Longest TXT content.
pub const MAX_TXT: usize = 2000;

/// A record, checked and normalised.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Record {
    /// Relative to the zone: `@`, `www`, `_dmarc`, `*.shop`.
    pub name: String,
    /// The type.
    pub rtype: RecordType,
    /// The data (for `MX`, only the target; the priority is separate).
    pub content: String,
    /// Time to live in seconds.
    pub ttl: u32,
    /// Priority of an `MX` or `SRV`.
    pub priority: Option<u16>,
}

fn label_ok(label: &str, allow_underscore: bool) -> bool {
    (1..=63).contains(&label.len())
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.bytes().all(|b| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || b == b'-'
                || (allow_underscore && b == b'_')
        })
}

/// Checks a name relative to a zone and returns it lower-case.
///
/// # Errors
/// What is wrong with it.
pub fn check_name(input: &str) -> Result<String, RecordError> {
    let name = input.trim().to_ascii_lowercase();
    if name == "@" {
        return Ok(name);
    }
    if name.is_empty() || name.len() > 200 || name.ends_with('.') {
        return Err(err(
            "name",
            "invalid_record_name",
            "Use @ for the zone itself, or a name such as www or mail.shop (no trailing dot).",
        ));
    }
    let labels: Vec<&str> = name.split('.').collect();
    for (i, label) in labels.iter().enumerate() {
        // `*` stands for any name, only as the first label; `_` is for service names.
        let ok = (i == 0 && *label == "*") || label_ok(label, true);
        if !ok {
            return Err(err(
                "name",
                "invalid_record_name",
                "Names use letters, digits and hyphens (and _ for service names such as _dmarc); `*` may start a name.",
            ));
        }
    }
    Ok(name)
}

/// An absolute host name, with the final dot added when missing.
fn absolute_host(field: &'static str, input: &str) -> Result<String, RecordError> {
    let bad = || {
        err(
            field,
            "invalid_record_target",
            "The target must be a host name such as mail.example.com.",
        )
    };
    let text = input.trim().to_ascii_lowercase();
    let host = text.strip_suffix('.').unwrap_or(&text);
    if host.is_empty() || host.len() > 253 || host == "@" {
        return Err(bad());
    }
    let ascii = idna::domain_to_ascii_strict(host).map_err(|_| bad())?;
    if ascii.split('.').all(|l| label_ok(l, true)) {
        Ok(format!("{ascii}."))
    } else {
        Err(bad())
    }
}

fn printable(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

impl Record {
    /// Checks a record as a person wrote it and normalises it.
    ///
    /// `ttl` defaults to [`DEFAULT_TTL`]. `priority` is required for `MX` and
    /// `SRV` and refused for the others.
    ///
    /// # Errors
    /// The first problem found.
    pub fn parse(
        name: &str,
        rtype: &str,
        content: &str,
        ttl: Option<u32>,
        priority: Option<u32>,
    ) -> Result<Self, RecordError> {
        let rtype = RecordType::parse(rtype)?;
        let name = check_name(name)?;
        let ttl = ttl.unwrap_or(DEFAULT_TTL);
        if !(MIN_TTL..=MAX_TTL).contains(&ttl) {
            return Err(err(
                "ttl",
                "ttl_out_of_range",
                format!("The TTL must be between {MIN_TTL} and {MAX_TTL} seconds."),
            ));
        }
        let priority = match (rtype.has_priority(), priority) {
            (true, Some(p)) => Some(u16::try_from(p).map_err(|_| {
                err(
                    "priority",
                    "priority_out_of_range",
                    "The priority must be 0 to 65535.",
                )
            })?),
            (true, None) => {
                return Err(err(
                    "priority",
                    "priority_required",
                    "This type needs a priority.",
                ));
            }
            (false, Some(_)) => {
                return Err(err(
                    "priority",
                    "priority_not_allowed",
                    "This type has no priority.",
                ));
            }
            (false, None) => None,
        };
        let content = content.trim();
        let content = match rtype {
            RecordType::A => content
                .parse::<Ipv4Addr>()
                .map(|a| a.to_string())
                .map_err(|_| {
                    err(
                        "content",
                        "invalid_ipv4",
                        "Expected an IPv4 address such as 192.0.2.10.",
                    )
                })?,
            RecordType::Aaaa => {
                content
                    .parse::<Ipv6Addr>()
                    .map(|a| a.to_string())
                    .map_err(|_| {
                        err(
                            "content",
                            "invalid_ipv6",
                            "Expected an IPv6 address such as 2001:db8::10.",
                        )
                    })?
            }
            RecordType::Cname | RecordType::Mx => absolute_host("content", content)?,
            RecordType::Ns => {
                if name == "@" {
                    return Err(err(
                        "name",
                        "apex_ns_reserved",
                        "The name servers of the zone itself are set by the server; NS records are for delegating a sub-zone.",
                    ));
                }
                absolute_host("content", content)?
            }
            RecordType::Txt => {
                // Printable ASCII without quotes and backslashes: what SPF, DKIM,
                // DMARC and verification records are made of. PowerDNS cannot
                // store the other characters reliably through its API.
                let allowed = |b: u8| (0x20..=0x7e).contains(&b) && b != b'"' && b != b'\\';
                if content.is_empty() || content.len() > MAX_TXT || !content.bytes().all(allowed) {
                    return Err(err(
                        "content",
                        "invalid_txt",
                        format!(
                            "Text of 1 to {MAX_TXT} printable ASCII characters, without quotes (\") or backslashes."
                        ),
                    ));
                }
                content.to_owned()
            }
            RecordType::Srv => srv_content(content)?,
            RecordType::Caa => caa_content(content)?,
        };
        if rtype == RecordType::Cname && name == "@" {
            return Err(err(
                "name",
                "cname_at_apex",
                "A CNAME cannot be at the zone itself (it would hide the zone's other records).",
            ));
        }
        Ok(Self {
            name,
            rtype,
            content,
            ttl,
            priority,
        })
    }
}

/// `weight port target` of an SRV record.
fn srv_content(content: &str) -> Result<String, RecordError> {
    let bad = || {
        err(
            "content",
            "invalid_srv",
            "Expected `weight port target`, for example `10 5060 sip.example.com.`.",
        )
    };
    let parts: Vec<&str> = content.split_whitespace().collect();
    let [weight, port, target] = parts.as_slice() else {
        return Err(bad());
    };
    let weight: u16 = weight.parse().map_err(|_| bad())?;
    let port: u16 = port.parse().map_err(|_| bad())?;
    // A target of "." means the service is not offered.
    let target = if *target == "." {
        ".".to_owned()
    } else {
        absolute_host("content", target)?
    };
    Ok(format!("{weight} {port} {target}"))
}

/// `flags tag "value"` of a CAA record.
fn caa_content(content: &str) -> Result<String, RecordError> {
    let bad = || {
        err(
            "content",
            "invalid_caa",
            "Expected `flags tag \"value\"`, for example `0 issue \"letsencrypt.org\"`.",
        )
    };
    let mut parts = content.splitn(3, char::is_whitespace);
    let (flags, tag, value) = (
        parts.next().ok_or_else(bad)?,
        parts.next().ok_or_else(bad)?,
        parts.next().ok_or_else(bad)?.trim(),
    );
    let flags: u8 = flags.parse().map_err(|_| bad())?;
    if flags != 0 && flags != 128 {
        return Err(bad());
    }
    let tag = tag.to_ascii_lowercase();
    if !matches!(tag.as_str(), "issue" | "issuewild" | "iodef") {
        return Err(bad());
    }
    let unquoted = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    if unquoted.len() > 255 || !printable(unquoted) || unquoted.contains('"') {
        return Err(bad());
    }
    Ok(format!("{flags} {tag} \"{unquoted}\""))
}

/// Whether `new` may join `existing` (the records of the same zone and name).
/// A CNAME is alone at its name; a name that is a CNAME takes nothing else; the
/// very same record twice is refused.
///
/// # Errors
/// The conflict.
pub fn check_conflicts<'a>(
    existing: impl IntoIterator<Item = &'a Record>,
    new: &Record,
) -> Result<(), RecordError> {
    for other in existing {
        if other.name != new.name {
            continue;
        }
        if other.rtype == new.rtype
            && other.content == new.content
            && other.priority == new.priority
        {
            return Err(err(
                "content",
                "duplicate_record",
                "This record exists already.",
            ));
        }
        let cname_clash = (new.rtype == RecordType::Cname) != (other.rtype == RecordType::Cname);
        if cname_clash {
            return Err(err(
                "type",
                "cname_conflict",
                "A name with a CNAME record cannot have other records, and a CNAME cannot join other records.",
            ));
        }
        if new.rtype == RecordType::Cname && other.rtype == RecordType::Cname {
            return Err(err(
                "type",
                "cname_conflict",
                "A name can have only one CNAME.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    fn rec(name: &str, t: &str, content: &str) -> Result<Record, RecordError> {
        Record::parse(
            name,
            t,
            content,
            None,
            if matches!(t, "MX" | "SRV") {
                Some(10)
            } else {
                None
            },
        )
    }

    #[test]
    fn good_records_are_accepted_and_normalised() {
        assert_eq!(rec("@", "a", "192.0.2.10").unwrap().content, "192.0.2.10");
        assert_eq!(rec("WWW", "A", " 192.0.2.10 ").unwrap().name, "www");
        assert_eq!(
            rec("@", "AAAA", "2001:DB8:0:0:0:0:0:10").unwrap().content,
            "2001:db8::10"
        );
        assert_eq!(
            rec("www", "CNAME", "Example.COM").unwrap().content,
            "example.com."
        );
        assert_eq!(
            rec("@", "MX", "mail.example.com").unwrap().content,
            "mail.example.com."
        );
        assert_eq!(rec("*.shop", "A", "192.0.2.1").unwrap().name, "*.shop");
        assert_eq!(
            rec("_dmarc", "TXT", "v=DMARC1; p=none").unwrap().content,
            "v=DMARC1; p=none"
        );
        assert_eq!(
            rec("_sip._tcp", "SRV", "5 5060 sip.example.com")
                .unwrap()
                .content,
            "5 5060 sip.example.com."
        );
        assert_eq!(
            rec("@", "CAA", "0 ISSUE letsencrypt.org").unwrap().content,
            "0 issue \"letsencrypt.org\""
        );
        assert_eq!(
            rec("@", "CAA", "0 iodef \"mailto:sec@example.com\"")
                .unwrap()
                .content,
            "0 iodef \"mailto:sec@example.com\""
        );
        assert_eq!(
            rec("sub", "NS", "ns1.other.net.").unwrap().content,
            "ns1.other.net."
        );
        assert!(
            rec("bücher", "CNAME", "bücher.de").is_err(),
            "names inside the zone are ASCII"
        );
        assert_eq!(
            rec("www", "CNAME", "bücher.de").unwrap().content,
            "xn--bcher-kva.de."
        );
        assert_eq!(rec("@", "A", "192.0.2.10").unwrap().ttl, DEFAULT_TTL);
    }

    #[test]
    fn bad_records_say_which_part_is_wrong() {
        let field = |name: &str, t: &str, c: &str| rec(name, t, c).unwrap_err().field;
        assert_eq!(field("@", "A", "256.1.1.1"), "content");
        assert_eq!(field("@", "A", "2001:db8::1"), "content");
        assert_eq!(field("@", "AAAA", "192.0.2.1"), "content");
        assert_eq!(field("www", "CNAME", "not a host"), "content");
        assert_eq!(field("www", "CNAME", "@"), "content");
        assert_eq!(field("@", "CNAME", "example.com"), "name");
        assert_eq!(field("@", "NS", "ns.example.com"), "name");
        assert_eq!(field("@", "TXT", ""), "content");
        assert_eq!(field("@", "TXT", "line\nbreak"), "content");
        assert_eq!(field("@", "TXT", "say \"hi\""), "content", "no quotes");
        assert_eq!(
            field("@", "TXT", "back\\slash"),
            "content",
            "no backslashes"
        );
        assert_eq!(field("@", "TXT", "café"), "content", "ASCII only");
        assert_eq!(field("@", "TXT", &"x".repeat(2001)), "content");
        assert!(rec("@", "TXT", &"x".repeat(2000)).is_ok());
        assert_eq!(field("@", "SRV", "5 5060"), "content");
        assert_eq!(field("@", "SRV", "x 5060 a.example.com"), "content");
        assert_eq!(field("@", "CAA", "0 badtag x.org"), "content");
        assert_eq!(field("@", "CAA", "7 issue x.org"), "content");
        assert_eq!(field("@", "SOA", "x"), "type");
        assert_eq!(field("@", "PTR", "x"), "type");
        for bad in [
            "",
            "a..b",
            ".a",
            "a.",
            "-a",
            "a-",
            "a b",
            "a/b",
            "*x",
            "a.*",
            &"a".repeat(64),
            &format!("{}.b", "a.".repeat(120)),
        ] {
            assert_eq!(field(bad, "A", "192.0.2.1"), "name", "{bad:?}");
        }
    }

    #[test]
    fn ttl_and_priority_follow_the_type() {
        let p = |t: &str, ttl: Option<u32>, prio: Option<u32>| {
            Record::parse(
                "@",
                t,
                if t == "MX" {
                    "m.example.com"
                } else {
                    "192.0.2.1"
                },
                ttl,
                prio,
            )
        };
        assert!(p("A", Some(60), None).is_ok());
        assert!(p("A", Some(604_800), None).is_ok());
        assert_eq!(p("A", Some(59), None).unwrap_err().code, "ttl_out_of_range");
        assert_eq!(
            p("A", Some(604_801), None).unwrap_err().code,
            "ttl_out_of_range"
        );
        assert_eq!(
            p("A", None, Some(10)).unwrap_err().code,
            "priority_not_allowed"
        );
        assert_eq!(p("MX", None, None).unwrap_err().code, "priority_required");
        assert_eq!(
            p("MX", None, Some(70_000)).unwrap_err().code,
            "priority_out_of_range"
        );
        assert_eq!(p("MX", None, Some(0)).unwrap().priority, Some(0));
    }

    #[test]
    fn a_cname_is_alone_at_its_name_and_duplicates_are_refused() {
        let a = rec("www", "A", "192.0.2.1").unwrap();
        let a2 = rec("www", "A", "192.0.2.2").unwrap();
        let cname = rec("www", "CNAME", "example.com").unwrap();
        let other = rec("blog", "CNAME", "example.com").unwrap();
        assert!(
            check_conflicts([&a], &a2).is_ok(),
            "two A records may share a name"
        );
        assert_eq!(
            check_conflicts([&a], &a).unwrap_err().code,
            "duplicate_record"
        );
        assert_eq!(
            check_conflicts([&a], &cname).unwrap_err().code,
            "cname_conflict"
        );
        assert_eq!(
            check_conflicts([&cname], &a).unwrap_err().code,
            "cname_conflict"
        );
        assert_eq!(
            check_conflicts([&cname], &rec("www", "CNAME", "other.org").unwrap())
                .unwrap_err()
                .code,
            "cname_conflict"
        );
        assert!(
            check_conflicts([&cname], &other).is_ok(),
            "another name is free"
        );
    }
}

// ---------------------------------------------------------------- templates

/// One line of a zone template, before its variables are filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRecord {
    /// Relative name (`@`, `www`, ...).
    pub name: String,
    /// The type, as written.
    pub rtype: String,
    /// The data; may use `{{domain}}`, `{{ipv4}}` and `{{ipv6}}`.
    pub content: String,
    /// Time to live; the default when absent.
    pub ttl: Option<u32>,
    /// Priority of an `MX` or `SRV`.
    pub priority: Option<u32>,
}

/// What a template may refer to for the zone it makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variables {
    /// The zone's name without the final dot, `example.com`.
    pub domain: String,
    /// The server's IPv4 address, when it has one.
    pub ipv4: Option<Ipv4Addr>,
    /// The server's IPv6 address, when it has one.
    pub ipv6: Option<Ipv6Addr>,
}

/// Most lines in a template.
pub const MAX_TEMPLATE_RECORDS: usize = 100;

/// Fills a template in for one zone.
///
/// A line that names a variable the server has no value for (`{{ipv6}}` on a
/// server without IPv6) is left out; every other line must be a valid record,
/// and the lines must not conflict with each other.
///
/// # Errors
/// The first bad line (its position is in the message).
pub fn render(lines: &[TemplateRecord], vars: &Variables) -> Result<Vec<Record>, RecordError> {
    if lines.len() > MAX_TEMPLATE_RECORDS {
        return Err(err(
            "records",
            "too_many_records",
            "A template has at most 100 records.",
        ));
    }
    let mut out: Vec<Record> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut content = line.content.replace("{{domain}}", &vars.domain);
        let mut missing = false;
        for (token, value) in [
            ("{{ipv4}}", vars.ipv4.map(|a| a.to_string())),
            ("{{ipv6}}", vars.ipv6.map(|a| a.to_string())),
        ] {
            if content.contains(token) {
                match value {
                    Some(v) => content = content.replace(token, &v),
                    None => missing = true,
                }
            }
        }
        if missing {
            continue;
        }
        if content.contains("{{") {
            return Err(err(
                "records",
                "unknown_variable",
                format!(
                    "Record {}: only {{{{domain}}}}, {{{{ipv4}}}} and {{{{ipv6}}}} may be used.",
                    i + 1
                ),
            ));
        }
        let record = Record::parse(&line.name, &line.rtype, &content, line.ttl, line.priority)
            .map_err(|e| {
                err(
                    "records",
                    e.code,
                    format!("Record {}: {}", i + 1, e.message),
                )
            })?;
        check_conflicts(out.iter(), &record).map_err(|e| {
            err(
                "records",
                e.code,
                format!("Record {}: {}", i + 1, e.message),
            )
        })?;
        out.push(record);
    }
    Ok(out)
}

/// Checks a template as an administrator saves it: filled in for a server with
/// both address kinds it must be valid and free of conflicts.
///
/// # Errors
/// The first bad line.
pub fn check_template(lines: &[TemplateRecord]) -> Result<(), RecordError> {
    render(
        lines,
        &Variables {
            domain: "example.com".to_owned(),
            ipv4: Some(Ipv4Addr::new(192, 0, 2, 1)),
            ipv6: Some(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        },
    )
    .map(|_| ())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod template_tests {
    use super::*;

    fn line(name: &str, t: &str, content: &str, priority: Option<u32>) -> TemplateRecord {
        TemplateRecord {
            name: name.into(),
            rtype: t.into(),
            content: content.into(),
            ttl: None,
            priority,
        }
    }

    fn default_template() -> Vec<TemplateRecord> {
        vec![
            line("@", "A", "{{ipv4}}", None),
            line("@", "AAAA", "{{ipv6}}", None),
            line("www", "CNAME", "{{domain}}.", None),
            line("@", "MX", "{{domain}}.", Some(10)),
            line("@", "TXT", "v=spf1 a mx ~all", None),
        ]
    }

    #[test]
    fn a_template_is_filled_in_for_the_zone() {
        let vars = Variables {
            domain: "example.com".into(),
            ipv4: Some("192.0.2.7".parse().unwrap()),
            ipv6: Some("2001:db8::7".parse().unwrap()),
        };
        let records = render(&default_template(), &vars).unwrap();
        let shown: Vec<String> = records
            .iter()
            .map(|r| format!("{} {} {}", r.name, r.rtype.as_str(), r.content))
            .collect();
        assert_eq!(
            shown,
            [
                "@ A 192.0.2.7",
                "@ AAAA 2001:db8::7",
                "www CNAME example.com.",
                "@ MX example.com.",
                "@ TXT v=spf1 a mx ~all"
            ]
        );
        assert_eq!(records[3].priority, Some(10));
    }

    #[test]
    fn lines_whose_address_is_unknown_are_left_out() {
        let vars = Variables {
            domain: "example.com".into(),
            ipv4: Some("192.0.2.7".parse().unwrap()),
            ipv6: None,
        };
        let records = render(&default_template(), &vars).unwrap();
        assert!(records.iter().all(|r| r.rtype != DnsType::Aaaa));
        let none = Variables { ipv4: None, ..vars };
        let records = render(&default_template(), &none).unwrap();
        assert_eq!(records.len(), 3, "CNAME, MX and TXT remain");
    }

    type DnsType = RecordType;

    #[test]
    fn bad_templates_name_the_line() {
        for (lines, code) in [
            (vec![line("@", "A", "not-an-ip", None)], "invalid_ipv4"),
            (
                vec![line("@", "A", "{{nonsense}}", None)],
                "unknown_variable",
            ),
            (
                vec![
                    line("www", "A", "192.0.2.1", None),
                    line("www", "CNAME", "x.org.", None),
                ],
                "cname_conflict",
            ),
            (
                vec![line("@", "MX", "m.example.com.", None)],
                "priority_required",
            ),
            (vec![line("@", "SOA", "x", None)], "unsupported_record_type"),
            (vec![line("@", "CNAME", "x.org.", None)], "cname_at_apex"),
        ] {
            let e = check_template(&lines).unwrap_err();
            assert_eq!(e.code, code, "{lines:?}");
        }
        let e = check_template(&[
            line("@", "A", "192.0.2.1", None),
            line("@", "A", "bad", None),
        ])
        .unwrap_err();
        assert!(e.message.starts_with("Record 2:"), "{}", e.message);
        assert!(check_template(&default_template()).is_ok());
        let many: Vec<_> = (0..=MAX_TEMPLATE_RECORDS)
            .map(|i| line(&format!("h{i}"), "A", "192.0.2.1", None))
            .collect();
        assert_eq!(check_template(&many).unwrap_err().code, "too_many_records");
    }
}
