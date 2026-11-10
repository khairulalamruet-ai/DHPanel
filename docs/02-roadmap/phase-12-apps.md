# Phase 12 — Apps: WordPress, Autoinstaller, Node/PM2, Container

### T12.01 — apps টেবিল ও app model
- **DoD:** একটা domain বা path-এ কোন app আছে, সেটা ট্র্যাক হয়।

### T12.02 — WordPress installer
- **কী করতে হবে:** DB তৈরি → official zip (checksum যাচাই) → wp-config (নিরাপদ salt) → admin user → ঐচ্ছিক plugin। Tenant container-এর ভেতরে WP-CLI চলে (typed op `RunWpCli` সীমিত subcommand allowlist নিয়ে)।
- **DoD:** ২ মিনিটের মধ্যে চালু WordPress পাওয়া যায়।

### T12.03 — WordPress manager
- **কী করতে হবে:** Version, plugin আর theme তালিকা, update, maintenance mode, admin password reset, auto-login (এক-বারের link), staging clone।
- **DoD:** Plugin update UI থেকে হয়।

### T12.04 — WordPress scan ও hardening
- **কী করতে হবে:** File permission, `xmlrpc` বন্ধ, debug বন্ধ, core checksum যাচাই।
- **DoD:** পরিবর্তিত core ফাইল ধরা পড়ে।

### T12.05 — Autoinstaller catalog
- **কী করতে হবে:** App definition format (YAML: নাম, version, উৎস URL + checksum, DB দরকার কিনা, ধাপ)। প্রথমে ৫টা: WordPress, Joomla, Drupal, Laravel skeleton, static site।
- **DoD:** Catalog-এ নতুন app যোগ করতে কোড লাগে না।

### T12.06 — Autoinstaller UI
- **DoD:** Catalog grid, install wizard, installed app তালিকা।

### T12.07 — (v2) Node.js app (PM2-এর মতো)
- **কী করতে হবে:** Node runtime container (version বেছে নেওয়া), start command, env var, Caddy reverse proxy route।
- **DoD:** একটা Express app domain-এ চলে।

### T12.08 — (v2) Python app
- **DoD:** Gunicorn বা Uvicorn দিয়ে একই ধরনের ব্যবস্থা।

### T12.09 — (v2) Git deploy
- **কী করতে হবে:** Repo URL + deploy key, branch, webhook, build command (sandboxed), atomic switch।
- **DoD:** Push করলে নিজে থেকে deploy হয়।

### T12.10 — Cache add-on (Redis, Memcached)
- **কী করতে হবে:** Tenant-এর নিজস্ব Redis বা Memcached container (মেমরি সীমা plan থেকে)। সংযোগের তথ্য দেখানো।
- **DoD:** WordPress object cache কাজ করে।

### T12.11 — (v2) Varnish
- **DoD:** Domain-প্রতি চালু বা বন্ধ, purge বোতাম।

### T12.12 — (v2) Client custom container
- **কী করতে হবে:** OpenPanel-এর `docker` feature-এর মতো: অনুমোদিত image catalog থেকে client নিজের container চালান, hardened, সীমাসহ।
- **DoD:** Privileged বা host mount চাইলে প্রত্যাখ্যাত হয়।
