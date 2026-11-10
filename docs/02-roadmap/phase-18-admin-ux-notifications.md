# Phase 18 — Dashboard, Notification, Branding, i18n, Update

### T18.01 — Admin dashboard
- **কী করতে হবে:** Widget: node অবস্থা, tenant সংখ্যা, resource ব্যবহার, সাম্প্রতিক operation, security alert, license অবস্থা। Layout সাজানো যায় (OpenPanel `/dashboard/layout`)।
- **DoD:** Widget-এর অবস্থান সংরক্ষিত থাকে।

### T18.02 — Client dashboard (পূর্ণ)
- **কী করতে হবে:** Resource bar, domain, শেষ backup, SSL সতর্কতা, shortcut, favorites।
- **DoD:** ২ সেকেন্ডের মধ্যে লোড হয়।

### T18.03 — Notification model ও channel
- **কী করতে হবে:** In-app, email (SMTP setting), webhook। Event → নিয়ম → channel। Pause, snooze (OpenPanel-এর মতো)।
- **DoD:** SSL মেয়াদ, backup ব্যর্থতা আর node offline — তিনটাই notification পাঠায়।

### T18.04 — Email template
- **DoD:** বাংলা ও English template, branding যুক্ত।

### T18.05 — Notification UI
- **DoD:** Bell icon, তালিকা, পড়া হয়েছে চিহ্ন, settings।

### T18.06 — Branding
- **কী করতে হবে:** Panel-এর নাম, logo, favicon, রং, login পেজের লেখা, nameserver। Custom CSS (sanitized) — custom JS নিরাপত্তার কারণে বাদ, অথবা শুধু client app-এ CSP nonce দিয়ে।
- **DoD:** Branding বদলালে reload ছাড়াই প্রয়োগ হয়।

### T18.07 — Onboarding ও tour
- **কী করতে হবে:** প্রথম admin login-এ quickstart checklist (node যোগ, plan তৈরি, nameserver, SMTP)। Client-এর জন্য ছোট tour।
- **DoD:** Dismiss করলে আর দেখায় না।

### T18.08 — Translation পূর্ণ করা
- **কী করতে হবে:** সব module-এর `bn` আর `en`। অনুপস্থিত key ধরার CI check।
- **DoD:** CI-তে key অনুপস্থিত থাকলে fail করে।

### T18.09 — Global search ও command palette
- **DoD:** `Ctrl+K` দিয়ে user, domain বা পেজ খোঁজা যায়।

### T18.10 — Panel update ব্যবস্থা
- **কী করতে হবে:** Version দেখা, channel (stable/beta), "এখন update করো", migration, rollback। Phase 17-এর signed manifest ব্যবহার করে।
- **DoD:** Update ব্যর্থ হলে আগের version-এ ফিরে যায়।

### T18.11 — API token ও public API docs
- **কী করতে হবে:** Admin বা client API token (scope সহ), OpenAPI docs পেজ, WHMCS বা billing integration-এর জন্য endpoint।
- **DoD:** Token দিয়ে `GET /users` চলে, scope-এর বাইরের কাজে 403 আসে।

### T18.12 — Support report
- **কী করতে হবে:** OpenPanel `/api/support/report`-এর মতো: diagnostic bundle (version, log-এর অংশ, config, secret বাদ দিয়ে)।
- **DoD:** Bundle-এ কোনো secret নেই (test দিয়ে যাচাই)।
