# Phase 03 — Module Registry, Feature-Gate, Settings

**লক্ষ্য:** Module যোগ করা, চালু বা বন্ধ করা, আর plan অনুযায়ী ফিচার নিয়ন্ত্রণের ভিত্তি।
বিস্তারিত: [03-module-system.md](../01-architecture/03-module-system.md)

### T03.01 — Manifest schema
- **কী করতে হবে:** `module.toml`-এর schema আর parser। Validation (নাম, dependency, permission key-এর format)।
- **DoD:** ভুল manifest দিলে startup-এ পরিষ্কার error আসে।

### T03.02 — `Module` trait ও registry
- **কী করতে হবে:** `kernel` crate-এ trait (manifest, routes, migrations, jobs, on_event, health)। Registry dependency গ্রাফ যাচাই করে আর cycle ধরে।
- **DoD:** Cycle থাকলে test fail করে।

### T03.03 — modules টেবিল ও sync
- **কী করতে হবে:** Startup-এ registry থেকে `modules` টেবিল sync হয় (নতুন module যোগ হয়, পুরনোটা "missing" চিহ্নিত হয়)।
- **DoD:** নতুন module crate যোগ করলে নিজে থেকে DB-তে আসে।

### T03.04 — Route mount
- **কী করতে হবে:** শুধু enabled module-এর admin আর client router mount হয়। Module enable বা disable করলে hot reload (router swap) হবে, অথবা restart লাগবে বলে জানানো হবে।
- **DoD:** বন্ধ module-এর route 404 দেয়।

### T03.05 — Event bus
- **কী করতে হবে:** In-process typed event (`TenantCreated`, `TenantDeleted`, `DomainAdded`…)। Transactional outbox (DB-তে `outbox` টেবিল) + worker দিয়ে পাঠানো।
- **DoD:** Transaction rollback হলে event যায় না।

### T03.06 — Permission sync
- **কী করতে হবে:** Manifest-এর permission key DB-তে যায়। `super_admin` সব পায়।
- **DoD:** নতুন module-এর permission role editor-এ দেখা যায়।

### T03.07 — feature-gate crate
- **কী করতে হবে:** `require_feature(key)` আর `check_limit(key, current+1)` extractor বা service। Tenant-এর plan থেকে cache (Redis, plan বদলালে invalidate)।
- **DoD:** Unit test: feature বন্ধ থাকলে 403 আর `code=feature_disabled`, সীমা পার হলে 409 আর `code=limit_reached`।

### T03.08 — settings টেবিল ও service
- **কী করতে হবে:** Scope (global/node/tenant), key, value jsonb। প্রতিটা module নিজের settings schema (JSON Schema) দেয়। লেখার আগে validation।
- **DoD:** ভুল type দিলে 422 আসে।

### T03.09 — Settings API
- **কী করতে হবে:** `GET/PUT /admin/settings/{module}`। পরিবর্তনের diff audit-এ যায়।
- **DoD:** Audit-এ আগের ও পরের মান দেখা যায়।

### T03.10 — Bootstrap endpoint
- **কী করতে হবে:** `GET /{admin|client}/bootstrap` → user, permissions, modules, features, limits, branding, locale, CSRF।
- **DoD:** Response ৫০ms-এর কম (cache)।

### T03.11 — Frontend module registry
- **কী করতে হবে:** `packages/modules/*` থেকে route ও nav একত্র করা। Bootstrap দিয়ে ফিল্টার। `<Can>`, `<FeatureGate>` component। "Plan-এ নেই" পেজ।
- **DoD:** একটা dummy module চালু বা বন্ধ করলে menu বদলায়।

### T03.12 — Module manager UI (admin)
- **কী করতে হবে:** Module তালিকা (category, version, dependency), চালু ও বন্ধ toggle (dependency সতর্কতা সহ)।
- **DoD:** `dns` বন্ধ করলে `domains` বন্ধ করার সতর্কতা দেখায়।

### T03.13 — Settings UI কাঠামো
- **কী করতে হবে:** JSON Schema থেকে form নিজে থেকে তৈরি (প্রাথমিক type: string, int, bool, enum)। Module অনুযায়ী tab।
- **DoD:** নতুন setting যোগ করলে UI কোড না লিখেই form দেখা যায়।

### T03.14 — Module authoring guide
- **কী করতে হবে:** `docs/guides/creating-a-module.md`: নতুন module যোগ করার checklist।
- **DoD:** একজন নতুন developer guide পড়ে একটা "hello" module বানাতে পারেন।
