# Phase 04 — Plans, Feature Set, Tenants, Users

**লক্ষ্য:** Admin plan বানাবেন, user বানাবেন, আর সেই user-এর জন্য hardened container stack তৈরি হবে (Milestone M1)।

### T04.01 — plans ও feature_sets migration
- **DoD:** টেবিল তৈরি হয়েছে। ৪টা preset seed করা: `default`, `basic`, `dns_only`, `db_only`।

### T04.02 — Feature set API ও UI
- **কী করতে হবে:** CRUD। সব module-এর feature key checkbox তালিকায় (category অনুযায়ী) দেখানো।
- **DoD:** UI থেকে feature set তৈরি ও সম্পাদনা করা যায়।

### T04.03 — Plan API
- **কী করতে হবে:** CRUD। Resource (cpu, memory, pids, disk, inodes, bandwidth), module limit (manifest-এর `[limits]` থেকে), feature set, isolation।
- **DoD:** Plan-এর সীমা node-এর সর্বোচ্চ সীমা পার হলে 422 আসে।

### T04.04 — Plan UI
- **কী করতে হবে:** তালিকা, তৈরি ও সম্পাদনা form। "Unlimited" toggle। কতজন user এই plan-এ আছে দেখানো।
- **DoD:** Playwright test আছে।

### T04.05 — tenants migration ও domain model
- **কী করতে হবে:** `tenants` টেবিল। Username নিয়ম (a-z0-9, ৩–৩২, সংরক্ষিত নাম নিষেধ)। UID বরাদ্দ (node অনুযায়ী range থেকে)।
- **DoD:** Domain unit test আছে।

### T04.06 — Tenant container template
- **কী করতে হবে:** Stack-এর বর্ণনা (web + php-fpm, শুরুতে)। Image digest allowlist। Hardened default (non-root, cap_drop ALL, no-new-privileges, seccomp, pids/mem/cpu)।
- **DoD:** Template ফাইল review হয়েছে আর security checklist pass করে।

### T04.07 — Agent: CreateTenant
- **কী করতে হবে:** priv-helper দিয়ে system user/UID তৈরি, tenant root `/srv/tenants/<id>` (0750), volume, network।
- **DoD:** Integration test: ফোল্ডার, UID আর network তৈরি হয়।

### T04.08 — Agent: SetStorageQuota
- **কী করতে হবে:** priv-helper → XFS project quota বা ext4 quota। Quota সমর্থিত না হলে capability-তে জানাবে।
- **DoD:** Quota-র বেশি লিখলে লেখা ব্যর্থ হয়।

### T04.09 — Agent: ProvisionTenantStack
- **কী করতে হবে:** Template অনুযায়ী container তৈরি ও চালু করা। Health check।
- **DoD:** Container non-root হিসেবে চলে (`docker inspect` দিয়ে যাচাই)।

### T04.10 — Agent: SetResourceLimits
- **কী করতে হবে:** চলমান container update (cpu, mem, pids)।
- **DoD:** Plan বদলালে limit বদলায়।

### T04.11 — Agent: Suspend, Resume, Delete
- **কী করতে হবে:** Suspend করলে container stop হয়, আর vhost একটা "suspended" পেজে যায় (website-এর ডেটা থাকে)। Delete: soft delete + ৭ দিন পরে purge (configurable)।
- **DoD:** Suspend করলে site-এ suspended পেজ আসে। Resume করলে ঠিক হয়ে যায়।

### T04.12 — User (client account) API
- **কী করতে হবে:** `POST /admin/users` = account + tenant + operation (একটা transaction-এ)। Read, list (filter, search), update, change plan, suspend, unsuspend, delete, password reset।
- **DoD:** তৈরির পর `operation_id` আসে। Succeeded হলে user active হয়।

### T04.13 — Login as user (impersonation)
- **কী করতে হবে:** এক-বারের signed token (৬০ সেকেন্ড), client app-এ session তৈরি, উপরে "Admin হিসেবে দেখছেন" banner, audit-এ দুজনেরই নাম।
- **DoD:** Token দ্বিতীয়বার কাজ করে না।

### T04.14 — Skeleton ফাইল
- **কী করতে হবে:** নতুন tenant-এর জন্য default ফাইল (index.html ইত্যাদি)। Admin সম্পাদনা করতে পারেন (OpenPanel-এর `settings/defaults/files`-এর মতো)।
- **DoD:** নতুন user-এর docroot-এ ফাইলগুলো থাকে।

### T04.15 — User তালিকা UI
- **কী করতে হবে:** Table (username, email, plan, node, অবস্থা, disk ব্যবহার), search, filter, bulk কাজ (suspend, change plan)।
- **DoD:** ১০০০ user-এও pagination মসৃণ চলে।

### T04.16 — User তৈরি wizard
- **কী করতে হবে:** ধাপ: account → plan → node (auto বা নির্দিষ্ট) → primary domain (ঐচ্ছিক) → নিশ্চিত করা। Operation progress দেখানো।
- **DoD:** E2E test: user তৈরি → container চলছে।

### T04.17 — User detail পেজ
- **কী করতে হবে:** Tab: overview, resource, domain, database, log, activity। Action বোতাম।
- **DoD:** সব tab-এ ডেটা আসে (পরের phase-এর tab-এ placeholder থাকবে)।

### T04.18 — Client dashboard কঙ্কাল
- **কী করতে হবে:** Client app-এর home: plan-এর সারাংশ, resource bar, shortcut (OpenPanel `summary` আর `favorites`-এর মতো)।
- **DoD:** Client login করলে নিজের তথ্য দেখেন।
