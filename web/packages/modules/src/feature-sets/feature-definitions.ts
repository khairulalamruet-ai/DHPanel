import { FeatureDefinition, FeatureCategoryKey } from "./types";

export const AVAILABLE_FEATURES: FeatureDefinition[] = [
  // Web & Domains
  {
    key: "domains",
    title: { en: "Custom Domains", bn: "কাস্টম ডোমেইন" },
    description: {
      en: "Add and map custom domain names to virtual host directories",
      bn: "ভার্চুয়াল হোস্ট ডিরেক্টরিতে কাস্টম ডোমেইন যুক্ত ও ম্যাপ করা",
    },
    category: "web",
    module: "domains",
  },
  {
    key: "redirects",
    title: { en: "URL Redirects", bn: "ইউআরএল রিডাইরেক্ট" },
    description: {
      en: "Create 301 permanent and 302 temporary domain redirects",
      bn: "৩০১ স্থায়ী এবং ৩০২ সাময়িক ডোমেইন রিডাইরেকশন তৈরি",
    },
    category: "web",
    module: "domains",
  },
  {
    key: "docroot",
    title: { en: "Custom Docroots", bn: "কাস্টম ডকরুট" },
    description: {
      en: "Specify custom document roots (e.g. /public, /dist) per domain",
      bn: "প্রতি ডোমেইনের জন্য কাস্টম ডকুমেন্ট রুট ডিরেক্টরি নির্ধারণ",
    },
    category: "web",
    module: "domains",
  },
  {
    key: "temporary_links",
    title: { en: "Preview URLs", bn: "প্রিভিউ লিংক" },
    description: {
      en: "Access websites before global DNS propagation via staging preview URLs",
      bn: "ডিএনএস চালুর পূর্বে প্রিভিউ লিংকের মাধ্যমে ওয়েবসাইট ব্রাউজ করা",
    },
    category: "web",
    module: "domains",
  },
  {
    key: "ssl_letsencrypt",
    title: { en: "Auto Let's Encrypt SSL", bn: "স্বয়ংক্রিয় লেটস এনক্রিপ্ট এসএসএল" },
    description: {
      en: "Automated issuance and 90-day renewal of free SSL/TLS certificates",
      bn: "বিনামূল্যে স্বয়ংক্রিয় এসএসএল সার্টিফিকেট ইস্যু ও নবায়ন",
    },
    category: "web",
    module: "ssl",
  },
  {
    key: "ssl_custom",
    title: { en: "Custom SSL Upload", bn: "কাস্টম এসএসএল আপলোড" },
    description: {
      en: "Upload enterprise commercial SSL certificates and private keys",
      bn: "বাণিজ্যিক এসএসএল সার্টিফিকেট ও প্রাইভেট কি আপলোড",
    },
    category: "web",
    module: "ssl",
  },

  // DNS
  {
    key: "dns_zones",
    title: { en: "DNS Zones", bn: "ডিএনএস জোন" },
    description: {
      en: "Manage dedicated DNS forward and reverse lookup zones",
      bn: "ডেডিকেটেড ডিএনএস জোন তৈরি ও পরিচালনা",
    },
    category: "dns",
    module: "dns",
  },
  {
    key: "dns_records",
    title: { en: "Record Editor", bn: "রেকর্ড এডিটর" },
    description: {
      en: "Configure A, AAAA, CNAME, MX, TXT, SRV, CAA DNS records",
      bn: "A, AAAA, CNAME, MX, TXT ইত্যাদি ডিএনএস রেকর্ড কনফিগারেশন",
    },
    category: "dns",
    module: "dns",
  },

  // Database
  {
    key: "db_mysql",
    title: { en: "MySQL Databases", bn: "মাইএসকিউএল ডাটাবেস" },
    description: {
      en: "Create MySQL / MariaDB databases and dedicated database users",
      bn: "MySQL / MariaDB ডাটাবেস ও ব্যবহারকারী তৈরি",
    },
    category: "db",
    module: "db-mysql",
  },
  {
    key: "db_remote",
    title: { en: "Remote Database Access", bn: "রিমোট ডাটাবেস অ্যাক্সেস" },
    description: {
      en: "Allow external IP addresses to connect directly to databases",
      bn: "বাহ্যিক আইপি ঠিকানা থেকে ডাটাবেসে সরাসরি সংযোগের অনুমতি",
    },
    category: "db",
    module: "db-mysql",
  },
  {
    key: "phpmyadmin",
    title: { en: "phpMyAdmin Web Interface", bn: "phpMyAdmin ওয়েব ইন্টারফেস" },
    description: {
      en: "Single sign-on access to phpMyAdmin web database manager",
      bn: "ওয়েব-ভিত্তিক ডাটাবেস ব্যবস্থাপনার জন্য phpMyAdmin প্রবেশাধিকার",
    },
    category: "db",
    module: "db-mysql",
  },

  // Files
  {
    key: "file_manager",
    title: { en: "Web File Manager", bn: "ওয়েব ফাইল ম্যানেজার" },
    description: {
      en: "Browser-based file explorer, code editor, zip archive extract",
      bn: "ব্রাউজার-ভিত্তিক ফাইল ব্রাউজার, টেক্সট এডিটর এবং জিপ ফাইল আনপ্যাক",
    },
    category: "files",
    module: "files",
  },
  {
    key: "ftp_accounts",
    title: { en: "FTP Accounts", bn: "এফটিপি অ্যাকাউন্ট" },
    description: {
      en: "Create chrooted FTPS / SFTP user accounts for file transfers",
      bn: "ফাইল আদান-প্রদানের জন্য সুরক্ষিত FTPS / SFTP অ্যাকাউন্ট তৈরি",
    },
    category: "files",
    module: "ftp",
  },

  // Mail
  {
    key: "email_inbox",
    title: { en: "Email Inboxes", bn: "ইমেইল ইনবক্স" },
    description: {
      en: "IMAP / POP3 email accounts with spam filtering and quota limits",
      bn: "স্প্যাম ফিল্টার ও কোটা সীমা সহ IMAP / POP3 ইমেইল অ্যাকাউন্ট",
    },
    category: "mail",
    module: "mail",
  },
  {
    key: "email_forwarding",
    title: { en: "Email Forwarders", bn: "ইমেইল ফরওয়ার্ডার" },
    description: {
      en: "Forward incoming domain messages to external email addresses",
      bn: "আগত ইমেইল স্বয়ংক্রিয়ভাবে বাইরের ইমেইলে পাঠিয়ে দেওয়া",
    },
    category: "mail",
    module: "mail",
  },
  {
    key: "webmail",
    title: { en: "Webmail Portal", bn: "ওয়েবমেইল পোর্টাল" },
    description: {
      en: "Roundcube / Snappy webmail client interface for checking email",
      bn: "ব্রাউজার থেকে ইমেইল দেখার জন্য ওয়েবমেইল পোর্টাল",
    },
    category: "mail",
    module: "mail",
  },

  // Security
  {
    key: "ip_blocker",
    title: { en: "IP Access Blocker", bn: "আইপি ব্লকার" },
    description: {
      en: "Block specific IP ranges or allow only whitelisted IP addresses",
      bn: "নির্দিষ্ট আইপি অ্যাড্রেস ব্লক করা বা অনুমোদিত তালিকা তৈরি",
    },
    category: "security",
    module: "security",
  },
  {
    key: "modsecurity",
    title: { en: "Web Application Firewall (WAF)", bn: "ওয়েব অ্যাপ্লিকেশন ফায়ারওয়াল (WAF)" },
    description: {
      en: "ModSecurity OWASP Core Rule Set rules and real-time threat blocking",
      bn: "ModSecurity ও OWASP নিয়মাবলীর মাধ্যমে রিয়েল-টাইম সুরক্ষা",
    },
    category: "security",
    module: "security",
  },
];

export const CATEGORY_ORDER: FeatureCategoryKey[] = [
  "web",
  "dns",
  "db",
  "files",
  "mail",
  "security",
];
