# Roadmap — ধাপে ধাপে কাজ

## Task লেখার নিয়ম
প্রতিটা task **ছোট** হবে (আধা দিন থেকে ১ দিন), আর একটা PR-এ শেষ হবে। Task-এর গঠন:

```
### T<phase>.<nn> — শিরোনাম
- **কী করতে হবে:** …
- **ফলাফল (output):** কোন ফাইল, endpoint বা পেজ
- **শেষ হয়েছে বলা যাবে যখন (DoD):** যাচাইযোগ্য শর্ত
- **নির্ভরতা:** T…
```

প্রতিটা task-এর **সাধারণ DoD** (আলাদা করে লেখা থাকবে না, কিন্তু সবসময় প্রযোজ্য):
- `cargo fmt`, `clippy -D warnings`, `cargo test` সব pass। Frontend-এ `pnpm lint`, `typecheck` আর `test` pass।
- নতুন endpoint-এর OpenAPI schema আছে, আর permission ও feature gate লাগানো।
- লেখার কাজে audit log লেখা হয়।
- নতুন UI text-এর বাংলা ও English দুটো translation আছে।

## Phase তালিকা ও ক্রম

| Phase | নাম | MVP? | নির্ভরতা |
|---|---|---|---|
| [00](phase-00-foundation.md) | Foundation: repo, CI, workspace, skeleton | ✅ | — |
| [01](phase-01-auth-rbac.md) | Auth, session, 2FA, RBAC, audit | ✅ | 00 |
| [02](phase-02-node-agent.md) | Node agent, mTLS, operation ledger | ✅ | 00 |
| [03](phase-03-module-system-settings.md) | Module registry, feature-gate, settings | ✅ | 01 |
| [04](phase-04-plans-tenants-users.md) | Plans, feature set, tenant, user | ✅ | 02, 03 |
| [05](phase-05-domains-dns-ssl.md) | Domains, DNS, SSL | ✅ | 04 |
| [06](phase-06-webserver-php.md) | Web server, vhost, PHP | ✅ | 05 |
| [07](phase-07-databases.md) | MySQL, PostgreSQL, phpMyAdmin | ✅ (MySQL) | 04 |
| [08](phase-08-files-ftp.md) | File manager, FTP/SFTP | ✅ | 04 |
| [09](phase-09-backups.md) | Backup ও restore | ✅ | 07, 08 |
| [10](phase-10-cron-logs-monitoring.md) | Cron, log, usage, service, process | ✅ | 04 |
| [11](phase-11-security.md) | WAF, firewall, malware, security center | ✅ | 06 |
| [12](phase-12-apps.md) | WordPress, autoinstaller, Node/PM2, container | ✅ (WP) | 06, 07 |
| [13](phase-13-email.md) | Email stack | v2 | 05 |
| [14](phase-14-resellers.md) | Reseller | v2 | 04 |
| [15](phase-15-migration-import.md) | cPanel, CyberPanel বা অন্য panel থেকে import, tenant transfer | v2 | 09 |
| [16](phase-16-multinode-cluster.md) | Multi-node scheduling, DNS cluster | v2 | 02, 05 |
| [17](phase-17-license.md) | License Authority + client + integrity | ✅ (basic) | 01 |
| [18](phase-18-admin-ux-notifications.md) | Dashboard, notification, branding, i18n, update | ✅ | 03 |
| [19](phase-19-hardening-release.md) | Security review, load test, packaging, release | ✅ | সব |

**সমান্তরালে কাজ করা যায়:** 01 আর 02 একসাথে। 07, 08 আর 10 একসাথে। 17 আলাদা দল আলাদাভাবে করতে পারে।

## Milestone
- **M1 "Hello tenant":** 00–04 শেষ। Admin একজন user আর তার খালি container বানাতে পারেন।
- **M2 "প্রথম website":** 05–08। Domain + SSL + PHP + DB + file দিয়ে একটা site চলে।
- **M3 "Production MVP":** 09–12, 17, 18, 19।
- **M4 v2:** 13–16।
