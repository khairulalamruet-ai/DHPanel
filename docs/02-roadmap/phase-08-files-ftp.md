# Phase 08 — File Manager, FTP/SFTP

**লক্ষ্য:** Browser থেকে নিরাপদে ফাইল পরিচালনা, আর FTP/SFTP account।

### T08.01 — Agent: নিরাপদ path resolver
- **কী করতে হবে:** সব file op-এ `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)` (Linux 5.6+), অথবা fallback (canonicalize + prefix যাচাই + symlink নিষেধ)। Tenant UID হিসেবে কাজ (priv-helper বা setfsuid)।
- **DoD:** Fuzz test: `../`, symlink, `/proc` — সব প্রত্যাখ্যাত হয়।

### T08.02 — Agent: ListDir, Stat
- **DoD:** Pagination (প্রতি পেজে কত ফাইল, setting থেকে) কাজ করে।

### T08.03 — Agent: ReadFile, WriteFile
- **কী করতে হবে:** Size সীমা (`filemanager_edit_size`)। Atomic write (temp + rename)।
- **DoD:** সীমার বেশি হলে 413 আসে।

### T08.04 — Agent: Move, Copy, Delete (trash)
- **কী করতে হবে:** Delete করলে `.trash/`-এ যায়। Auto-purge setting।
- **DoD:** Trash থেকে restore করা যায়।

### T08.05 — Agent: Archive, Extract
- **কী করতে হবে:** zip, tar.gz। Zip-slip প্রতিরোধ। সময় ও size সীমা।
- **DoD:** ক্ষতিকর archive (`../../etc/passwd` entry) প্রত্যাখ্যাত হয়।

### T08.06 — Agent: Chmod, FixPermissions
- **কী করতে হবে:** শুধু tenant UID। setuid/setgid bit নিষেধ।
- **DoD:** `4755` দিলে প্রত্যাখ্যান করে।

### T08.07 — Upload ও download
- **কী করতে হবে:** Chunked/resumable upload (tus protocol) সরাসরি agent-এ (control plane signed token দেয়) অথবা control plane proxy। Download-এর জন্য signed URL।
- **DoD:** ২GB ফাইল upload হয়, মাঝপথে বন্ধ হলে আবার চালু করা যায়।

### T08.08 — File API
- **কী করতে হবে:** Control plane-এর client API, যেটা agent op-এ রূপান্তর করে। Audit (ফাইল লেখা, মুছে ফেলা)।
- **DoD:** Integration test আছে।

### T08.09 — File manager UI
- **কী করতে হবে:** Tree + তালিকা view, breadcrumb, multi-select, drag-drop upload, context menu, Monaco editor (syntax highlight), image preview, archive ও extract, permission dialog, trash।
- **DoD:** E2E test: upload → সম্পাদনা → মুছে ফেলা → restore।

### T08.10 — Disk ও inode usage
- **কী করতে হবে:** Agent পর্যায়ক্রমে `du` সমতুল্য হিসাব করে (ionice)। ফোল্ডার অনুযায়ী ভাঙা হিসাব।
- **DoD:** UI-তে সবচেয়ে বড় ১০টা ফোল্ডার দেখা যায়।

### T08.11 — SFTP (প্রস্তাবিত ডিফল্ট)
- **কী করতে হবে:** Node-এ chroot SFTP (যেমন SFTPGo container), tenant-প্রতি virtual user, home = tenant root।
- **DoD:** SFTP দিয়ে শুধু নিজের ফাইল দেখা যায়।

### T08.12 — FTP account (FTPS)
- **কী করতে হবে:** `ftp_accounts` CRUD। Sub-folder home, quota, password। Plain FTP ডিফল্টে বন্ধ (setting)।
- **DoD:** তৈরি করা account দিয়ে FTPS login হয়।

### T08.13 — FTP UI
- **কী করতে হবে:** Account তালিকা, তৈরি, password বদলানো, সংযোগের তথ্য (host, port, protocol)।
- **DoD:** E2E test আছে।
