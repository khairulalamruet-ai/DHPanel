# ০২ — Tech Stack

Backend `doc-guide/rust_recommended_stack_hosting_panel.pdf` আর `complete_license_anti_nulled_architecture.pdf` অনুযায়ী। Frontend আপনার নির্দেশ অনুযায়ী React (Next.js নয়)।

## Backend (Rust)

| কাজ | Crate | মন্তব্য |
|---|---|---|
| HTTP | `axum` | Router, extractor, middleware |
| Runtime | `tokio` | |
| Middleware | `tower`, `tower-http` | trace, cors, timeout, compression, request-id, limit |
| Docker | `bollard` | শুধু node-agent-এ |
| DB | `sqlx` (postgres, macros, migrate) | Compile-time checked query |
| Redis | `fred` বা `redis` | rate-limit, cache, pubsub |
| Jobs | `apalis` (postgres backend) | durable queue |
| Validation | `validator` + নিজস্ব domain check | |
| Serialization | `serde`, `serde_json` | |
| Error | `thiserror` (lib), `anyhow` (bin) | |
| Auth | server-side session (Redis), `argon2`, `totp-rs` | PDF: session পছন্দনীয় |
| Passkey (পরে) | `webauthn-rs` | |
| Crypto | `ed25519-dalek`, `sha2`, `rand` | |
| mTLS | `rustls`, `rcgen` (CA) | node certificate |
| Agent RPC | HTTP/2 + JSON (`axum` + `reqwest`) অথবা `tonic` (gRPC) | সিদ্ধান্ত: [08](08-node-agent-protocol.md) |
| Observability | `tracing`, `tracing-subscriber`, `opentelemetry-otlp` | |
| Metrics | `metrics` + `metrics-exporter-prometheus` | |
| OpenAPI | `utoipa` | Frontend-এর type এখান থেকে তৈরি হবে |
| Config | `figment` বা `config` | env + ফাইল |
| ACME | `instant-acme` | অথবা Caddy-র auto-TLS ব্যবহার |
| DNS | PowerDNS HTTP API (`reqwest`) | |
| Test | `tokio::test`, `sqlx::test`, `testcontainers` | |
| Security | `cargo-audit`, `cargo-deny` | CI-তে |

## Frontend (React, Next.js নয়)

| কাজ | Library | কারণ |
|---|---|---|
| Build | **Vite** (multi-entry: admin, client) | দ্রুত, SPA |
| ভাষা | TypeScript (strict) | |
| UI | React 19 | |
| Routing | **TanStack Router** (file-based, type-safe) | Module অনুযায়ী route যোগ করা সহজ |
| Server state | **TanStack Query** | cache, retry, SSE invalidation |
| Form | React Hook Form + Zod | |
| UI kit | **shadcn/ui** (Radix) + Tailwind CSS | নিজের নিয়ন্ত্রণে থাকে, theme-যোগ্য |
| Table | TanStack Table | |
| Chart | Recharts | usage graph |
| Editor | Monaco (code/vhost), CodeMirror (হালকা) | |
| Terminal (পরে) | xterm.js | |
| i18n | i18next | বাংলা + English |
| API type | `openapi-typescript` + `openapi-fetch` | backend-এর utoipa থেকে |
| Test | Vitest, React Testing Library, Playwright | |
| Lint | ESLint, Prettier | |
| Package manager | pnpm (workspace) | |

**Next.js বাদ দেওয়ার প্রভাব:** SSR লাগবে না, কারণ panel login করা অবস্থায় ব্যবহার হয় আর SEO-র প্রয়োজন নেই। Session cookie `HttpOnly; Secure; SameSite=Strict` হবে, আর CSRF token header-এ যাবে। Static build সরাসরি Caddy থেকে serve হবে।

## Infra
| কাজ | প্রযুক্তি |
|---|---|
| OS | Ubuntu 24.04 / Debian 12 / AlmaLinux 9 |
| Process | systemd (control-api, worker, node-agent) |
| Container | Docker Engine (Podman socket-ও সমর্থিত) |
| Edge | Caddy + Coraza WAF |
| DNS | PowerDNS Authoritative |
| Firewall | nftables |
| Monitoring | Prometheus + Grafana + Loki (ঐচ্ছিক) |
| CI | GitHub Actions (fmt, clippy, test, audit, build, image scan) |
