# Phase 06 — Web Server, VHost, PHP

**লক্ষ্য:** Tenant-এর website nginx + PHP-FPM-এ চলবে। PHP version আর ini বদলানো যাবে। Milestone M2-এর মূল অংশ।

### T06.01 — Tenant web image
- **কী করতে হবে:** নিজস্ব hardened image: nginx (non-root, 8080), আর PHP-FPM ৮.১–৮.৪ (প্রতিটা আলাদা tag)। ভাল ডিফল্ট extension। Digest pin। Trivy scan।
- **DoD:** Image-এ critical CVE নেই। Non-root চলে।

### T06.02 — Image registry ও allowlist
- **কী করতে হবে:** `images` config: নাম → digest। Agent শুধু allowlist-এর image চালায়।
- **DoD:** তালিকার বাইরের image চালাতে চাইলে প্রত্যাখ্যান করে।

### T06.03 — VHost template engine
- **কী করতে হবে:** `vhost_templates` (nginx)। Template variable (docroot, php socket, domain, alias)। Rendering control plane-এ হয়, agent শুধু ফাইল লেখে।
- **DoD:** Snapshot test: template থেকে প্রত্যাশিত config তৈরি হয়।

### T06.04 — Agent: AttachDomain ও DetachDomain
- **কী করতে হবে:** VHost ফাইল লেখা (tenant volume-এর conf dir-এ), `nginx -t` (container-এর ভেতরে, typed op হিসেবে), reload। ব্যর্থ হলে আগের config ফিরিয়ে আনা।
- **DoD:** ভুল config দিলে আগেরটা ফিরে আসে আর site বন্ধ হয় না।

### T06.05 — Agent: SetDocroot, SetVhostPolicy
- **কী করতে হবে:** Docroot tenant root-এর ভেতরে কিনা যাচাই। Rate limit, HSTS, custom header।
- **DoD:** `../` দিয়ে বাইরে গেলে প্রত্যাখ্যান করে।

### T06.06 — VHost দেখা ও সম্পাদনা (advanced)
- **কী করতে হবে:** Client-এর `edit_vhost` feature থাকলে নিরাপদ অংশ (custom directive block) সম্পাদনা করা যায়। নিষিদ্ধ directive-এর তালিকা থাকবে (`include /`, `alias /etc` ইত্যাদি)।
- **DoD:** নিষিদ্ধ directive দিলে 422 আসে।

### T06.07 — Admin vhost template UI
- **কী করতে হবে:** Monaco editor, preview, version history।
- **DoD:** Template বদলালে "সব domain-এ প্রয়োগ" operation চালানো যায়।

### T06.08 — PHP version API
- **কী করতে হবে:** Tenant-এর ডিফল্ট version আর domain-প্রতি version। Admin সমর্থিত version তালিকা ঠিক করেন।
- **DoD:** Version বদলালে container বা socket বদলায় আর site চলে।

### T06.09 — Agent: SetPhpVersion
- **DoD:** Integration test: `phpinfo()`-এ নতুন version দেখায়।

### T06.10 — php.ini ও options
- **কী করতে হবে:** অনুমোদিত key-এর allowlist (memory_limit, upload_max_filesize, max_execution_time…) আর plan অনুযায়ী সর্বোচ্চ সীমা।
- **DoD:** সীমার বাইরের মান দিলে 422 আসে।

### T06.11 — PHP UI (client)
- **কী করতে হবে:** Version dropdown, ini form, "ডিফল্টে ফেরত" বোতাম।
- **DoD:** E2E test আছে।

### T06.12 — Web service restart (client)
- **কী করতে হবে:** OpenPanel `services` feature-এর মতো: client নিজের nginx বা php restart করতে পারেন (rate limited)।
- **DoD:** প্রতি মিনিটে ৩ বারের বেশি করলে 429 আসে।

### T06.13 — Suspended ও error পেজ template
- **কী করতে হবে:** Admin সম্পাদনাযোগ্য suspended domain, suspended user আর 404/502 পেজ।
- **DoD:** Suspend করলে সঠিক পেজ আসে।

### T06.14 — Temporary preview link
- **কী করতে হবে:** DNS ঠিক হওয়ার আগে `<random>.preview.<panel-domain>` দিয়ে site দেখা (মেয়াদসহ)।
- **DoD:** মেয়াদ শেষে link কাজ করে না।
