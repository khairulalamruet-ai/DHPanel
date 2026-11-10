# Phase 16 — Multi-node ও Cluster (v2)

Phase 02-এ agent আর enrollment আগে থেকেই multi-node সমর্থন করে। এই phase-এ আসবে scheduling, DNS cluster আর ক্ষমতা ব্যবস্থাপনা।

### T16.01 — Node capacity model
- **কী করতে হবে:** বরাদ্দ করা বনাম আসল ব্যবহার (CPU, RAM, disk), oversell ratio (setting)।
- **DoD:** Admin UI-তে node-প্রতি পূর্ণতা (%) দেখা যায়।

### T16.02 — Placement scheduler
- **কী করতে হবে:** নতুন tenant কোন node-এ যাবে: নীতি (least-loaded, region, tag), node drain বা maintenance mode।
- **DoD:** Drain-এ থাকা node-এ নতুন tenant যায় না।

### T16.03 — Node group, region আর tag
- **DoD:** Plan একটা node group-এ বাঁধা যায়।

### T16.04 — DNS cluster
- **কী করতে হবে:** একাধিক PowerDNS (primary/secondary, অথবা API replication)। Admin UI-তে DNS server যোগ করা।
- **DoD:** একটা DNS server বন্ধ থাকলেও resolve হয়।

### T16.05 — Shared IP pool ও dedicated IP
- **DoD:** IP বরাদ্দ আর মুক্ত করা, node অনুযায়ী।

### T16.06 — Apply-to-all operation
- **কী করতে হবে:** OpenPanel `/cluster/applyall`-এর মতো: setting বা template সব node-এ (batch, rate-limited, ব্যর্থ হলে থামে)।
- **DoD:** ১০টা node-এ rolling apply হয়।

### T16.07 — Node health ও failover alert
- **DoD:** Node offline হলে admin ১ মিনিটে জানতে পারেন। Tenant তালিকায় "node offline" দেখায়।

### T16.08 — Agent self-update (signed)
- **কী করতে হবে:** Phase 17-এর signed release manifest দিয়ে agent binary update, rollback।
- **DoD:** ভুল signature-এর binary প্রত্যাখ্যাত হয়।
