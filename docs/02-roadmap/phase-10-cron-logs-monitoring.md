# Phase 10 — Cron, Logs, Usage, Services, Processes

**লক্ষ্য:** Client নিজের cron, log, resource ব্যবহার আর process দেখবেন। Admin পুরো server দেখবেন।

## Cron
### T10.01 — cron_jobs migration ও validation
- **কী করতে হবে:** Schedule parse করা (৫-field), সর্বনিম্ন interval (setting, যেমন ৫ মিনিট), command দৈর্ঘ্যের সীমা।
- **DoD:** `* * * * *` দিলে interval নিয়মে প্রত্যাখ্যাত হয় (setting অনুযায়ী)।

### T10.02 — Agent: SyncCronJobs ও runner
- **কী করতে হবে:** Agent-এর scheduler tenant container-এর ভেতরে command চালায় (tenant UID হিসেবে, timeout সহ)। Output log রাখা হয়।
- **DoD:** Job চলে আর last_status আপডেট হয়।

### T10.03 — Cron UI
- **কী করতে হবে:** তালিকা, তৈরি (preset: প্রতি ঘণ্টা বা প্রতিদিন), চালু ও বন্ধ, "এখন চালাও", শেষ output।
- **DoD:** E2E test আছে।

## Logs
### T10.04 — Agent: ReadLog, TailLogs
- **কী করতে হবে:** Access ও error log (domain-প্রতি), PHP error log। Stream (SSE বা WebSocket)। শুধু allowlist-এর log path।
- **DoD:** Live tail UI-তে আসে।

### T10.05 — Log rotation
- **কী করতে হবে:** Agent-এর ভেতরে job: size বা দিন অনুযায়ী rotate, compress, retention।
- **DoD:** Disk-এ log সীমার মধ্যে থাকে।

### T10.06 — Log viewer UI
- **কী করতে হবে:** Domain বেছে নেওয়া, access বা error, search, live mode, download।
- **DoD:** ১০০MB log-এও ব্রাউজার আটকে যায় না (virtualized list)।

### T10.07 — Admin system log
- **কী করতে হবে:** Control plane, agent, Caddy, auth log (OpenPanel `/log/*`-এর মতো)।
- **DoD:** Admin সব দেখতে পান, client কিছুই দেখেন না।

## Usage ও Monitoring
### T10.08 — Agent: CollectMetrics
- **কী করতে হবে:** Container-প্রতি CPU, mem, io, net (cgroup v2 থেকে)। Node-এর সার্বিক মেট্রিক। প্রতি ৩০ সেকেন্ডে push।
- **DoD:** ১০০ container-এর node-এও agent-এর CPU ব্যবহার ২%-এর নিচে।

### T10.09 — usage_samples storage
- **কী করতে হবে:** Partitioned টেবিল (মাস অনুযায়ী) বা TimescaleDB। Downsample (১ মিনিট → ১ ঘণ্টা), retention।
- **DoD:** ৩০ দিনের query ২০০ms-এর কম।

### T10.10 — Usage chart UI
- **কী করতে হবে:** Client: নিজের CPU, RAM, disk, bandwidth (২৪ ঘণ্টা, ৭ দিন, ৩০ দিন)। Admin: server আর tenant অনুযায়ী top-N।
- **DoD:** Chart light ও dark দুই theme-এ পড়া যায়।

### T10.11 — Prometheus exporter
- **কী করতে হবে:** Control plane আর agent `/metrics` (node, tenant মেট্রিক)। Grafana dashboard JSON।
- **DoD:** Grafana-তে dashboard import করলে চলে।

## Services ও Processes
### T10.12 — Agent: ListContainers, RestartContainer (tenant scope)
- **DoD:** Client শুধু নিজের service দেখেন ও restart করেন।

### T10.13 — Admin service manager
- **কী করতে হবে:** Node-এর core service (Caddy, DB, agent, DNS)। অবস্থা, restart, log। Crash হলে alert।
- **DoD:** Service বন্ধ হলে ১ মিনিটের মধ্যে notification যায়।

### T10.14 — Process manager
- **কী করতে হবে:** Client: নিজের container-এর process তালিকা আর kill (SIGTERM, তারপর SIGKILL)। Admin: host process (sudo mode)।
- **DoD:** Client অন্যের process দেখতে পান না।

### T10.15 — Server tools (admin)
- **কী করতে হবে:** Timezone, swap, memory cache drop, reboot (sudo mode + নিশ্চিতকরণ), SSH config দেখা — OpenPanel-এর `/api/server/*`-এর নিরাপদ সংস্করণ, priv-helper দিয়ে।
- **DoD:** প্রতিটা কাজ audit-এ যায়।
