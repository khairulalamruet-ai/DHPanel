# Phase 19 — Hardening, Testing, Packaging, Release

### T19.01 — Installer script
- **কী করতে হবে:** এক কমান্ডে install (OS যাচাই, Docker, PostgreSQL, Redis, Caddy, control plane, local agent, firewall, প্রথম admin)। Idempotent। ব্যর্থ হলে পরিষ্কার বার্তা।
- **DoD:** নতুন Ubuntu 24.04 আর Debian 12-এ ১৫ মিনিটের মধ্যে install হয়।

### T19.02 — systemd hardening
- **কী করতে হবে:** `ProtectSystem=strict`, `NoNewPrivileges`, `PrivateTmp`, `CapabilityBoundingSet`, `ReadWritePaths` — প্রতিটা service-এর জন্য।
- **DoD:** `systemd-analyze security` স্কোর ৩-এর নিচে (control-api)।

### T19.03 — Tenant isolation test suite
- **কী করতে হবে:** দুটো tenant। একটা থেকে অন্যটার ফাইল, DB, process আর network-এ ঢোকার সব চেষ্টা।
- **DoD:** সব চেষ্টা ব্যর্থ হয়, আর CI-তে নিয়মিত চলে।

### T19.04 — API security test
- **কী করতে হবে:** IDOR (অন্যের ID), CSRF, auth bypass, rate limit, injection (OWASP ASVS L2 checklist)।
- **DoD:** Checklist-এর সব আইটেম pass করে।

### T19.05 — Load test
- **কী করতে হবে:** k6 দিয়ে: ৫০০ tenant, ১০০ জন একসাথে UI-তে, ১০০০ op/মিনিট।
- **DoD:** p95 API latency ৩০০ms-এর কম, কোনো error নেই।

### T19.06 — Chaos ও failure test
- **কী করতে হবে:** Agent বন্ধ, DB restart, Redis বন্ধ, network বিচ্ছিন্ন।
- **DoD:** কোনো operation হারায় না, সবই পরে শেষ হয়।

### T19.07 — Backup restore drill
- **DoD:** System backup থেকে নতুন server-এ পুরো panel ফিরে আসে (runbook অনুযায়ী)।

### T19.08 — Dependency ও image audit
- **DoD:** cargo-audit, npm audit আর Trivy-তে কোনো critical বা high সমস্যা নেই। SBOM তৈরি হয়েছে।

### T19.09 — বাইরের security review
- **কী করতে হবে:** স্বাধীন pentest (PDF-এর সুপারিশ)।
- **DoD:** Critical বা high finding ঠিক করা হয়েছে।

### T19.10 — Documentation
- **কী করতে হবে:** Install guide, admin guide, client guide (বাংলা + English), API docs, runbook (incident, key rotation, restore)।
- **DoD:** Docs site publish হয়েছে।

### T19.11 — Release pipeline
- **কী করতে হবে:** Tag → reproducible build → signed manifest → artifact → changelog → staging-এ deploy → smoke test → production।
- **DoD:** v1.0.0 release হয়েছে।

### T19.12 — Production acceptance checklist (license PDF ১৭ নম্বর অংশ)
- **DoD:** PDF-এর ১২টা আইটেম যাচাই আর সই করা হয়েছে।
