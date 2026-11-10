# Phase 07 — Databases (MySQL/MariaDB, PostgreSQL)

**লক্ষ্য:** Client database ও user তৈরি করবেন, import করবেন, আর phpMyAdmin-এ SSO দিয়ে ঢুকবেন। MVP-তে MySQL (shared), PostgreSQL পরে।

### T07.01 — Shared DB server (node-এ)
- **কী করতে হবে:** Node-এ একটা MariaDB container (hardened, শুধু tenant network থেকে পৌঁছানো যায়)। Root credential agent-এর secret store-এ।
- **DoD:** Tenant container থেকে সংযোগ হয়, বাইরে থেকে হয় না।

### T07.02 — databases, db_users, grants migration
- **কী করতে হবে:** নামের নিয়ম: `<tenant>_<name>` prefix বাধ্যতামূলক (setting)। সংরক্ষিত নাম নিষেধ।
- **DoD:** Unit test আছে।

### T07.03 — Agent: CreateDatabase, DropDatabase
- **কী করতে হবে:** Parameterized SQL (identifier quote করা)। কখনো string concat নয়।
- **DoD:** Injection test (`a; DROP …`) প্রত্যাখ্যাত হয়।

### T07.04 — Agent: UpsertDbUser, GrantDb
- **কী করতে হবে:** Password নিজে থেকে তৈরি বা client দেয়। Strength যাচাই। Host `%` নয়, tenant network থেকেই।
- **DoD:** User শুধু নিজের DB দেখতে পায়।

### T07.05 — Remote access
- **কী করতে হবে:** `remote_db_hosts` (CIDR)। Firewall আর grant দুটোই আপডেট হয়।
- **DoD:** যোগ করা IP থেকে সংযোগ হয়, অন্য IP থেকে হয় না।

### T07.06 — Database API ও limit
- **কী করতে হবে:** CRUD, size দেখানো (agent পর্যায়ক্রমে সংগ্রহ করে), `max_databases` limit।
- **DoD:** Limit test আছে।

### T07.07 — SQL import
- **কী করতে হবে:** Upload (tus বা chunked), size সীমা (setting), gzip সমর্থন, background operation, progress।
- **DoD:** ৫০০MB dump import হয়, UI-তে progress দেখায়।

### T07.08 — SQL export (download)
- **DoD:** gzip করা dump, signed download link (৫ মিনিট মেয়াদ)।

### T07.09 — phpMyAdmin SSO
- **কী করতে হবে:** Node-এ phpMyAdmin container। Control plane এক-বারের token দেয় → signon script → সেই DB user হিসেবে login।
- **DoD:** Client শুধু নিজের DB দেখেন। Token দ্বিতীয়বার কাজ করে না।

### T07.10 — Database UI (client)
- **কী করতে হবে:** DB তালিকা, user তালিকা, grant matrix, import, export, phpMyAdmin বোতাম।
- **DoD:** E2E test আছে।

### T07.11 — Admin DB overview
- **DoD:** Node অনুযায়ী DB সংখ্যা আর size, বড় DB-র তালিকা দেখা যায়।

### T07.12 — (v2) PostgreSQL
- **কী করতে হবে:** একই কাজ PostgreSQL-এর জন্য (`db-postgres` module), pgAdmin SSO।
- **DoD:** একই test suite PostgreSQL-এ pass করে।

### T07.13 — (v2) Dedicated DB container
- **কী করতে হবে:** Plan-এ `db_isolation = dedicated` থাকলে tenant-এর নিজের DB container।
- **DoD:** Plan বদলালে migration operation চলে (dump → restore)।
