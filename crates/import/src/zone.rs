//! A small reader for BIND zone files (master file format), enough for the zones panels write.

use crate::model::{DnsRecord, DnsZone};

/// Reads the records of a zone. The SOA and the zone's own name servers are left out (our
/// DNS server provides them); a line that cannot be read is skipped and named in the second
/// return value.
#[must_use]
pub fn parse_zone(origin: &str, text: &str) -> (DnsZone, Vec<String>) {
    let origin = origin.trim_end_matches('.').to_ascii_lowercase();
    let mut ctx = Ctx {
        origin: origin.clone(),
        ttl: None,
        owner: "@".to_owned(),
    };
    let mut zone = DnsZone {
        name: origin,
        records: Vec::new(),
    };
    let mut skipped = Vec::new();
    for line in logical_lines(text) {
        let starts_blank = line.starts_with(char::is_whitespace);
        let tokens = tokenize(&line);
        let Some(first) = tokens.first() else {
            continue;
        };
        if first.starts_with('$') {
            match (first.to_ascii_uppercase().as_str(), tokens.get(1)) {
                ("$ORIGIN", Some(o)) => {
                    ctx.origin = absolute(o, &ctx.origin).trim_end_matches('.').to_owned();
                }
                ("$TTL", Some(t)) => ctx.ttl = parse_ttl(t),
                _ => {}
            }
            continue;
        }
        match record(&mut ctx, &tokens, starts_blank, &zone.name) {
            Ok(Some(r)) => zone.records.push(r),
            Ok(None) => {}
            Err(why) => skipped.push(format!(
                "{}: {why}",
                line.chars().take(80).collect::<String>()
            )),
        }
    }
    (zone, skipped)
}

struct Ctx {
    origin: String,
    ttl: Option<u32>,
    owner: String,
}

/// Joins `( ... )` continuations and drops comments.
fn logical_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for raw in text.lines() {
        let mut line = String::new();
        let mut quoted = false;
        let mut escaped = false;
        for c in raw.chars() {
            if escaped {
                line.push(c);
                escaped = false;
                continue;
            }
            match c {
                '\\' => {
                    escaped = true;
                    line.push(c);
                }
                '"' => {
                    quoted = !quoted;
                    line.push(c);
                }
                ';' if !quoted => break,
                '(' if !quoted => {
                    depth += 1;
                    line.push(' ');
                }
                ')' if !quoted => {
                    depth -= 1;
                    line.push(' ');
                }
                _ => line.push(c),
            }
        }
        if current.is_empty() {
            current = line;
        } else {
            current.push(' ');
            current.push_str(line.trim());
        }
        if depth <= 0 {
            depth = 0;
            if !current.trim().is_empty() {
                out.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut had_quote = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            '"' => {
                quoted = !quoted;
                had_quote = true;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() || had_quote {
                    // A quoted string keeps its quotes so that TXT data survives intact.
                    tokens.push(if had_quote {
                        format!("\"{cur}\"")
                    } else {
                        std::mem::take(&mut cur)
                    });
                    cur.clear();
                    had_quote = false;
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || had_quote {
        tokens.push(if had_quote { format!("\"{cur}\"") } else { cur });
    }
    tokens
}

fn parse_ttl(s: &str) -> Option<u32> {
    let mut total: u64 = 0;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: u64 = num.parse().ok()?;
            num.clear();
            total += n * match c.to_ascii_lowercase() {
                's' => 1,
                'm' => 60,
                'h' => 3600,
                'd' => 86_400,
                'w' => 604_800,
                _ => return None,
            };
        }
    }
    if !num.is_empty() {
        total += num.parse::<u64>().ok()?;
    }
    u32::try_from(total).ok()
}

fn is_ttl(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_digit()) && parse_ttl(s).is_some()
}

fn is_class(s: &str) -> bool {
    matches!(s.to_ascii_uppercase().as_str(), "IN" | "CH" | "HS")
}

fn absolute(name: &str, origin: &str) -> String {
    if name == "@" {
        format!("{origin}.")
    } else if name.ends_with('.') {
        name.to_ascii_lowercase()
    } else {
        format!("{}.{origin}.", name.to_ascii_lowercase())
    }
}

/// A name relative to the zone: `@` for the zone, `www` for `www.zone.`, the full name
/// (without the trailing dot) for names outside the zone.
fn relative(abs: &str, zone: &str) -> String {
    let abs = abs.trim_end_matches('.');
    if abs == zone {
        "@".to_owned()
    } else if let Some(rest) = abs.strip_suffix(&format!(".{zone}")) {
        rest.to_owned()
    } else {
        abs.to_owned()
    }
}

/// A target name inside record data: relative to the origin unless it ends with a dot.
fn target(name: &str, ctx: &Ctx) -> String {
    absolute(name, &ctx.origin).trim_end_matches('.').to_owned()
}

fn record(
    ctx: &mut Ctx,
    tokens: &[String],
    starts_blank: bool,
    zone: &str,
) -> Result<Option<DnsRecord>, String> {
    let mut i = 0;
    if !starts_blank {
        ctx.owner = relative(&absolute(&tokens[0], &ctx.origin), zone);
        i = 1;
    }
    let mut ttl = None;
    // TTL and class may come in either order.
    for _ in 0..2 {
        match tokens.get(i) {
            Some(t) if is_ttl(t) => {
                ttl = parse_ttl(t);
                i += 1;
            }
            Some(t) if is_class(t) => i += 1,
            _ => {}
        }
    }
    let rtype = tokens.get(i).ok_or("no record type")?.to_ascii_uppercase();
    let data = &tokens[i + 1..];
    let ttl = ttl.or(ctx.ttl);
    let name = ctx.owner.clone();
    let mk = |priority: Option<u16>, content: String| DnsRecord {
        name: name.clone(),
        rtype: rtype.clone(),
        ttl,
        priority,
        content,
    };
    match rtype.as_str() {
        "SOA" => Ok(None),
        "NS" => {
            let host = data.first().ok_or("NS needs a name")?;
            if name == "@" {
                // The zone's own name servers are ours to provide.
                return Ok(None);
            }
            Ok(Some(mk(None, target(host, ctx))))
        }
        "A" | "AAAA" => {
            let ip = data.first().ok_or("an address is missing")?;
            Ok(Some(mk(None, ip.to_ascii_lowercase())))
        }
        "CNAME" => Ok(Some(mk(
            None,
            target(data.first().ok_or("CNAME needs a name")?, ctx),
        ))),
        "MX" => {
            let prio: u16 = data
                .first()
                .and_then(|p| p.parse().ok())
                .ok_or("MX needs a priority")?;
            let host = data.get(1).ok_or("MX needs a name")?;
            Ok(Some(mk(Some(prio), target(host, ctx))))
        }
        "TXT" | "SPF" => {
            if data.is_empty() {
                return Err("TXT needs text".into());
            }
            let text: String = data
                .iter()
                .map(|t| t.trim_matches('"'))
                .collect::<Vec<_>>()
                .concat();
            let mut r = mk(None, text);
            r.rtype = "TXT".to_owned();
            Ok(Some(r))
        }
        "SRV" => {
            if data.len() < 4 {
                return Err("SRV needs priority, weight, port and target".into());
            }
            let prio: u16 = data[0].parse().map_err(|_| "bad SRV priority")?;
            Ok(Some(mk(
                Some(prio),
                format!("{} {} {}", data[1], data[2], target(&data[3], ctx)),
            )))
        }
        "CAA" => {
            if data.len() < 3 {
                return Err("CAA needs flags, tag and value".into());
            }
            Ok(Some(mk(
                None,
                format!("{} {} \"{}\"", data[0], data[1], data[2].trim_matches('"')),
            )))
        }
        other => Err(format!("type {other} is not carried over")),
    }
}
