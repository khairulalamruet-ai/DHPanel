# Phase 11 — Security: WAF, Firewall, Malware, Security Center

### T11.01 — Coraza WAF সহ Caddy build
- **কী করতে হবে:** Caddy + coraza-caddy module, OWASP CRS। Image pin ও scan।
- **DoD:** SQLi বা XSS test request block হয়।

### T11.02 — WAF policy model
- **কী করতে হবে:** `waf_policies`: global, tenant আর domain scope। Mode `off/detect/block`। Disabled rule ID-র তালিকা।
- **DoD:** Domain-এর policy global-কে override করে।

### T11.03 — Agent: SetWafPolicy
- **DoD:** Policy বদলালে Caddy route-এ প্রয়োগ হয়, restart ছাড়াই।

### T11.04 — WAF event log ও UI
- **কী করতে হবে:** Block হওয়া request-এর তালিকা (rule, IP, URI)। "এই rule এই domain-এ বন্ধ করো" বোতাম (false positive-এর জন্য)।
- **DoD:** UI থেকে rule বন্ধ করা যায়।

### T11.05 — User-agent blacklist
- **DoD:** Admin তালিকা সম্পাদনা করেন, আর মিলে গেলে 403।

### T11.06 — Firewall model (nftables)
- **কী করতে হবে:** Node firewall rule: অনুমোদিত পোর্ট, IP block বা allow, রেট limit। priv-helper `SetFirewallRules` atomic ভাবে প্রয়োগ করে (`nft -f`)।
- **DoD:** ভুল ruleset দিলে আগেরটা থাকে, আর SSH lockout হয় না (panel IP আর SSH সবসময় allowlist-এ)।

### T11.07 — Brute-force সুরক্ষা
- **কী করতে হবে:** Panel login, SFTP আর mail auth ব্যর্থতা থেকে auto IP block (fail2ban-এর ধারণা, agent-এ)।
- **DoD:** ১০ বার ব্যর্থ হলে IP ১ ঘণ্টা block থাকে।

### T11.08 — Firewall UI (admin) ও IP block (client)
- **DoD:** Client নিজের site-এর জন্য IP block করতে পারেন (Caddy পর্যায়ে)।

### T11.09 — ClamAV malware scan
- **কী করতে হবে:** Node-এ ClamAV container। Agent `ScanPath` (tenant scope)। Scheduled আর on-demand। ফলাফল আর quarantine।
- **DoD:** EICAR test ফাইল ধরা পড়ে।

### T11.10 — Website vulnerability scan
- **কী করতে হবে:** WordPress core, plugin আর theme-এর version যাচাই (WPScan DB বা নিজস্ব feed)।
- **DoD:** পুরনো plugin থাকলে সতর্কতা দেখায়।

### T11.11 — Security Center UI
- **কী করতে হবে:** Admin: সাম্প্রতিক login ব্যর্থতা, WAF block, malware ঘটনা, 2FA ছাড়া admin, মেয়াদোত্তীর্ণ cert, পুরনো image।
- **DoD:** প্রতিটা সমস্যার পাশে "ঠিক করো" link আছে।

### T11.12 — Container image update ও CVE scan
- **কী করতে হবে:** Tenant আর core image-এর নতুন digest যাচাই, Trivy scan ফলাফল, "update" operation (rolling)।
- **DoD:** Update-এর সময় site ৫ সেকেন্ডের বেশি বন্ধ থাকে না।
