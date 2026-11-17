# ADR 0006 — Certificates: Caddy on-demand TLS with an `ask` endpoint

Status: accepted (implemented in T05.14, T05.15, T05.16, T05.17, T05.18)

## Context
Every domain a client adds needs an HTTPS certificate on the node that serves it, without a human
step and without the panel holding every private key. Names arrive at any time (a client adds a
domain, DNS points to the node a minute later), and a stranger can point any name at a node's IP.

## Decision
- The web server on each node is **Caddy**. It gets and renews certificates itself (ACME) the first
  time a visitor asks for a name (**on-demand TLS**). Private keys are created and kept on the node.
- Before Caddy asks the CA, it asks the panel: `GET /api/v1/public/tls/ask?domain=<name>`
  (`crates/modules/domains/src/tls.rs`). `200` only for a name that belongs to a running (or
  suspended) hosting account and is itself active; `404` for everything else, `400` for a string
  that is not a host name, `429` when one address asks more than 600 times a minute. The endpoint has
  no body, no session and tells nothing DNS does not.
- Routes are pushed to Caddy's admin API by the node agent as a typed operation (T05.14); the panel
  never talks to Caddy directly and Caddy's admin socket is not reachable from outside the node.
- A client may upload their own certificate (T05.17). The panel checks it and stores the private
  key sealed with the master key; the agent installs it on the node (T05.17b) and Caddy serves it
  instead of requesting one.
- Per domain: force HTTPS and HSTS (T05.18), and the panel records each certificate's end date and
  warns before it (T05.16).

## Alternatives considered
- **ACME inside the panel** (`instant-acme`): the panel would hold every private key and push them
  to nodes; a panel breach would expose all keys. It also needs its own renewal scheduler.
- **certbot on each node**: shell-driven, needs a reload hook per renewal and has no way to refuse
  names the panel does not own.
- **A wildcard certificate per node**: fits only names we control, not clients' own domains.

## Consequences
- Issuance is lazy: the first visit to a new name waits for the certificate. Rate limits of the CA
  apply per registered domain; the `ask` limit and the active-domain check keep us from spending them
  on names that are not ours.
- The `ask` endpoint is the only thing standing between a hostile DNS record and a certificate
  request, so it must stay cheap, rate-limited and free of details. Changes to it need review.
- If the panel is unreachable, Caddy cannot issue **new** certificates; names that already have one
  keep working and renew from the cached state.
