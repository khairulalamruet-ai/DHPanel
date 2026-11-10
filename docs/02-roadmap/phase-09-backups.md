# Phase 09 — Backup ও Restore

**লক্ষ্য:** User আর system দুই ধরনের backup, scheduled, local ও remote (S3/SFTP) destination, আর নিরাপদ restore।

### T09.01 — Backup format ADR
- **কী করতে হবে:** Backup-এ কী থাকবে (files tar.zst, DB dump, DNS zone, metadata JSON, manifest + SHA-256)। এনক্রিপশন (age বা XChaCha20)। Incremental (restic বা নিজস্ব) — সিদ্ধান্ত।
- **DoD:** ADR merge হয়েছে।

### T09.02 — backups, destinations, schedules migration
- **DoD:** Destination credential এনক্রিপ্ট করে রাখা হয়।

### T09.03 — Destination adapter
- **কী করতে হবে:** `BackupDestination` trait। Local, S3 (যেকোনো S3-compatible), SFTP। "Test connection" বোতাম।
- **DoD:** তিনটা adapter-এর integration test আছে (MinIO, SFTP container)।

### T09.04 — Agent: CreateBackup (tenant)
- **কী করতে হবে:** Files + DB dump (consistent) + metadata → stream করে compress ও encrypt → destination। ionice/nice।
- **DoD:** ১০GB tenant backup-এর সময় site চলতে থাকে।

### T09.05 — Agent: RestoreBackup
- **কী করতে হবে:** পুরো বা আংশিক (শুধু files, শুধু একটা DB, একটা ফোল্ডার)। Restore-এর আগে বর্তমান অবস্থার snapshot।
- **DoD:** Restore-এর পর site আর DB আগের মতো চলে।

### T09.06 — Manifest যাচাই
- **কী করতে হবে:** প্রতিটা অংশের SHA-256। Restore-এর আগে যাচাই হয়।
- **DoD:** নষ্ট backup ধরা পড়ে আর restore থামে।

### T09.07 — Scheduler ও retention
- **কী করতে হবে:** Cron schedule, retention (দৈনিক N, সাপ্তাহিক M), concurrency সীমা (node-প্রতি একসাথে কয়টা)।
- **DoD:** পুরনো backup নিজে থেকে মুছে যায়।

### T09.08 — System backup
- **কী করতে হবে:** Control plane DB (pg_dump), config, CA (এনক্রিপ্টেড), audit।
- **DoD:** নতুন server-এ system restore-এর runbook পরীক্ষা করা হয়েছে।

### T09.09 — Client backup UI
- **কী করতে হবে:** Backup তালিকা, "এখন backup নাও", restore wizard (কী restore করবেন), download।
- **DoD:** E2E test আছে।

### T09.10 — Admin backup UI
- **কী করতে হবে:** Destination, schedule, সব backup job-এর অবস্থা, ব্যর্থ job, system backup।
- **DoD:** ব্যর্থ হলে notification যায়।

### T09.11 — User export (account export)
- **কী করতে হবে:** OpenPanel-এর `users/{u}/export`-এর মতো: পুরো account একটা ফাইলে (migration-এ কাজে লাগে)।
- **DoD:** অন্য node-এ import করা যায় (phase 15-এর ভিত্তি)।
