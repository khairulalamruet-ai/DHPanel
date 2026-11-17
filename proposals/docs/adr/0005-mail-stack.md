# ADR 0005 — The mail stack

Status: **proposed** (needs the human's decision before T13.02)

## Context
The panel must offer mailboxes, forwarders, DKIM/SPF/DMARC, spam filtering, quotas, webmail with
single sign-on and brute-force protection, per hosting account and per domain, managed by typed
operations, and shipped as containers we can pin by digest (as for the other roles).

## Options
| | docker-mailserver | Stalwart Mail Server | Own Postfix + Dovecot + Rspamd |
|---|---|---|---|
| Language / shape | shell + Postfix + Dovecot + Rspamd in one image | one Rust binary (SMTP, IMAP, JMAP, spam filter, DKIM) | three daemons we configure |
| Managing users and domains | files and a `setup` script; no real API | HTTP management API; accounts, domains, aliases, DKIM keys, quotas | our own config files and reloads |
| Fit for typed operations | we would write config files and reload | call an API with typed bodies | same as docker-mailserver |
| Maturity | years in production | younger, quick moving, active security response | the most proven parts, the most work |
| Footprint | large | small | medium |
| Webmail | Roundcube separately | Roundcube separately (IMAP) or JMAP clients | Roundcube separately |
| Licence | MIT | AGPL-3.0 (with a commercial option) | permissive |

## Proposal
Use **Stalwart**, one container per mail node, managed through its API by the node agent, with
Roundcube for webmail (single sign-on by a one-time token, as for phpMyAdmin). Reasons: an API
fits our "typed operation" rule better than editing files; one binary is easier to pin, scan and
update; the mail stack stays in the memory-safe family of the rest of the system.

## Things the human must weigh
- **Licence:** AGPL-3.0. Running unmodified Stalwart as a separate service is allowed; any
  change we make to it must be published. A commercial licence is available.
- **Maturity risk:** if a feature we depend on is missing or unstable, fall back to
  docker-mailserver (same operations, different handler); the typed operation layer hides which
  one is behind it.
- **Deliverability** (reverse DNS, IP reputation, port 25) is a deployment matter, not a stack
  matter, and is the same with every option.

## Consequences
If accepted: T13.02 provisions one Stalwart container (SMTP 25/465/587, IMAP 993) with the
certificate from the panel's Caddy; T13.03 and later call its management API.
