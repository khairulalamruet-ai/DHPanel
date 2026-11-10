# Phase 05 — Domains, DNS, SSL

**লক্ষ্য:** Client domain যোগ করবেন → DNS zone তৈরি হবে → SSL নিজে থেকে আসবে।

## Domains
### T05.01 — domains migration ও domain model
- **কী করতে হবে:** `domains`, `redirects`। FQDN validation (IDN → punycode, public suffix list, blocklist, `domain_restriction`-এর মতো নিষিদ্ধ তালিকা)।
- **DoD:** Unit test: `example.com` ঠিক, `com` ভুল, `xn--` ঠিক।

### T05.02 — Ownership ও duplication নিয়ম
- **কী করতে হবে:** একটা FQDN সারা সিস্টেমে একবারই থাকতে পারবে। Subdomain শুধু নিজের domain-এর অধীনে (permit_subdomain_sharing setting)।
- **DoD:** অন্যের domain-এর subdomain যোগ করলে 409 আসে।

### T05.03 — Domain API (client + admin)
- **কী করতে হবে:** List, add (primary/addon/sub/alias), delete, suspend, unsuspend, docroot বদলানো। Limit check (`max_domains`)।
- **DoD:** Limit পার হলে `limit_reached` আসে।

### T05.04 — Redirect API
- **DoD:** 301/302 CRUD। Loop ধরা পড়ে।

### T05.05 — Domain UI (client)
- **কী করতে হবে:** তালিকা, যোগ করার dialog, docroot সম্পাদনা, redirect tab, DNS ও SSL-এর অবস্থা badge।
- **DoD:** E2E test: domain যোগ → তালিকায় দেখা যায়।

### T05.06 — Domain UI (admin)
- **কী করতে হবে:** সব domain, owner, node, bulk add, suspend।
- **DoD:** Bulk add-এ ১০০টা domain যোগ করা যায় (operation queue দিয়ে)।

## DNS
### T05.07 — DNS backend সিদ্ধান্ত ও setup
- **কী করতে হবে:** PowerDNS Authoritative (API) ব্যবহার, ADR লেখা। Dev compose-এ যোগ।
- **DoD:** API দিয়ে zone তৈরি করা যায়।

### T05.08 — dns_zones ও records migration + zone template
- **কী করতে হবে:** Default template (A, AAAA, CNAME www, MX, TXT SPF)। Admin template সম্পাদনা করতে পারেন।
- **DoD:** Domain যোগ করলে template থেকে zone তৈরি হয়।

### T05.09 — DNS provider adapter
- **কী করতে হবে:** `DnsProvider` trait। PowerDNS বাস্তবায়ন (পরে Cloudflare)।
- **DoD:** Trait-এর mock দিয়ে test আছে।

### T05.10 — Record API
- **কী করতে হবে:** CRUD। Type validation (A, AAAA, CNAME, MX, TXT, SRV, CAA, NS), TTL সীমা, CNAME-এর সাথে অন্য record থাকলে conflict।
- **DoD:** ভুল record দিলে 422 আর স্পষ্ট বার্তা আসে।

### T05.11 — DNS editor UI
- **কী করতে হবে:** Table-এ inline সম্পাদনা, type অনুযায়ী form, zone export ও import (BIND format)।
- **DoD:** E2E test আছে।

### T05.12 — Nameserver settings
- **কী করতে হবে:** Admin ns1–ns4 সেট করেন। Client-কে "আপনার registrar-এ এই NS দিন" নির্দেশনা দেখানো হয়।
- **DoD:** Domain-এর NS ঠিক আছে কিনা যাচাইয়ের বোতাম আছে।

## SSL
### T05.13 — SSL কৌশল ADR
- **কী করতে হবে:** Caddy-র on-demand/auto TLS নাকি নিজস্ব ACME (`instant-acme`), সিদ্ধান্ত নেওয়া। প্রস্তাব: Caddy-র auto TLS + `ask` endpoint (control plane যাচাই করে domain বৈধ কিনা)।
- **DoD:** ADR merge হয়েছে।

### T05.14 — Caddy route ব্যবস্থাপনা (agent)
- **কী করতে হবে:** Agent Caddy admin API (localhost:2019) দিয়ে domain → tenant upstream route যোগ ও সরায়। `Reconcile`-এ সব route মেলানো হয়।
- **DoD:** Caddy restart হলেও route ঠিক থাকে (config persist বা reconcile দিয়ে)।

### T05.15 — On-demand TLS `ask` endpoint
- **কী করতে হবে:** `GET /internal/tls/ask?domain=` → শুধু active domain হলে 200। Rate limit।
- **DoD:** অজানা domain-এর জন্য cert তৈরি হয় না।

### T05.16 — certificates টেবিল ও অবস্থা sync
- **কী করতে হবে:** Agent cert-এর মেয়াদ রিপোর্ট করে। মেয়াদ শেষের ১৪ দিন আগে notification যায়।
- **DoD:** UI-তে মেয়াদ দেখা যায়।

### T05.17 — Custom certificate upload
- **কী করতে হবে:** PEM cert + key + chain। Key আর cert মেলে কিনা যাচাই। এনক্রিপ্ট করে রাখা।
- **DoD:** অমিল key দিলে 422 আসে।

### T05.18 — HSTS ও force HTTPS
- **DoD:** Domain-প্রতি toggle আছে, header যাচাই করা হয়েছে।

### T05.19 — SSL UI
- **কী করতে হবে:** Domain-প্রতি অবস্থা, issuer, মেয়াদ, "আবার চেষ্টা" বোতাম, custom upload।
- **DoD:** E2E test আছে।
