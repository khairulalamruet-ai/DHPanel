# Phase 13 — Email (v2)

Email সবচেয়ে জটিল আর ঝুঁকিপূর্ণ অংশ (spam, IP reputation, deliverability), তাই এটা আলাদা phase-এ রাখা হয়েছে।

### T13.01 — Mail stack ADR
- **কী করতে হবে:** docker-mailserver, Stalwart (Rust) নাকি নিজের Postfix + Dovecot + Rspamd, তুলনা করে সিদ্ধান্ত। প্রস্তাব: **Stalwart** (Rust, API, JMAP), নইলে docker-mailserver।
- **DoD:** ADR merge হয়েছে।

### T13.02 — Mail server provision (node বা আলাদা mail node)
- **DoD:** SMTP/IMAP/Submission চালু হয়, TLS (Caddy-র cert শেয়ার করে বা নিজস্ব ACME)।

### T13.03 — mail_domains, mailboxes, aliases migration
- **DoD:** টেবিল ও domain model তৈরি।

### T13.04 — DNS নিজে থেকে: MX, SPF, DKIM, DMARC
- **কী করতে হবে:** Domain-এ mail চালু করলে DKIM key তৈরি হয় আর record যোগ হয়।
- **DoD:** mail-tester-এ ৯/১০ বা তার বেশি স্কোর।

### T13.05 — Mailbox CRUD + quota
- **DoD:** Quota পূর্ণ হলে bounce হয়।

### T13.06 — Forwarder, alias, catch-all, autoresponder
- **DoD:** প্রতিটার integration test আছে।

### T13.07 — Outbound rate limit ও abuse নিয়ন্ত্রণ
- **কী করতে হবে:** Tenant অনুযায়ী ঘণ্টায় কয়টা mail, queue পর্যবেক্ষণ, সীমা পার হলে hold আর notify।
- **DoD:** Spam ঝড় নিজে থেকে থামে।

### T13.08 — Spam filter (Rspamd বা বিল্ট-ইন)
- **DoD:** GTUBE test spam হিসেবে চিহ্নিত হয়।

### T13.09 — Webmail (Roundcube বা SnappyMail) SSO
- **DoD:** Client এক ক্লিকে webmail-এ ঢোকেন।

### T13.10 — Mail queue UI (admin)
- **DoD:** Queue দেখা, flush করা আর delete করা যায়।

### T13.11 — Deliverability report
- **কী করতে হবে:** Blacklist (RBL) যাচাই, DNS record যাচাই, DMARC report parse করা।
- **DoD:** সমস্যা থাকলে UI-তে সতর্কতা দেখায়।

### T13.12 — Email UI (client)
- **DoD:** Mailbox, forwarder, autoresponder, client config তথ্য (IMAP/SMTP setting)।
