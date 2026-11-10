# Phase 14 — Reseller (v2)

### T14.01 — Reseller model
- **কী করতে হবে:** `resellers` (account + মোট সীমা: user সংখ্যা, disk, bandwidth, অনুমোদিত plan)। Parent-child সম্পর্ক।
- **DoD:** Domain model test আছে।

### T14.02 — Reseller package (admin)
- **DoD:** OpenPanel-এর `/plans/resellers`-এর মতো CRUD।

### T14.03 — Reseller scoping
- **কী করতে হবে:** Reseller শুধু নিজের user দেখবেন (repository স্তরে `reseller_id` filter)। RBAC role `reseller`।
- **DoD:** Security test: অন্য reseller-এর user-এ ঢুকলে 404 আসে।

### T14.04 — Oversell ও সীমা যাচাই
- **DoD:** মোট সীমা পার হলে নতুন user বানানো যায় না।

### T14.05 — Reseller UI
- **কী করতে হবে:** Admin shell-এর সীমিত সংস্করণ: user, plan (নিজের), branding (নিজস্ব logo ও রং, যা তাঁর client দেখবেন)।
- **DoD:** E2E test আছে।

### T14.06 — Reseller activity ও report
- **DoD:** Admin reseller অনুযায়ী ব্যবহার আর activity দেখেন।

### T14.07 — Reseller suspend
- **DoD:** Reseller suspend হলে তাঁর সব user-এর panel login বন্ধ হয় (site চলতে থাকে বা বন্ধ হয়, policy অনুযায়ী)।
