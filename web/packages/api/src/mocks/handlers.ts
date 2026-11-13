import { http, HttpResponse } from "msw";

/**
 * MSW Mock Handlers
 * Real backend paths from repo-root openapi.json.
 */
let mfaEnabled = false;
const backupCodes = [
  "a1b2-c3d4",
  "e5f6-g7h8",
  "i9j0-k1l2",
  "m3n4-o5p6",
  "q7r8-s9t0",
  "u1v2-w3x4",
  "y5z6-a7b8",
  "c9d0-e1f2",
  "g3h4-i5j6",
  "k7l8-m9n0",
];

const getInitialSessions = () => [
  {
    id: "00000000-0000-0000-0000-000000000001",
    ip: "192.168.1.105",
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/130.0",
    created_at: new Date(Date.now() - 3600 * 1000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 7).toISOString(),
    current: true,
  },
  {
    id: "00000000-0000-0000-0000-000000000002",
    ip: "203.0.113.42",
    user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Firefox/131.0",
    created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
    last_seen_at: new Date(Date.now() - 3600 * 1000 * 5).toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 4).toISOString(),
    current: false,
  },
  {
    id: "00000000-0000-0000-0000-000000000003",
    ip: "198.51.100.12",
    user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) Mobile Safari/604.1",
    created_at: new Date(Date.now() - 86400 * 1000 * 7).toISOString(),
    last_seen_at: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
    expires_at: new Date(Date.now() + 86400 * 1000 * 1).toISOString(),
    current: false,
  },
];
let sessions = getInitialSessions();

const loginEvents = [
  {
    id: "evt_01",
    ip_address: "192.168.1.105",
    user_agent: "Chrome 130.0 (macOS)",
    status: "success" as const,
    created_at: new Date(Date.now() - 600 * 1000).toISOString(),
  },
  {
    id: "evt_02",
    ip_address: "192.168.1.105",
    user_agent: "Chrome 130.0 (macOS)",
    status: "2fa_prompt" as const,
    created_at: new Date(Date.now() - 650 * 1000).toISOString(),
  },
  {
    id: "evt_03",
    ip_address: "185.220.101.5",
    user_agent: "Python-urllib/3.10",
    status: "failed" as const,
    created_at: new Date(Date.now() - 3600 * 1000 * 4).toISOString(),
  },
  {
    id: "evt_04",
    ip_address: "203.0.113.42",
    user_agent: "Firefox 131.0 (Windows)",
    status: "success" as const,
    created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
  },
];

export interface MockModuleItem {
  name: string;
  title: { en: string; bn: string };
  description: { en: string; bn: string };
  version: string;
  category: "core" | "web" | "db" | "files" | "mail" | "security" | "system";
  core: boolean;
  enabled: boolean;
  depends_on: string[];
  status: "active" | "disabled" | "error";
}

export const getInitialModules = (): MockModuleItem[] => [
  {
    name: "auth",
    title: { en: "Authentication", bn: "প্রমাণীকরণ" },
    description: {
      en: "User authentication, sessions, and multi-factor security",
      bn: "ব্যবহারকারী প্রমাণীকরণ, সেশন এবং টু-ফ্যাক্টর নিরাপত্তা",
    },
    version: "1.0.0",
    category: "core",
    core: true,
    enabled: true,
    depends_on: [],
    status: "active",
  },
  {
    name: "users",
    title: { en: "Users & RBAC", bn: "ব্যবহারকারী ও আরব্যাক" },
    description: {
      en: "User management and role-based access control",
      bn: "ব্যবহারকারী ব্যবস্থাপনা এবং ভূমিকা-ভিত্তিক অ্যাক্সেস নিয়ন্ত্রণ",
    },
    version: "1.0.0",
    category: "core",
    core: true,
    enabled: true,
    depends_on: ["auth"],
    status: "active",
  },
  {
    name: "settings",
    title: { en: "Settings", bn: "সেটিংস" },
    description: {
      en: "Global panel configuration and environment preferences",
      bn: "গ্লোবাল প্যানেল কনফিগারেশন এবং পরিবেশ সেটিংস",
    },
    version: "1.0.0",
    category: "core",
    core: true,
    enabled: true,
    depends_on: [],
    status: "active",
  },
  {
    name: "webserver",
    title: { en: "Web Server", bn: "ওয়েব সার্ভার" },
    description: {
      en: "Nginx / Traefik reverse proxy and vhost management",
      bn: "Nginx / Traefik রিভার্স প্রক্সি এবং ভার্চুয়াল হোস্ট পরিচালনা",
    },
    version: "1.0.0",
    category: "web",
    core: false,
    enabled: true,
    depends_on: [],
    status: "active",
  },
  {
    name: "dns",
    title: { en: "DNS Manager", bn: "ডিএনএস ম্যানেজার" },
    description: {
      en: "PowerDNS / CoreDNS zone files and record management",
      bn: "PowerDNS / CoreDNS জোন ফাইল এবং রেকর্ড ব্যবস্থাপনা",
    },
    version: "1.0.0",
    category: "web",
    core: false,
    enabled: true,
    depends_on: [],
    status: "active",
  },
  {
    name: "domains",
    title: { en: "Domains", bn: "ডোমেইন" },
    description: {
      en: "Domain routing, virtual hosts, and docroots",
      bn: "ডোমেইন রাউটিং, ভার্চুয়াল হোস্ট এবং ডকরুট",
    },
    version: "1.0.0",
    category: "web",
    core: false,
    enabled: true,
    depends_on: ["webserver", "dns"],
    status: "active",
  },
  {
    name: "db-mysql",
    title: { en: "MySQL / MariaDB", bn: "মাইএসকিউএল / মারিয়াডিবি" },
    description: {
      en: "Database container provisioning and user grants",
      bn: "ডাটাবেস কন্টেইনার প্রভিশনিং এবং ব্যবহারকারী গ্রান্ট",
    },
    version: "1.0.0",
    category: "db",
    core: false,
    enabled: true,
    depends_on: [],
    status: "active",
  },
  {
    name: "files",
    title: { en: "File Manager", bn: "ফাইল ম্যানেজার" },
    description: {
      en: "Web-based file explorer and permissions editor",
      bn: "ওয়েব-ভিত্তিক ফাইল ব্রাউজার এবং পারমিশন এডিটর",
    },
    version: "1.0.0",
    category: "files",
    core: false,
    enabled: false,
    depends_on: [],
    status: "disabled",
  },
];

let modules = getInitialModules();

export const resetMockModules = () => {
  modules = getInitialModules();
};

export const getInitialSettingsStore = () => ({
  general: {
    title: { en: "General", bn: "সাধারণ" },
    description: {
      en: "Global panel identity, default language, and session preferences",
      bn: "গ্লোবাল প্যানেল পরিচিতি, ডিফল্ট ভাষা এবং সেশন পছন্দসমূহ",
    },
    schema: {
      type: "object" as const,
      title: "General Settings",
      required: ["panel_name", "support_email"],
      properties: {
        panel_name: {
          type: "string",
          title: "Panel Name",
          description: "Name displayed across headers, emails, and login pages",
          default: "DHPanel",
        },
        support_email: {
          type: "string",
          format: "email",
          title: "Support Contact Email",
          description: "Inquiries and customer issue reports will be directed here",
          default: "support@dhpanel.local",
        },
        session_timeout: {
          type: "integer",
          title: "Session Timeout (Minutes)",
          description: "Duration of idle user session before automatic logout",
          default: 60,
          minimum: 5,
          maximum: 1440,
        },
        maintenance_mode: {
          type: "boolean",
          title: "Maintenance Mode",
          description: "Temporarily pause user access for platform maintenance",
          default: false,
        },
        default_locale: {
          type: "string",
          title: "Default Language",
          enum: ["bn", "en"],
          enumNames: ["বাংলা", "English"],
          default: "bn",
        },
      },
    },
    values: {
      panel_name: "DHPanel",
      support_email: "support@dhpanel.local",
      session_timeout: 60,
      maintenance_mode: false,
      default_locale: "bn",
    },
  },
  security: {
    title: { en: "Security", bn: "নিরাপত্তা" },
    description: {
      en: "Authentication policies, password rules, and brute-force protection",
      bn: "প্রমাণীকরণ নীতি, পাসওয়ার্ড নিয়ম এবং ব্রুট-ফোর্স সুরক্ষা",
    },
    schema: {
      type: "object" as const,
      title: "Security Settings",
      required: ["password_min_length", "max_login_attempts"],
      properties: {
        password_min_length: {
          type: "integer",
          title: "Minimum Password Length",
          description: "Minimum character count for new passwords",
          default: 8,
          minimum: 8,
          maximum: 64,
        },
        max_login_attempts: {
          type: "integer",
          title: "Max Failed Login Attempts",
          description: "Consecutive failed logins before temporary IP lockout",
          default: 5,
          minimum: 3,
          maximum: 20,
        },
        require_2fa_for_admins: {
          type: "boolean",
          title: "Enforce 2FA for Administrators",
          description: "Mandates TOTP setup for all admin accounts",
          default: true,
        },
        session_ip_binding: {
          type: "boolean",
          title: "Bind Sessions to IP Address",
          description: "Terminates session if request IP address changes",
          default: false,
        },
      },
    },
    values: {
      password_min_length: 8,
      max_login_attempts: 5,
      require_2fa_for_admins: true,
      session_ip_binding: false,
    },
  },
  backup: {
    title: { en: "Backups", bn: "ব্যাকআপ" },
    description: {
      en: "Automated snapshot frequency, compression, and retention policies",
      bn: "স্বয়ংক্রিয় স্ন্যাপশট ফ্রিকোয়েন্সি, কম্প্রেশন এবং রিটেনশন নীতি",
    },
    schema: {
      type: "object" as const,
      title: "Backup Settings",
      required: ["backup_retention_days", "compression_algorithm"],
      properties: {
        backup_retention_days: {
          type: "integer",
          title: "Retention Period (Days)",
          description: "Number of days before automated snapshots are pruned",
          default: 30,
          minimum: 1,
          maximum: 365,
        },
        compression_algorithm: {
          type: "string",
          title: "Compression Algorithm",
          enum: ["zstd", "gzip", "none"],
          enumNames: ["Zstandard (Fast & High Ratio)", "Gzip (Standard)", "None (Uncompressed)"],
          default: "zstd",
        },
        auto_backup_enabled: {
          type: "boolean",
          title: "Automatic Daily Backups",
          description: "Run automated backups every midnight",
          default: true,
        },
        remote_storage_provider: {
          type: "string",
          title: "Storage Destination",
          enum: ["local", "s3", "b2"],
          enumNames: ["Local Disk", "Amazon S3", "Backblaze B2"],
          default: "local",
        },
      },
    },
    values: {
      backup_retention_days: 30,
      compression_algorithm: "zstd",
      auto_backup_enabled: true,
      remote_storage_provider: "local",
    },
  },
  notifications: {
    title: { en: "Notifications", bn: "বিজ্ঞপ্তি" },
    description: {
      en: "Alert channels, email dispatches, and incoming webhooks",
      bn: "সতর্কবার্তা চ্যানেল, ইমেইল প্রেরণ এবং ইনকামিং ওয়েবহুক",
    },
    schema: {
      type: "object" as const,
      title: "Notification Settings",
      required: ["alert_email"],
      properties: {
        alert_email: {
          type: "string",
          format: "email",
          title: "Alert Recipient Email",
          description: "Destination address for critical node alarms",
          default: "alerts@dhpanel.local",
        },
        notify_on_node_offline: {
          type: "boolean",
          title: "Notify when a Node goes offline",
          description: "Dispatches immediate alert upon missed node heartbeat",
          default: true,
        },
        slack_webhook_url: {
          type: "string",
          title: "Slack Webhook URL",
          description: "Optional URL for posting critical system alerts to Slack channel",
          default: "",
        },
      },
    },
    values: {
      alert_email: "alerts@dhpanel.local",
      notify_on_node_offline: true,
      slack_webhook_url: "",
    },
  },
});

let mockSettingsStore: Record<string, any> = getInitialSettingsStore();

export const resetMockSettings = () => {
  mockSettingsStore = getInitialSettingsStore();
};

export interface MockFeatureSet {
  id: string;
  name: string;
  description: string;
  features: string[];
  is_default: boolean;
  plans_count: number;
  created_at: string;
  updated_at: string;
}

export const getInitialFeatureSets = (): MockFeatureSet[] => [
  {
    id: "fs_default",
    name: "Default (Full Stack)",
    description: "Standard full-featured web, database, email and DNS hosting",
    features: [
      "domains",
      "redirects",
      "docroot",
      "temporary_links",
      "ssl_custom",
      "ssl_letsencrypt",
      "dns_zones",
      "dns_records",
      "db_mysql",
      "db_remote",
      "phpmyadmin",
      "file_manager",
      "ftp_accounts",
      "email_inbox",
      "email_forwarding",
      "webmail",
      "ip_blocker",
      "modsecurity",
    ],
    is_default: true,
    plans_count: 3,
    created_at: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
  },
  {
    id: "fs_basic",
    name: "Basic Web",
    description: "Essential web hosting with basic SSL and file manager",
    features: ["domains", "redirects", "docroot", "ssl_letsencrypt", "file_manager"],
    is_default: false,
    plans_count: 1,
    created_at: new Date(Date.now() - 86400 * 1000 * 10).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 5).toISOString(),
  },
  {
    id: "fs_dns_only",
    name: "DNS Only",
    description: "Dedicated nameserver and DNS zone record hosting",
    features: ["dns_zones", "dns_records"],
    is_default: false,
    plans_count: 0,
    created_at: new Date(Date.now() - 86400 * 1000 * 8).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 8).toISOString(),
  },
  {
    id: "fs_db_only",
    name: "Database Only",
    description: "Managed MySQL database instance with phpMyAdmin",
    features: ["db_mysql", "db_remote", "phpmyadmin"],
    is_default: false,
    plans_count: 0,
    created_at: new Date(Date.now() - 86400 * 1000 * 7).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 7).toISOString(),
  },
];

let featureSets = getInitialFeatureSets();

export const resetMockFeatureSets = () => {
  featureSets = getInitialFeatureSets();
};

export interface MockPlanResources {
  cpu: number;
  memory_mb: number;
  disk_mb: number;
  bandwidth_mb: number;
  pids_limit: number;
}

export interface MockPlanLimits {
  max_domains: number;
  max_subdomains: number;
  max_databases: number;
  max_ftp_accounts: number;
  max_email_accounts: number;
}

export interface MockPlan {
  id: string;
  name: string;
  description: string;
  feature_set_id: string;
  feature_set_name: string;
  is_default: boolean;
  users_count: number;
  resources: MockPlanResources;
  limits: MockPlanLimits;
  created_at: string;
  updated_at: string;
}

export const getInitialPlans = (): MockPlan[] => [
  {
    id: "plan_starter",
    name: "Starter Cloud",
    description: "Entry-level tier for personal blogs and landing pages",
    feature_set_id: "fs_basic",
    feature_set_name: "Basic Web",
    is_default: false,
    users_count: 8,
    resources: {
      cpu: 1,
      memory_mb: 1024,
      disk_mb: 10240,
      bandwidth_mb: 51200,
      pids_limit: 100,
    },
    limits: {
      max_domains: 1,
      max_subdomains: 5,
      max_databases: 1,
      max_ftp_accounts: 2,
      max_email_accounts: 5,
    },
    created_at: new Date(Date.now() - 86400 * 1000 * 20).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 5).toISOString(),
  },
  {
    id: "plan_standard",
    name: "Standard Business",
    description: "Balanced hosting plan for business sites and eCommerce stores",
    feature_set_id: "fs_default",
    feature_set_name: "Default (Full Stack)",
    is_default: true,
    users_count: 24,
    resources: {
      cpu: 2,
      memory_mb: 2048,
      disk_mb: 25600,
      bandwidth_mb: -1,
      pids_limit: 256,
    },
    limits: {
      max_domains: 5,
      max_subdomains: 20,
      max_databases: 5,
      max_ftp_accounts: 5,
      max_email_accounts: 20,
    },
    created_at: new Date(Date.now() - 86400 * 1000 * 15).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
  },
  {
    id: "plan_pro",
    name: "Pro Developer",
    description: "High-performance compute and database allocation for web agencies",
    feature_set_id: "fs_default",
    feature_set_name: "Default (Full Stack)",
    is_default: false,
    users_count: 5,
    resources: {
      cpu: 4,
      memory_mb: 4096,
      disk_mb: 51200,
      bandwidth_mb: -1,
      pids_limit: 512,
    },
    limits: {
      max_domains: 20,
      max_subdomains: 50,
      max_databases: 20,
      max_ftp_accounts: 20,
      max_email_accounts: -1,
    },
    created_at: new Date(Date.now() - 86400 * 1000 * 10).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 1).toISOString(),
  },
  {
    id: "plan_enterprise",
    name: "Enterprise Dedicated",
    description: "Uncapped resources and unlimited domains for enterprise clients",
    feature_set_id: "fs_default",
    feature_set_name: "Default (Full Stack)",
    is_default: false,
    users_count: 2,
    resources: {
      cpu: -1,
      memory_mb: -1,
      disk_mb: -1,
      bandwidth_mb: -1,
      pids_limit: -1,
    },
    limits: {
      max_domains: -1,
      max_subdomains: -1,
      max_databases: -1,
      max_ftp_accounts: -1,
      max_email_accounts: -1,
    },
    created_at: new Date(Date.now() - 86400 * 1000 * 5).toISOString(),
    updated_at: new Date(Date.now() - 86400 * 1000 * 5).toISOString(),
  },
];

let plans = getInitialPlans();

export const resetMockPlans = () => {
  plans = getInitialPlans();
};

export interface MockUser {
  id: string;
  username: string;
  email: string;
  status: "active" | "suspended" | "pending";
  plan_id: string;
  plan_name: string;
  node_id: string;
  node_name: string;
  primary_domain: string;
  disk_used_bytes: number;
  disk_limit_bytes: number;
  bandwidth_used_bytes: number;
  bandwidth_limit_bytes: number;
  domains_count: number;
  created_at: string;
  suspended_at: string | null;
  suspended_reason: string | null;
}

const mockPlansList = [
  { id: "plan_starter", name: "Starter Cloud", disk_limit: 10 * 1024 * 1024 * 1024 },
  { id: "plan_standard", name: "Standard Business", disk_limit: 25 * 1024 * 1024 * 1024 },
  { id: "plan_pro", name: "Pro Developer", disk_limit: 50 * 1024 * 1024 * 1024 },
  { id: "plan_enterprise", name: "Enterprise Dedicated", disk_limit: -1 },
];

const mockNodesList = [
  { id: "node-sgp-01", name: "node-sgp-01 (Singapore)" },
  { id: "node-fra-01", name: "node-fra-01 (Frankfurt)" },
  { id: "node-nyc-01", name: "node-nyc-01 (New York)" },
];

export const getInitialUsers = (): MockUser[] => {
  const usersList: MockUser[] = [];
  const baseUsernames = [
    "alpha", "beta", "gamma", "delta", "nexus", "vertex", "horizon", "aurora",
    "zenith", "stellar", "quantum", "solaris", "vortex", "cosmic", "prism",
    "cyber", "apex", "ember", "titan", "phoenix",
  ];

  for (let i = 1; i <= 1000; i++) {
    const base = baseUsernames[(i - 1) % baseUsernames.length];
    const username = i <= 20 ? base : `${base}_${i}`;
    const plan = mockPlansList[(i - 1) % mockPlansList.length];
    const node = mockNodesList[(i - 1) % mockNodesList.length];

    const status: "active" | "suspended" | "pending" =
      i % 10 === 0 ? "suspended" : i % 20 === 0 ? "pending" : "active";

    const diskLimit = plan.disk_limit;
    const diskUsed =
      diskLimit === -1
        ? 4294967296
        : Math.min(diskLimit, ((i * 12345678) % (10 * 1024 * 1024 * 1024)) + 104857600);

    usersList.push({
      id: `usr_${String(i).padStart(4, "0")}`,
      username,
      email: `${username}@example.com`,
      status,
      plan_id: plan.id,
      plan_name: plan.name,
      node_id: node.id,
      node_name: node.name,
      primary_domain: `${username}.com`,
      disk_used_bytes: diskUsed,
      disk_limit_bytes: diskLimit,
      bandwidth_used_bytes: (i * 9876543) % (50 * 1024 * 1024 * 1024),
      bandwidth_limit_bytes: -1,
      domains_count: (i % 5) + 1,
      created_at: new Date(Date.now() - i * 86400 * 1000).toISOString(),
      suspended_at:
        status === "suspended" ? new Date(Date.now() - 3600 * 1000 * 12).toISOString() : null,
      suspended_reason: status === "suspended" ? "Terms of service violation review" : null,
    });
  }
  return usersList;
};

let mockUsers = getInitialUsers();

export const resetMockUsers = () => {
  mockUsers = getInitialUsers();
  mockDetailStore = {};
};

export let mockDetailStore: Record<string, any> = {};

interface MockClientDomain {
  id: string;
  name: string;
  type: "primary" | "addon" | "subdomain" | "alias";
  docroot: string;
  is_primary: boolean;
  ssl_status: "active" | "pending" | "expired" | "self_signed" | "none";
  ssl_expires_at?: string | null;
  ssl_issuer?: string | null;
  dns_status: "active" | "pending" | "proxied" | "error";
  redirect?: {
    enabled: boolean;
    target_url: string;
    status_code: 301 | 302;
  } | null;
  php_version?: string;
  created_at: string;
}

const getInitialClientDomains = (): MockClientDomain[] => [
  {
    id: "dom_01",
    name: "alpha-hosting.com",
    type: "primary",
    docroot: "/domains/alpha-hosting.com/public_html",
    is_primary: true,
    ssl_status: "active",
    ssl_expires_at: new Date(Date.now() + 86400 * 1000 * 76).toISOString(),
    ssl_issuer: "Let's Encrypt Authority X3",
    dns_status: "active",
    redirect: null,
    php_version: "PHP 8.3",
    created_at: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
  },
  {
    id: "dom_02",
    name: "blog.alpha-hosting.com",
    type: "subdomain",
    docroot: "/domains/blog.alpha-hosting.com/public_html",
    is_primary: false,
    ssl_status: "active",
    ssl_expires_at: new Date(Date.now() + 86400 * 1000 * 60).toISOString(),
    ssl_issuer: "Let's Encrypt Authority X3",
    dns_status: "active",
    redirect: null,
    php_version: "PHP 8.3",
    created_at: new Date(Date.now() - 86400 * 1000 * 10).toISOString(),
  },
  {
    id: "dom_03",
    name: "shop.alpha-hosting.com",
    type: "subdomain",
    docroot: "/domains/shop.alpha-hosting.com/public_html",
    is_primary: false,
    ssl_status: "pending",
    dns_status: "pending",
    redirect: null,
    php_version: "PHP 8.3",
    created_at: new Date(Date.now() - 3600 * 1000 * 2).toISOString(),
  },
];

let mockClientDomains = getInitialClientDomains();

export const resetMockClientDomains = () => {
  mockClientDomains = getInitialClientDomains();
};

interface MockAdminDomain {
  id: string;
  name: string;
  type: "primary" | "addon" | "subdomain" | "alias";
  tenant_id: string;
  tenant_username: string;
  node_id: string;
  node_name: string;
  docroot: string;
  is_primary: boolean;
  status: "active" | "suspended";
  suspended_at?: string | null;
  suspended_reason?: string | null;
  ssl_status: "active" | "pending" | "expired" | "self_signed" | "none";
  ssl_expires_at?: string | null;
  ssl_issuer?: string | null;
  dns_status: "active" | "pending" | "proxied" | "error";
  redirect?: {
    enabled: boolean;
    target_url: string;
    status_code: 301 | 302;
  } | null;
  php_version?: string;
  created_at: string;
}

const getInitialAdminDomains = (): MockAdminDomain[] => {
  const tenants = [
    { id: "usr_0001", username: "alpha", node: "node-sgp-01", node_name: "node-sgp-01 (Singapore)" },
    { id: "usr_0002", username: "beta", node: "node-master-01", node_name: "node-master-01 (US-East)" },
    { id: "usr_0003", username: "gamma", node: "node-worker-02", node_name: "node-worker-02 (EU-Frankfurt)" },
    { id: "usr_0004", username: "delta", node: "node-sgp-01", node_name: "node-sgp-01 (Singapore)" },
  ];

  const items: MockAdminDomain[] = [];
  tenants.forEach((t, idx) => {
    items.push({
      id: `adm_dom_${idx + 1}`,
      name: `${t.username}-hosting.com`,
      type: "primary",
      tenant_id: t.id,
      tenant_username: t.username,
      node_id: t.node,
      node_name: t.node_name,
      docroot: `/domains/${t.username}-hosting.com/public_html`,
      is_primary: true,
      status: "active",
      ssl_status: "active",
      ssl_expires_at: new Date(Date.now() + 86400 * 1000 * 75).toISOString(),
      ssl_issuer: "Let's Encrypt Authority X3",
      dns_status: "active",
      redirect: null,
      php_version: "PHP 8.3",
      created_at: new Date(Date.now() - 86400 * 1000 * (15 - idx)).toISOString(),
    });
  });

  items.push(
    {
      id: "adm_dom_05",
      name: "shop.alpha-hosting.com",
      type: "subdomain",
      tenant_id: "usr_0001",
      tenant_username: "alpha",
      node_id: "node-sgp-01",
      node_name: "node-sgp-01 (Singapore)",
      docroot: "/domains/shop.alpha-hosting.com/public_html",
      is_primary: false,
      status: "active",
      ssl_status: "active",
      ssl_expires_at: new Date(Date.now() + 86400 * 1000 * 60).toISOString(),
      ssl_issuer: "Let's Encrypt Authority X3",
      dns_status: "active",
      redirect: null,
      php_version: "PHP 8.3",
      created_at: new Date(Date.now() - 86400 * 1000 * 10).toISOString(),
    },
    {
      id: "adm_dom_06",
      name: "blog.beta-hosting.com",
      type: "subdomain",
      tenant_id: "usr_0002",
      tenant_username: "beta",
      node_id: "node-master-01",
      node_name: "node-master-01 (US-East)",
      docroot: "/domains/blog.beta-hosting.com/public_html",
      is_primary: false,
      status: "suspended",
      ssl_status: "expired",
      dns_status: "active",
      redirect: null,
      php_version: "PHP 8.3",
      created_at: new Date(Date.now() - 86400 * 1000 * 8).toISOString(),
    },
    {
      id: "adm_dom_07",
      name: "landing.gamma-hosting.com",
      type: "addon",
      tenant_id: "usr_0003",
      tenant_username: "gamma",
      node_id: "node-worker-02",
      node_name: "node-worker-02 (EU-Frankfurt)",
      docroot: "/domains/landing.gamma-hosting.com/public_html",
      is_primary: false,
      status: "active",
      ssl_status: "pending",
      dns_status: "pending",
      redirect: null,
      php_version: "PHP 8.3",
      created_at: new Date(Date.now() - 86400 * 1000 * 3).toISOString(),
    }
  );

  return items;
};

let mockAdminDomains = getInitialAdminDomains();

export const resetMockAdminDomains = () => {
  mockAdminDomains = getInitialAdminDomains();
};

const getInitialOperations = (): any[] => [
  {
    id: "00000000-0000-0000-0000-000000000101",
    idempotency_key: "idemp_ping_001",
    op_type: "Ping",
    node_id: "00000000-0000-0000-0000-0000000000aa",
    node_name: "node-master-01",
    tenant_id: null,
    status: "succeeded",
    retries_count: 0,
    payload: { echo: "dhpanel-heartbeat", timestamp: new Date(Date.now() - 60000).toISOString() },
    output: { pong: true, latency_ms: 1.4 },
    error: null,
    created_at: new Date(Date.now() - 65000).toISOString(),
    updated_at: new Date(Date.now() - 60000).toISOString(),
  },
  {
    id: "00000000-0000-0000-0000-000000000102",
    idempotency_key: "idemp_domain_002",
    op_type: "AttachDomain",
    node_id: "00000000-0000-0000-0000-0000000000aa",
    node_name: "node-master-01",
    tenant_id: "00000000-0000-0000-0000-000000000001",
    status: "succeeded",
    retries_count: 0,
    payload: { domain: "example.com", docroot: "/var/www/example.com", php: "8.3" },
    output: { vhost_created: true, caddy_reloaded: true },
    error: null,
    created_at: new Date(Date.now() - 120000).toISOString(),
    updated_at: new Date(Date.now() - 110000).toISOString(),
  },
  {
    id: "00000000-0000-0000-0000-000000000103",
    idempotency_key: "idemp_pull_003",
    op_type: "PullImage",
    node_id: "00000000-0000-0000-0000-0000000000bb",
    node_name: "node-worker-02",
    tenant_id: null,
    status: "failed",
    retries_count: 2,
    payload: { image: "redis:7.4-alpine", digest: "sha256:7f9202a061b" },
    output: null,
    error: {
      code: "image_pull_timeout",
      message: "Network timeout while pulling container image layer 3 from registry",
      details: { registry: "docker.io", layer: "sha256:4a8bc9...", elapsed_seconds: 45 },
    },
    created_at: new Date(Date.now() - 300000).toISOString(),
    updated_at: new Date(Date.now() - 250000).toISOString(),
  },
  {
    id: "00000000-0000-0000-0000-000000000104",
    idempotency_key: "idemp_db_004",
    op_type: "CreateDatabase",
    node_id: "00000000-0000-0000-0000-0000000000bb",
    node_name: "node-worker-02",
    tenant_id: "00000000-0000-0000-0000-000000000001",
    status: "retryable",
    retries_count: 1,
    payload: { name: "demo_db", user: "db_user", charset: "utf8mb4" },
    output: null,
    error: {
      code: "connection_refused",
      message: "Database socket connection refused: daemon is initializing",
      details: { socket: "/var/run/mysqld/mysqld.sock" },
    },
    created_at: new Date(Date.now() - 400000).toISOString(),
    updated_at: new Date(Date.now() - 350000).toISOString(),
  },
  {
    id: "00000000-0000-0000-0000-000000000105",
    idempotency_key: "idemp_reload_005",
    op_type: "ReloadWebServer",
    node_id: "00000000-0000-0000-0000-0000000000aa",
    node_name: "node-master-01",
    tenant_id: null,
    status: "dispatched",
    retries_count: 0,
    payload: { graceful: true },
    output: null,
    error: null,
    created_at: new Date(Date.now() - 5000).toISOString(),
    updated_at: new Date(Date.now() - 3000).toISOString(),
  },
];

let mockOperations = getInitialOperations();

export const resetMockOperations = () => {
  mockOperations = getInitialOperations();
};

interface MockDnsRecord {
  id: string;
  zone_id: string;
  name: string;
  type: string;
  content: string;
  ttl: number;
  priority?: number | null;
  weight?: number | null;
  port?: number | null;
  flags?: number | null;
  tag?: string | null;
  created_at: string;
  updated_at: string;
}

interface MockDnsZone {
  id: string;
  domain_id: string;
  domain_name: string;
  tenant_id: string;
  nameservers: string[];
  soa_serial: number;
  records_count: number;
  is_custom_ns: boolean;
  dnssec_enabled: boolean;
  updated_at: string;
}

const getInitialDnsZones = (): MockDnsZone[] => [
  {
    id: "zone_alpha_01",
    domain_id: "dom_01",
    domain_name: "alpha-hosting.com",
    tenant_id: "usr_0001",
    nameservers: ["ns1.dhpanel.net", "ns2.dhpanel.net"],
    soa_serial: 2026111301,
    records_count: 7,
    is_custom_ns: false,
    dnssec_enabled: true,
    updated_at: new Date(Date.now() - 3600 * 1000).toISOString(),
  },
  {
    id: "zone_alpha_02",
    domain_id: "dom_02",
    domain_name: "blog.alpha-hosting.com",
    tenant_id: "usr_0001",
    nameservers: ["ns1.dhpanel.net", "ns2.dhpanel.net"],
    soa_serial: 2026111302,
    records_count: 2,
    is_custom_ns: false,
    dnssec_enabled: false,
    updated_at: new Date(Date.now() - 7200 * 1000).toISOString(),
  },
  {
    id: "zone_beta_01",
    domain_id: "dom_03",
    domain_name: "beta-app.io",
    tenant_id: "usr_0001",
    nameservers: ["ns1.dhpanel.net", "ns2.dhpanel.net"],
    soa_serial: 2026111303,
    records_count: 2,
    is_custom_ns: false,
    dnssec_enabled: false,
    updated_at: new Date(Date.now() - 86400 * 1000).toISOString(),
  },
];

const getInitialDnsRecords = (): MockDnsRecord[] => [
  {
    id: "rec_01",
    zone_id: "zone_alpha_01",
    name: "@",
    type: "A",
    content: "192.168.1.100",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_02",
    zone_id: "zone_alpha_01",
    name: "www",
    type: "CNAME",
    content: "alpha-hosting.com.",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_03",
    zone_id: "zone_alpha_01",
    name: "@",
    type: "MX",
    content: "mail.alpha-hosting.com.",
    ttl: 3600,
    priority: 10,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_04",
    zone_id: "zone_alpha_01",
    name: "@",
    type: "TXT",
    content: "v=spf1 a mx ~all",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_05",
    zone_id: "zone_alpha_01",
    name: "@",
    type: "AAAA",
    content: "2001:db8::1",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_06",
    zone_id: "zone_alpha_01",
    name: "mail",
    type: "A",
    content: "192.168.1.105",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_07",
    zone_id: "zone_alpha_01",
    name: "@",
    type: "CAA",
    content: "0 issue \"letsencrypt.org\"",
    ttl: 3600,
    flags: 0,
    tag: "issue",
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_08",
    zone_id: "zone_alpha_02",
    name: "@",
    type: "A",
    content: "192.168.1.102",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_09",
    zone_id: "zone_alpha_02",
    name: "www",
    type: "CNAME",
    content: "blog.alpha-hosting.com.",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_10",
    zone_id: "zone_beta_01",
    name: "@",
    type: "A",
    content: "192.168.1.110",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "rec_11",
    zone_id: "zone_beta_01",
    name: "www",
    type: "CNAME",
    content: "beta-app.io.",
    ttl: 3600,
    priority: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    updated_at: new Date(Date.now() - 86400000).toISOString(),
  },
];

let mockDnsZones = getInitialDnsZones();
let mockDnsRecords = getInitialDnsRecords();

export const resetMockDns = () => {
  mockDnsZones = getInitialDnsZones();
  mockDnsRecords = getInitialDnsRecords();
};

interface MockSslCert {
  id: string;
  domain_id: string;
  domain_name: string;
  status: "active" | "pending" | "expired" | "self_signed" | "none";
  type: "lets_encrypt" | "zerossl" | "custom" | "self_signed" | "none";
  issuer: string | null;
  subject: string;
  sans: string[];
  expires_at: string | null;
  days_remaining: number;
  auto_renew: boolean;
  force_https: boolean;
  hsts_enabled: boolean;
  hsts_subdomains: boolean;
  hsts_preload: boolean;
  created_at: string;
  updated_at: string;
  cert_fingerprint?: string | null;
}

const getInitialSslCertificates = (): MockSslCert[] => [
  {
    id: "cert_01",
    domain_id: "dom_01",
    domain_name: "alpha-hosting.com",
    status: "active",
    type: "lets_encrypt",
    issuer: "Let's Encrypt Authority X3",
    subject: "CN=alpha-hosting.com",
    sans: ["alpha-hosting.com", "www.alpha-hosting.com"],
    expires_at: new Date(Date.now() + 76 * 86400 * 1000).toISOString(),
    days_remaining: 76,
    auto_renew: true,
    force_https: true,
    hsts_enabled: true,
    hsts_subdomains: true,
    hsts_preload: false,
    created_at: new Date(Date.now() - 14 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 14 * 86400 * 1000).toISOString(),
    cert_fingerprint: "SHA256:4a:8b:c9:e1:2f:33:9a:b4:c7:d2:e5:6f:88:99:aa:bb",
  },
  {
    id: "cert_02",
    domain_id: "dom_02",
    domain_name: "blog.alpha-hosting.com",
    status: "active",
    type: "lets_encrypt",
    issuer: "Let's Encrypt Authority X3",
    subject: "CN=blog.alpha-hosting.com",
    sans: ["blog.alpha-hosting.com"],
    expires_at: new Date(Date.now() + 12 * 86400 * 1000).toISOString(),
    days_remaining: 12,
    auto_renew: true,
    force_https: false,
    hsts_enabled: false,
    hsts_subdomains: false,
    hsts_preload: false,
    created_at: new Date(Date.now() - 78 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 78 * 86400 * 1000).toISOString(),
    cert_fingerprint: "SHA256:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00",
  },
  {
    id: "cert_03",
    domain_id: "dom_03",
    domain_name: "beta-app.io",
    status: "expired",
    type: "zerossl",
    issuer: "ZeroSSL RSA Domain Secure Site CA",
    subject: "CN=beta-app.io",
    sans: ["beta-app.io", "*.beta-app.io"],
    expires_at: new Date(Date.now() - 2 * 86400 * 1000).toISOString(),
    days_remaining: 0,
    auto_renew: false,
    force_https: false,
    hsts_enabled: false,
    hsts_subdomains: false,
    hsts_preload: false,
    created_at: new Date(Date.now() - 92 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 2 * 86400 * 1000).toISOString(),
    cert_fingerprint: "SHA256:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99",
  },
  {
    id: "cert_04",
    domain_id: "dom_04",
    domain_name: "dev.internal-net.org",
    status: "self_signed",
    type: "self_signed",
    issuer: "DHPanel Internal Self-Signed CA",
    subject: "CN=dev.internal-net.org",
    sans: ["dev.internal-net.org"],
    expires_at: new Date(Date.now() + 350 * 86400 * 1000).toISOString(),
    days_remaining: 350,
    auto_renew: false,
    force_https: false,
    hsts_enabled: false,
    hsts_subdomains: false,
    hsts_preload: false,
    created_at: new Date(Date.now() - 15 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 15 * 86400 * 1000).toISOString(),
    cert_fingerprint: "SHA256:99:88:77:66:55:44:33:22:11:00:ff:ee:dd:cc:bb:aa",
  },
];

let mockSslCertificates = getInitialSslCertificates();

export const resetMockSsl = () => {
  mockSslCertificates = getInitialSslCertificates();
};

export interface MockVhostTemplate {
  id: string;
  name: string;
  slug: string;
  server_type: "nginx" | "caddy";
  content: string;
  variables: string[];
  version: number;
  is_default: boolean;
  domain_count: number;
  created_at: string;
  updated_at: string;
}

export interface MockVhostVersion {
  id: string;
  template_id: string;
  version: number;
  content: string;
  changelog: string;
  created_by: string;
  created_at: string;
}

const getInitialVhostTemplates = (): MockVhostTemplate[] => [
  {
    id: "tpl_01",
    name: "Nginx PHP-FPM Standard",
    slug: "nginx-php-fpm",
    server_type: "nginx",
    content: `server {
    listen 80;
    listen [::]:80;
    server_name {{domain}} www.{{domain}};
    root {{docroot}};
    index index.php index.html index.htm;

    access_log /var/log/nginx/{{domain}}.access.log;
    error_log /var/log/nginx/{{domain}}.error.log;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \\.php$ {
        fastcgi_pass unix:{{php_socket}};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        include fastcgi_params;
    }

    location ~ /\\.(?!well-known).* {
        deny all;
    }
}`,
    variables: ["domain", "docroot", "php_socket"],
    version: 3,
    is_default: true,
    domain_count: 3,
    created_at: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 2 * 86400 * 1000).toISOString(),
  },
  {
    id: "tpl_02",
    name: "Nginx Static Site",
    slug: "nginx-static",
    server_type: "nginx",
    content: `server {
    listen 80;
    listen [::]:80;
    server_name {{domain}} www.{{domain}};
    root {{docroot}};
    index index.html index.htm;

    access_log /var/log/nginx/{{domain}}.access.log;
    error_log /var/log/nginx/{{domain}}.error.log;

    location / {
        try_files $uri $uri/ =404;
    }

    location ~* \\.(jpg|jpeg|png|gif|ico|css|js)$ {
        expires 30d;
        add_header Cache-Control "public, no-transform";
    }
}`,
    variables: ["domain", "docroot"],
    version: 1,
    is_default: false,
    domain_count: 1,
    created_at: new Date(Date.now() - 40 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 40 * 86400 * 1000).toISOString(),
  },
  {
    id: "tpl_03",
    name: "Caddy Reverse Proxy",
    slug: "caddy-reverse-proxy",
    server_type: "caddy",
    content: `{{domain}}, www.{{domain}} {
    encode gzip zstd
    log {
        output file /var/log/caddy/{{domain}}.log
    }

    reverse_proxy 127.0.0.1:{{port}} {
        header_up Host {host}
        header_up X-Real-IP {remote}
    }
}`,
    variables: ["domain", "port"],
    version: 2,
    is_default: false,
    domain_count: 0,
    created_at: new Date(Date.now() - 20 * 86400 * 1000).toISOString(),
    updated_at: new Date(Date.now() - 5 * 86400 * 1000).toISOString(),
  },
];

const getInitialVhostVersions = (): MockVhostVersion[] => [
  {
    id: "ver_03",
    template_id: "tpl_01",
    version: 3,
    content: `server {
    listen 80;
    listen [::]:80;
    server_name {{domain}} www.{{domain}};
    root {{docroot}};
    index index.php index.html index.htm;

    access_log /var/log/nginx/{{domain}}.access.log;
    error_log /var/log/nginx/{{domain}}.error.log;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \\.php$ {
        fastcgi_pass unix:{{php_socket}};
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        include fastcgi_params;
    }

    location ~ /\\.(?!well-known).* {
        deny all;
    }
}`,
    changelog: "Add hidden files protection except for well-known ACME challenges",
    created_by: "admin",
    created_at: new Date(Date.now() - 2 * 86400 * 1000).toISOString(),
  },
  {
    id: "ver_02",
    template_id: "tpl_01",
    version: 2,
    content: `server {
    listen 80;
    server_name {{domain}} www.{{domain}};
    root {{docroot}};
    index index.php index.html;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \\.php$ {
        fastcgi_pass unix:{{php_socket}};
        include fastcgi_params;
    }
}`,
    changelog: "Support front-controller routing with try_files query parameter pass-through",
    created_by: "admin",
    created_at: new Date(Date.now() - 15 * 86400 * 1000).toISOString(),
  },
  {
    id: "ver_01",
    template_id: "tpl_01",
    version: 1,
    content: `server {
    listen 80;
    server_name {{domain}};
    root {{docroot}};
    index index.php index.html;

    location ~ \\.php$ {
        fastcgi_pass unix:{{php_socket}};
    }
}`,
    changelog: "Initial basic PHP vhost configuration",
    created_by: "admin",
    created_at: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
  },
];

let mockVhostTemplates = getInitialVhostTemplates();
let mockVhostVersions = getInitialVhostVersions();

export const resetMockVhost = () => {
  mockVhostTemplates = getInitialVhostTemplates();
  mockVhostVersions = getInitialVhostVersions();
};

const defaultPlanIni = {
  memory_limit: "256M",
  upload_max_filesize: "64M",
  post_max_size: "64M",
  max_execution_time: 60,
  max_input_time: 60,
  display_errors: false,
  opcache_enable: true,
  date_timezone: "UTC",
};

const getInitialPhpConfig = () => ({
  default_version: "8.3",
  supported_versions: [
    { version: "8.4", status: "active" as const, is_default: false, socket_path: "/run/php/php8.4-fpm.sock" },
    { version: "8.3", status: "active" as const, is_default: true, socket_path: "/run/php/php8.3-fpm.sock" },
    { version: "8.2", status: "active" as const, is_default: false, socket_path: "/run/php/php8.2-fpm.sock" },
    { version: "8.1", status: "deprecated" as const, is_default: false, socket_path: "/run/php/php8.1-fpm.sock" },
  ],
  domain_configs: [
    { domain_id: "dom_01", domain_name: "alpha-hosting.com", php_version: "8.3", is_custom: false },
    { domain_id: "dom_02", domain_name: "blog.alpha-hosting.com", php_version: "8.2", is_custom: true },
  ],
  ini_settings: { ...defaultPlanIni },
  plan_defaults: { ...defaultPlanIni },
  plan_limits: {
    max_memory_limit: "512M",
    max_execution_time: 300,
    max_upload_filesize: "128M",
  },
  installed_extensions: [
    "bcmath", "curl", "fileinfo", "gd", "intl", "json", "mbstring", "mysqli",
    "opcache", "pdo", "pdo_mysql", "pdo_sqlite", "redis", "sodium", "xml", "zip"
  ],
});

let mockClientPhpConfig = getInitialPhpConfig();

export const resetMockPhp = () => {
  mockClientPhpConfig = getInitialPhpConfig();
};

interface MockService {
  id: string;
  name: "nginx" | "php-fpm";
  display_name: string;
  description: string;
  status: "running" | "stopped" | "restarting" | "degraded";
  uptime_seconds: number;
  memory_usage_mb: number;
  cpu_percent: number;
  workers_count: number;
  last_restarted_at: string;
  restarts_limit_per_minute: number;
}

const getInitialServices = (): MockService[] => [
  {
    id: "srv_nginx",
    name: "nginx",
    display_name: "Nginx Web Server",
    description: "Reverse proxy, static asset delivery, and TLS termination engine",
    status: "running",
    uptime_seconds: 84600,
    memory_usage_mb: 48,
    cpu_percent: 0.8,
    workers_count: 4,
    last_restarted_at: new Date(Date.now() - 86400 * 1000).toISOString(),
    restarts_limit_per_minute: 3,
  },
  {
    id: "srv_php_fpm",
    name: "php-fpm",
    display_name: "PHP-FPM Process Manager",
    description: "FastCGI process pool serving dynamic tenant PHP applications",
    status: "running",
    uptime_seconds: 43200,
    memory_usage_mb: 136,
    cpu_percent: 1.4,
    workers_count: 6,
    last_restarted_at: new Date(Date.now() - 43200 * 1000).toISOString(),
    restarts_limit_per_minute: 3,
  },
];

let mockServices = getInitialServices();
let mockServiceRestartsLog: Record<string, number[]> = {
  nginx: [],
  "php-fpm": [],
};

export const resetMockServices = () => {
  mockServices = getInitialServices();
  mockServiceRestartsLog = {
    nginx: [],
    "php-fpm": [],
  };
};

interface MockPageTemplate {
  id: string;
  type: "suspended_domain" | "suspended_user" | "error_404" | "error_502";
  title: string;
  description: string;
  statusCode: number;
  content: string;
  defaultContent: string;
  updatedAt: string;
  isCustomized: boolean;
  variables: { name: string; description: string; sample: string }[];
}

const defaultSuspendedDomainHtml = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Domain Suspended - {{domain}}</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: sans-serif; background-color: #0f172a; color: #f8fafc; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 1.5rem; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 1rem; max-width: 540px; width: 100%; padding: 2.5rem; text-align: center; }
    .badge { display: inline-block; padding: 0.35rem 0.85rem; border-radius: 9999px; font-size: 0.85rem; font-weight: 600; background: #7f1d1d; color: #fecaca; margin-bottom: 1.25rem; }
    h1 { font-size: 1.75rem; font-weight: 700; color: #f1f5f9; margin-bottom: 0.75rem; }
    p { color: #94a3b8; font-size: 0.95rem; line-height: 1.6; margin-bottom: 1.5rem; }
    .domain-box { background: #0f172a; border: 1px solid #334155; border-radius: 0.5rem; padding: 0.85rem; font-family: monospace; color: #38bdf8; font-size: 1rem; margin-bottom: 1.5rem; }
    .reason-box { background: rgba(239, 68, 68, 0.1); border-left: 3px solid #ef4444; padding: 0.75rem 1rem; text-align: left; border-radius: 0.25rem; color: #fca5a5; font-size: 0.875rem; margin-bottom: 1.75rem; }
    .btn { display: inline-block; background: #2563eb; color: #ffffff; text-decoration: none; font-weight: 600; padding: 0.75rem 1.5rem; border-radius: 0.5rem; }
    .footer { margin-top: 2rem; font-size: 0.75rem; color: #64748b; }
  </style>
</head>
<body>
  <div class="card">
    <div class="badge">Notice</div>
    <h1>Domain Suspended</h1>
    <p>This domain name has been suspended and its services are temporarily disabled.</p>
    <div class="domain-box">{{domain}}</div>
    <div class="reason-box"><strong>Reason:</strong> {{reason}}</div>
    <a href="mailto:{{contact_email}}" class="btn">Contact Administrator</a>
    <div class="footer">&copy; {{year}} {{panel_name}}. All rights reserved.</div>
  </div>
</body>
</html>`;

const defaultSuspendedUserHtml = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Account Suspended</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: sans-serif; background-color: #0f172a; color: #f8fafc; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 1.5rem; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 1rem; max-width: 540px; width: 100%; padding: 2.5rem; text-align: center; }
    h1 { font-size: 1.75rem; font-weight: 700; color: #f1f5f9; margin-bottom: 0.75rem; }
    p { color: #94a3b8; font-size: 0.95rem; line-height: 1.6; margin-bottom: 1.5rem; }
    .btn { display: inline-block; background: #2563eb; color: #ffffff; text-decoration: none; font-weight: 600; padding: 0.75rem 1.5rem; border-radius: 0.5rem; }
    .footer { margin-top: 2rem; font-size: 0.75rem; color: #64748b; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Account Suspended</h1>
    <p>The hosting account associated with this site is currently suspended.</p>
    <a href="mailto:{{contact_email}}" class="btn">Contact Support</a>
    <div class="footer">&copy; {{year}} {{panel_name}}. All rights reserved.</div>
  </div>
</body>
</html>`;

const defaultError404Html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>404 - Page Not Found</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: sans-serif; background-color: #0b0f19; color: #f3f4f6; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 1.5rem; }
    .container { text-align: center; max-width: 500px; width: 100%; }
    .code { font-size: 6rem; font-weight: 900; line-height: 1; color: #38bdf8; margin-bottom: 0.5rem; }
    h1 { font-size: 1.75rem; font-weight: 700; margin-bottom: 0.75rem; color: #f9fafb; }
    p { color: #9ca3af; font-size: 1rem; line-height: 1.6; margin-bottom: 1.75rem; }
    .path-box { display: inline-block; background: #1f2937; border-radius: 0.375rem; padding: 0.35rem 0.75rem; font-family: monospace; color: #e5e7eb; font-size: 0.875rem; margin-bottom: 1.5rem; }
    .footer { margin-top: 2.5rem; font-size: 0.75rem; color: #4b5563; }
  </style>
</head>
<body>
  <div class="container">
    <div class="code">404</div>
    <h1>Page Not Found</h1>
    <p>The page or resource you requested could not be located on this server.</p>
    <div class="path-box">{{path}}</div>
    <div class="footer">&copy; {{year}} {{panel_name}} Web Services</div>
  </div>
</body>
</html>`;

const defaultError502Html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>502 - Bad Gateway</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: sans-serif; background-color: #0b0f19; color: #f3f4f6; min-height: 100vh; display: flex; align-items: center; justify-content: center; padding: 1.5rem; }
    .card { background: #111827; border: 1px solid #1f2937; border-radius: 0.75rem; max-width: 580px; width: 100%; padding: 2.5rem; text-align: center; }
    .code { font-size: 4rem; font-weight: 800; color: #f87171; line-height: 1; margin-bottom: 0.5rem; }
    h1 { font-size: 1.5rem; font-weight: 700; color: #f9fafb; margin-bottom: 0.75rem; }
    p { color: #9ca3af; font-size: 0.95rem; line-height: 1.6; margin-bottom: 1.5rem; }
    .footer { font-size: 0.75rem; color: #6b7280; }
  </style>
</head>
<body>
  <div class="card">
    <div class="code">502</div>
    <h1>Bad Gateway</h1>
    <p>The web server received an invalid response while acting as a gateway or proxy to the upstream PHP application ({{service}}).</p>
    <div class="footer">&copy; {{year}} {{panel_name}} Web Services</div>
  </div>
</body>
</html>`;

const getInitialPageTemplates = (): MockPageTemplate[] => [
  {
    id: "tpl_suspended_domain",
    type: "suspended_domain",
    title: "Domain Suspended",
    description: "Served when a specific domain is suspended by the administrator.",
    statusCode: 451,
    content: defaultSuspendedDomainHtml,
    defaultContent: defaultSuspendedDomainHtml,
    updatedAt: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
    isCustomized: false,
    variables: [
      { name: "{{domain}}", description: "Suspended domain name", sample: "example.com" },
      { name: "{{reason}}", description: "Reason for domain suspension", sample: "Terms of service review" },
      { name: "{{contact_email}}", description: "Support or admin contact email", sample: "admin@dhpanel.net" },
      { name: "{{panel_name}}", description: "Platform / branding name", sample: "DHPanel" },
      { name: "{{year}}", description: "Current calendar year", sample: "2026" },
    ],
  },
  {
    id: "tpl_suspended_user",
    type: "suspended_user",
    title: "Account Suspended",
    description: "Served for all sites belonging to a suspended tenant user account.",
    statusCode: 403,
    content: defaultSuspendedUserHtml,
    defaultContent: defaultSuspendedUserHtml,
    updatedAt: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
    isCustomized: false,
    variables: [
      { name: "{{username}}", description: "Suspended user account name", sample: "tenant_user" },
      { name: "{{domain}}", description: "Domain name being accessed", sample: "client-site.com" },
      { name: "{{contact_email}}", description: "Administrative support email", sample: "support@dhpanel.net" },
      { name: "{{panel_name}}", description: "Platform / branding name", sample: "DHPanel" },
      { name: "{{year}}", description: "Current calendar year", sample: "2026" },
    ],
  },
  {
    id: "tpl_error_404",
    type: "error_404",
    title: "404 Not Found",
    description: "Custom 404 page displayed when requested route or file does not exist.",
    statusCode: 404,
    content: defaultError404Html,
    defaultContent: defaultError404Html,
    updatedAt: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
    isCustomized: false,
    variables: [
      { name: "{{domain}}", description: "Host domain name", sample: "my-app.net" },
      { name: "{{path}}", description: "Missing request path", sample: "/old-missing-page" },
      { name: "{{panel_name}}", description: "Platform / branding name", sample: "DHPanel" },
      { name: "{{year}}", description: "Current calendar year", sample: "2026" },
    ],
  },
  {
    id: "tpl_error_502",
    type: "error_502",
    title: "502 Bad Gateway",
    description: "Custom 502 error page displayed when upstream backend or PHP-FPM daemon fails.",
    statusCode: 502,
    content: defaultError502Html,
    defaultContent: defaultError502Html,
    updatedAt: new Date(Date.now() - 30 * 86400 * 1000).toISOString(),
    isCustomized: false,
    variables: [
      { name: "{{domain}}", description: "Host domain name", sample: "api-server.org" },
      { name: "{{service}}", description: "Failing upstream daemon name", sample: "php-fpm-8.3" },
      { name: "{{panel_name}}", description: "Platform / branding name", sample: "DHPanel" },
      { name: "{{year}}", description: "Current calendar year", sample: "2026" },
    ],
  },
];

let mockPageTemplates = getInitialPageTemplates();

export const resetMockPageTemplates = () => {
  mockPageTemplates = getInitialPageTemplates();
};

interface MockPreviewLink {
  domain_id: string;
  target_domain: string;
  enabled: boolean;
  token: string;
  url: string;
  created_at: string;
  expires_at: string;
  ttl_hours: number;
}

const getInitialPreviewLinks = (): Record<string, MockPreviewLink> => ({
  dom_02: {
    domain_id: "dom_02",
    target_domain: "blog.alpha-hosting.com",
    enabled: true,
    token: "exp_test_7890",
    url: "https://exp_test_7890.preview.dhpanel.net",
    created_at: new Date(Date.now() - 48 * 3600 * 1000).toISOString(),
    expires_at: new Date(Date.now() - 24 * 3600 * 1000).toISOString(), // Expired
    ttl_hours: 24,
  },
});

let mockPreviewLinks = getInitialPreviewLinks();

export const resetMockPreviewLinks = () => {
  mockPreviewLinks = getInitialPreviewLinks();
};

interface MockDb {
  id: string;
  name: string;
  raw_name: string;
  prefix: string;
  charset: string;
  collation: string;
  size_bytes: number;
  size_formatted: string;
  tables_count: number;
  assigned_users: string[];
  created_at: string;
}

interface MockDbUser {
  id: string;
  username: string;
  raw_username: string;
  prefix: string;
  host: string;
  grants: {
    db_name: string;
    privileges: string[];
  }[];
  created_at: string;
}

const getInitialDatabases = (): MockDb[] => [
  {
    id: "db_01",
    name: "demo_wordpress",
    raw_name: "wordpress",
    prefix: "demo_",
    charset: "utf8mb4",
    collation: "utf8mb4_unicode_ci",
    size_bytes: 24657920,
    size_formatted: "23.5 MB",
    tables_count: 14,
    assigned_users: ["demo_wpuser"],
    created_at: "2026-11-10T12:00:00Z",
  },
  {
    id: "db_02",
    name: "demo_analytics",
    raw_name: "analytics",
    prefix: "demo_",
    charset: "utf8mb4",
    collation: "utf8mb4_unicode_ci",
    size_bytes: 19922944,
    size_formatted: "19.0 MB",
    tables_count: 6,
    assigned_users: ["demo_analyst"],
    created_at: "2026-11-11T15:30:00Z",
  },
];

const getInitialDbUsers = (): MockDbUser[] => [
  {
    id: "dbusr_01",
    username: "demo_wpuser",
    raw_username: "wpuser",
    prefix: "demo_",
    host: "10.0.0.%",
    grants: [
      {
        db_name: "demo_wordpress",
        privileges: ["ALL PRIVILEGES"],
      },
    ],
    created_at: "2026-11-10T12:05:00Z",
  },
  {
    id: "dbusr_02",
    username: "demo_analyst",
    raw_username: "analyst",
    prefix: "demo_",
    host: "10.0.0.%",
    grants: [
      {
        db_name: "demo_analytics",
        privileges: ["SELECT", "INSERT", "UPDATE"],
      },
    ],
    created_at: "2026-11-11T15:35:00Z",
  },
];

let mockDatabases = getInitialDatabases();
let mockDbUsers = getInitialDbUsers();
let mockRemoteDbHosts = ["198.51.100.12/32"];

export const resetMockDatabases = () => {
  mockDatabases = getInitialDatabases();
  mockDbUsers = getInitialDbUsers();
  mockRemoteDbHosts = ["198.51.100.12/32"];
};


const mockMe = {
  id: "00000000-0000-0000-0000-000000000001",
  username: "admin",
  email: "admin@dhpanel.local",
  kind: "admin",
  locale: "bn",
  mfa_enabled: false,
  permissions: ["*"],
  roles: ["super_admin"],
  modules: [
    "dashboard",
    "security",
    "users",
    "nodes",
    "operations",
    "feature-sets",
    "plans",
    "audit",
    "domains",
    "dns",
    "ssl",
    "settings",
    "demo",
  ],
  features: ["all"],
  csrf_token: "mock-csrf-token-admin-12345",
};

export const handlers = [
  // Health
  http.get(/\/api\/v1\/public\/health$/, () => {
    return HttpResponse.json({
      status: "ok",
      version: "0.1.0",
      timestamp: new Date().toISOString(),
    });
  }),

  // Auth: Current user (GET /api/v1/auth/me)
  http.get(/\/api\/v1\/auth\/me$/, () => {
    return HttpResponse.json({
      ...mockMe,
      mfa_enabled: mfaEnabled,
    });
  }),

  // Backward-compatibility: bootstrap endpoints
  http.get(/\/api\/v1\/admin\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: mockMe.id,
        username: mockMe.username,
        email: mockMe.email,
        name: "DHPanel Super Admin",
      },
      roles: ["super_admin"],
      permissions: ["*"],
      modules: ["dashboard", "users", "domains", "settings", "security"],
      features: ["all"],
      limits: {},
      branding: {
        companyName: "DHPanel",
        logoUrl: "/logo.svg",
      },
      locale: "bn",
      csrfToken: mockMe.csrf_token,
    });
  }),

  http.get(/\/api\/v1\/client\/bootstrap$/, () => {
    return HttpResponse.json({
      user: {
        id: "00000000-0000-0000-0000-000000000002",
        username: "demo_user",
        email: "demo@example.com",
        name: "Demo Tenant User",
      },
      roles: ["client"],
      permissions: ["dashboard.read", "domains.read", "domains.create", "files.read"],
      modules: ["dashboard", "domains", "files", "security"],
      features: ["domains", "redirects"],
      limits: { max_domains: 5, used_domains: 1 },
      branding: { companyName: "DHPanel" },
      locale: "bn",
      csrfToken: "mock-csrf-token-client-12345",
    });
  }),

  // Auth: Login step 1 (POST /api/v1/public/auth/login)
  http.post(/\/api\/v1\/public\/auth\/login$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const login = body?.login || body?.username;
    const password = body?.password;

    if (!login || !password) {
      return HttpResponse.json(
        { title: "Invalid username or password", status: 401 },
        { status: 401 }
      );
    }

    if (login === "invalid" || password === "wrong") {
      return HttpResponse.json(
        { title: "Invalid username or password", status: 401 },
        { status: 401 }
      );
    }

    if (login === "admin_2fa") {
      return HttpResponse.json(
        {
          mfa_required: true,
          methods: ["totp"],
          expires_in: 300,
        },
        { status: 202 }
      );
    }

    return HttpResponse.json({
      ...mockMe,
      username: login,
      email: `${login}@dhpanel.local`,
    });
  }),

  // Auth: Login step 2 (POST /api/v1/public/auth/login/mfa)
  http.post(/\/api\/v1\/public\/auth\/login\/mfa$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { code } = body || {};

    if (code !== "123456") {
      return HttpResponse.json(
        { title: "Invalid MFA code", status: 401 },
        { status: 401 }
      );
    }

    return HttpResponse.json({
      ...mockMe,
      username: "admin_2fa",
      email: "admin_2fa@dhpanel.local",
      mfa_enabled: true,
    });
  }),

  // Auth: Logout (POST /api/v1/auth/logout)
  http.post(/\/api\/v1\/auth\/logout$/, () => {
    return new HttpResponse(null, { status: 204 });
  }),

  // Auth: Sudo mode (POST /api/v1/auth/sudo)
  http.post(/\/api\/v1\/auth\/sudo$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    if (body?.password === "wrong-password") {
      return HttpResponse.json(
        { title: "Incorrect password", status: 401 },
        { status: 401 }
      );
    }
    return new HttpResponse(null, { status: 204 });
  }),

  // MFA: Status (GET /api/v1/auth/mfa)
  http.get(/\/api\/v1\/auth\/mfa$/, () => {
    return HttpResponse.json({
      mfa_enabled: mfaEnabled,
      backup_codes_remaining: mfaEnabled ? backupCodes.length : 0,
    });
  }),

  // MFA: Enroll TOTP (POST /api/v1/auth/mfa/totp/enroll)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/enroll$/, () => {
    return HttpResponse.json({
      secret: "JBSWY3DPEHPK3PXP",
      otpauth_uri: "otpauth://totp/DHPanel:admin@dhpanel.local?secret=JBSWY3DPEHPK3PXP&issuer=DHPanel",
    });
  }),

  // MFA: Confirm TOTP (POST /api/v1/auth/mfa/totp/confirm)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/confirm$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const code = body?.code || body?.totp_code;

    if (code !== "123456") {
      return HttpResponse.json(
        { title: "Invalid 6-digit TOTP code", status: 400 },
        { status: 400 }
      );
    }

    mfaEnabled = true;
    return HttpResponse.json({
      backup_codes: backupCodes,
    });
  }),

  // MFA: Disable TOTP (POST /api/v1/auth/mfa/totp/disable)
  http.post(/\/api\/v1\/auth\/mfa\/totp\/disable$/, () => {
    mfaEnabled = false;
    return new HttpResponse(null, { status: 204 });
  }),

  // Sessions: List live sessions (GET /api/v1/auth/sessions)
  http.get(/\/api\/v1\/auth\/sessions$/, () => {
    if (sessions.length <= 1) {
      sessions = getInitialSessions();
    }
    return HttpResponse.json({ sessions });
  }),

  // Sessions: Revoke a session (DELETE /api/v1/auth/sessions/:id)
  http.delete(/\/api\/v1\/auth\/sessions\/(.+)$/, ({ params }) => {
    const id = params[0];
    sessions = sessions.filter((s) => s.id !== id);
    return new HttpResponse(null, { status: 204 });
  }),

  // Sessions: Revoke all other sessions (POST /api/v1/auth/sessions/revoke-others)
  http.post(/\/api\/v1\/auth\/sessions\/revoke-others$/, () => {
    const initialCount = sessions.length;
    sessions = sessions.filter((s) => s.current);
    return HttpResponse.json({
      revoked_count: Math.max(0, initialCount - sessions.length),
    });
  }),

  // Account Password change (POST /api/v1/account/password)
  http.post(/\/api\/v1\/account\/password$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { current_password, new_password } = body || {};

    if (!current_password || !new_password) {
      return HttpResponse.json(
        { error: "bad_request", message: "Current and new password are required" },
        { status: 400 }
      );
    }

    if (current_password === "wrong-password") {
      return HttpResponse.json(
        { error: "invalid_current_password", message: "Current password does not match" },
        { status: 400 }
      );
    }

    if (new_password.length < 8) {
      return HttpResponse.json(
        { error: "weak_password", message: "Password must be at least 8 characters" },
        { status: 400 }
      );
    }

    return HttpResponse.json({ success: true, message: "Password updated successfully" });
  }),

  // Login History (GET /api/v1/account/login-history)
  http.get(/\/api\/v1\/account\/login-history$/, () => {
    return HttpResponse.json({ events: loginEvents });
  }),

  // Audit Logs (GET /api/v1/admin/audit-logs) (T01.12 / T01.16)
  ...(() => {
    const mockAuditLogs = Array.from({ length: 30 }, (_, i) => {
      const actions = [
        "auth.login",
        "auth.login_failed",
        "auth.mfa_enrolled",
        "auth.session_revoked",
        "auth.password_changed",
        "auth.sudo_granted",
        "vhost.create",
        "domain.create",
        "user.role_assigned",
      ];
      const outcomes = ["success", "failure", "denied"] as const;
      const targetTypes = ["session", "account", "domain", "role"];
      const action = actions[i % actions.length];
      const outcome = action.includes("failed") ? "failure" : outcomes[i % outcomes.length];
      const targetType = targetTypes[i % targetTypes.length];
      const at = new Date(Date.now() - (i + 1) * 3600 * 1000 * 2).toISOString();

      return {
        id: `00000000-0000-0000-0000-${String(i + 1).padStart(12, "0")}`,
        at,
        action,
        outcome,
        actor_id: i % 3 === 0 ? "00000000-0000-0000-0000-000000000001" : `00000000-0000-0000-0000-${String(i + 10).padStart(12, "0")}`,
        actor_ip: i % 2 === 0 ? "192.168.1.105" : "203.0.113.42",
        correlation_id: `req-${1000 + i}-trace`,
        node_id: "00000000-0000-0000-0000-0000000000aa",
        target_type: targetType,
        target_id: `obj-${100 + i}`,
        tenant_id: i % 4 === 0 ? "00000000-0000-0000-0000-000000000001" : null,
        metadata: {
          client: i % 2 === 0 ? "Chrome 130 (macOS)" : "Firefox 131 (Linux)",
          details: `Executed ${action} with status ${outcome}`,
          ip: i % 2 === 0 ? "192.168.1.105" : "203.0.113.42",
          duration_ms: (i + 1) * 12,
        },
      };
    });

    return [
      http.get(/\/api\/v1\/admin\/audit-logs/, ({ request }) => {
        const url = new URL(request.url);
        const action = url.searchParams.get("action");
        const actorId = url.searchParams.get("actor_id");
        const outcome = url.searchParams.get("outcome");
        const targetType = url.searchParams.get("target_type");
        const targetId = url.searchParams.get("target_id");
        const limit = parseInt(url.searchParams.get("limit") || "10", 10);
        const cursor = url.searchParams.get("cursor");

        let filtered = [...mockAuditLogs];

        if (action) {
          filtered = filtered.filter((item) =>
            item.action.toLowerCase().startsWith(action.toLowerCase())
          );
        }
        if (actorId) {
          filtered = filtered.filter((item) => item.actor_id === actorId);
        }
        if (outcome && outcome !== "all") {
          filtered = filtered.filter((item) => item.outcome === outcome);
        }
        if (targetType) {
          filtered = filtered.filter((item) => item.target_type === targetType);
        }
        if (targetId) {
          filtered = filtered.filter((item) => item.target_id === targetId);
        }

        let startIndex = 0;
        if (cursor) {
          const found = filtered.findIndex((item) => item.id === cursor);
          if (found !== -1) {
            startIndex = found + 1;
          }
        }

        const pageItems = filtered.slice(startIndex, startIndex + limit);
        const nextItem = filtered[startIndex + limit];
        const nextCursor = nextItem ? nextItem.id : null;

        return HttpResponse.json({
          items: pageItems,
          next_cursor: nextCursor,
        });
      }),
    ];
  })(),

  // Node Management (GET /api/v1/admin/nodes) TODO(api:T02.08)
  ...(() => {
    const mockNodes = [
      {
        id: "00000000-0000-0000-0000-0000000000aa",
        name: "node-master-01",
        hostname: "cp1.dhpanel.net",
        ip_address: "167.104.101.81",
        status: "online",
        version: "0.1.0",
        fingerprint: "SHA256:7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b",
        last_seen_at: new Date().toISOString(),
        enrolled_at: new Date(Date.now() - 7 * 86400 * 1000).toISOString(),
        capabilities: {
          cpu_cores: 8,
          cpu_model: "AMD EPYC 7763 64-Core Processor",
          cpu_usage_pct: 18,
          ram_total_bytes: 16 * 1024 * 1024 * 1024,
          ram_used_bytes: 5.4 * 1024 * 1024 * 1024,
          disk_total_bytes: 250 * 1024 * 1024 * 1024,
          disk_used_bytes: 52 * 1024 * 1024 * 1024,
          kernel: "Linux 6.8.0-45-generic",
          docker_version: "27.3.1",
          security_features: ["seccomp", "apparmor", "userns"],
        },
      },
      {
        id: "00000000-0000-0000-0000-0000000000bb",
        name: "node-worker-02",
        hostname: "worker2.dhpanel.net",
        ip_address: "167.104.101.82",
        status: "draining",
        version: "0.1.0",
        fingerprint: "SHA256:9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e",
        last_seen_at: new Date(Date.now() - 30 * 1000).toISOString(),
        enrolled_at: new Date(Date.now() - 3 * 86400 * 1000).toISOString(),
        capabilities: {
          cpu_cores: 16,
          cpu_model: "AMD EPYC 7763 64-Core Processor",
          cpu_usage_pct: 42,
          ram_total_bytes: 32 * 1024 * 1024 * 1024,
          ram_used_bytes: 18.2 * 1024 * 1024 * 1024,
          disk_total_bytes: 500 * 1024 * 1024 * 1024,
          disk_used_bytes: 140 * 1024 * 1024 * 1024,
          kernel: "Linux 6.8.0-45-generic",
          docker_version: "27.3.1",
          security_features: ["seccomp", "apparmor"],
        },
      },
      {
        id: "00000000-0000-0000-0000-0000000000cc",
        name: "node-worker-03-backup",
        hostname: "worker3.dhpanel.net",
        ip_address: "167.104.101.83",
        status: "offline",
        version: "0.1.0",
        fingerprint: "SHA256:1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
        last_seen_at: new Date(Date.now() - 3600 * 1000 * 5).toISOString(),
        enrolled_at: new Date(Date.now() - 14 * 86400 * 1000).toISOString(),
        capabilities: {
          cpu_cores: 4,
          cpu_model: "Intel Xeon E-2288G CPU @ 3.70GHz",
          cpu_usage_pct: 0,
          ram_total_bytes: 8 * 1024 * 1024 * 1024,
          ram_used_bytes: 1.1 * 1024 * 1024 * 1024,
          disk_total_bytes: 120 * 1024 * 1024 * 1024,
          disk_used_bytes: 22 * 1024 * 1024 * 1024,
          kernel: "Linux 6.5.0-28-generic",
          docker_version: "26.1.4",
          security_features: ["seccomp"],
        },
      },
    ];

    const mockEnrollments: any[] = [
      {
        id: "00000000-0000-0000-0000-000000000001",
        name: "node-worker-pending",
        created_at: new Date(Date.now() - 300 * 1000).toISOString(),
        created_by: "00000000-0000-0000-0000-000000000001",
        expires_at: new Date(Date.now() + 600 * 1000).toISOString(),
        status: "pending",
      },
    ];

    return [
      // List Nodes (GET /api/v1/admin/nodes) TODO(api:T02.08)
      http.get(/\/api\/v1\/admin\/nodes$/, () => {
        return HttpResponse.json({
          items: mockNodes,
          total: mockNodes.length,
        });
      }),

      // Create Enrollment Token (POST /api/v1/admin/nodes)
      http.post(/\/api\/v1\/admin\/nodes$/, async ({ request }) => {
        const body = (await request.json().catch(() => ({}))) as any;
        const name = body?.name || "node-worker-" + Math.floor(Math.random() * 100);
        const id = "00000000-0000-0000-0000-" + Math.random().toString(16).substring(2, 14).padEnd(12, "0");
        const token = "dhn_" + Math.random().toString(36).substring(2, 15) + Math.random().toString(36).substring(2, 15);
        const expiresIn = 900;
        const expiresAt = new Date(Date.now() + expiresIn * 1000).toISOString();
        const enrollCommand = `curl -sSL https://get.dhpanel.com/agent.sh | sudo bash -s -- --token ${token}`;

        const newEnrollment = {
          id,
          name,
          token,
          expires_at: expiresAt,
          expires_in: expiresIn,
          enroll_command: enrollCommand,
        };

        mockEnrollments.unshift({
          id,
          name,
          created_at: new Date().toISOString(),
          created_by: "00000000-0000-0000-0000-000000000001",
          expires_at: expiresAt,
          status: "pending",
        });

        return HttpResponse.json(newEnrollment, { status: 201 });
      }),

      // List Node Enrollments (GET /api/v1/admin/nodes/enrollments)
      http.get(/\/api\/v1\/admin\/nodes\/enrollments$/, () => {
        return HttpResponse.json({
          items: mockEnrollments,
        });
      }),

      // Cancel/Revoke Node Enrollment (DELETE /api/v1/admin/nodes/enrollments/:id)
      http.delete(/\/api\/v1\/admin\/nodes\/enrollments\/([^/]+)$/, ({ params }) => {
        const { 0: id } = params as any;
        const idx = mockEnrollments.findIndex((e) => e.id === id);
        if (idx !== -1) {
          mockEnrollments[idx].status = "revoked";
        }
        return new HttpResponse(null, { status: 204 });
      }),

      // Revoke Node (POST /api/v1/admin/nodes/:id/revoke)
      http.post(/\/api\/v1\/admin\/nodes\/([^/]+)\/revoke$/, ({ params }) => {
        const { 0: nodeId } = params as any;
        const target = mockNodes.find((n) => n.id === nodeId || n.name === nodeId);
        if (target) {
          target.status = "revoked";
        }
        return HttpResponse.json({
          success: true,
          message: "Node revoked successfully",
        });
      }),
    ];
  })(),
  // Operations Ledger (T02.10, T02.11, T02.17) TODO(api:T02.17)
  // List operations (GET /api/v1/admin/operations)
  http.get(/\/api\/v1\/admin\/operations$/, ({ request }) => {
    const url = new URL(request.url);
    const status = url.searchParams.get("status");
    const opType = url.searchParams.get("op_type");
    const nodeId = url.searchParams.get("node_id");

    let items = [...mockOperations];
    if (status && status !== "all") {
      items = items.filter((op) => op.status === status);
    }
    if (opType) {
      items = items.filter((op) => op.op_type.toLowerCase().includes(opType.toLowerCase()));
    }
    if (nodeId) {
      items = items.filter((op) => op.node_id === nodeId || op.node_name === nodeId);
    }

    return HttpResponse.json({
      items,
      total: items.length,
    });
  }),

  // Get single operation (GET /api/v1/admin/operations/:id)
  http.get(/\/api\/v1\/admin\/operations\/([^/]+)$/, ({ params }) => {
    const id = (params.id || (params as any)[0] || (params as any)["0"]) as string;
    const op = mockOperations.find((o) => o.id === id);
    if (op) {
      return HttpResponse.json(op);
    }
    // Fallback for mock tenant creation or other generic mock operations
    return HttpResponse.json({
      id,
      status: "succeeded",
      op_type: "GenericOperation",
      action: "tenant.create",
      progress_pct: 100,
      container_status: "running",
      completed_at: new Date().toISOString(),
    });
  }),

  // Retry operation (POST /api/v1/admin/operations/:id/retry)
  http.post(/\/api\/v1\/admin\/operations\/([^/]+)\/retry$/, ({ params }) => {
    const { 0: id } = params as any;
    const opIndex = mockOperations.findIndex((o) => o.id === id);
    if (opIndex === -1) {
      return HttpResponse.json({ error: "not_found", message: "Operation not found" }, { status: 404 });
    }

    const updated = {
      ...mockOperations[opIndex],
      status: "pending",
      retries_count: mockOperations[opIndex].retries_count + 1,
      error: null,
      updated_at: new Date().toISOString(),
    };
    mockOperations[opIndex] = updated;

    return HttpResponse.json({
      success: true,
      operation: updated,
    });
  }),

  // Modules: List all modules (GET /api/v1/admin/modules)
  // TODO(api:T03.03): Replace with real backend endpoint
  http.get(/\/api\/v1\/admin\/modules$/, () => {
    return HttpResponse.json({
      items: modules,
      total: modules.length,
    });
  }),

  // Modules: Toggle module state (POST /api/v1/admin/modules/:name/toggle)
  // TODO(api:T03.03): Replace with real backend endpoint
  // TODO(api:T03.12): Cascading dependency toggle warnings
  http.post(/\/api\/v1\/admin\/modules\/([^/]+)\/toggle$/, async ({ params, request }) => {
    const name = params[0] as string;
    const body = (await request.json().catch(() => ({}))) as any;
    const { cascade = false } = body || {};

    const modIndex = modules.findIndex((m) => m.name === name);
    if (modIndex === -1) {
      return HttpResponse.json({ error: "not_found", message: "Module not found" }, { status: 404 });
    }

    const mod = modules[modIndex];
    if (mod.core) {
      return HttpResponse.json(
        { error: "core_module_locked", message: "Core modules cannot be disabled" },
        { status: 400 }
      );
    }

    const targetEnabled = !mod.enabled;
    const disabledDependents: string[] = [];

    if (!targetEnabled) {
      // Disabling module: find all enabled dependents
      const activeDependents = modules.filter(
        (m) => m.enabled && m.depends_on.includes(name)
      );

      if (activeDependents.length > 0) {
        if (!cascade) {
          return HttpResponse.json(
            {
              error: "dependencies_active",
              message: "Cannot disable module because dependent modules are active",
              dependents: activeDependents.map((d) => d.name),
            },
            { status: 409 }
          );
        }

        // Cascade disable dependents
        for (const dep of activeDependents) {
          dep.enabled = false;
          dep.status = "disabled";
          disabledDependents.push(dep.name);
        }
      }
    }

    mod.enabled = targetEnabled;
    mod.status = targetEnabled ? "active" : "disabled";

    return HttpResponse.json({
      success: true,
      module: mod,
      disabled_dependents: disabledDependents,
    });
  }),

  // Settings: Get all modules and their schemas/values (GET /api/v1/admin/settings)
  // TODO(api:T03.09): Replace with real backend settings endpoint
  http.get(/\/api\/v1\/admin\/settings$/, () => {
    return HttpResponse.json({
      modules: Object.keys(mockSettingsStore).map((key) => ({
        id: key,
        title: mockSettingsStore[key].title,
        description: mockSettingsStore[key].description,
        schema: mockSettingsStore[key].schema,
        values: mockSettingsStore[key].values,
      })),
    });
  }),

  // Settings: Get schema & values for a specific module (GET /api/v1/admin/settings/:module)
  // TODO(api:T03.09): Replace with real backend settings endpoint
  http.get(/\/api\/v1\/admin\/settings\/([^/]+)$/, ({ params }) => {
    const mod = params[0] as string;
    const entry = mockSettingsStore[mod];
    if (!entry) {
      return HttpResponse.json(
        { error: "not_found", message: `Settings module '${mod}' not found` },
        { status: 404 }
      );
    }
    return HttpResponse.json({
      module: mod,
      title: entry.title,
      description: entry.description,
      schema: entry.schema,
      values: entry.values,
    });
  }),

  // Settings: Update values for a module (PUT /api/v1/admin/settings/:module)
  // TODO(api:T03.09): Replace with real backend settings endpoint
  // TODO(api:T03.08): JSON Schema validation
  http.put(/\/api\/v1\/admin\/settings\/([^/]+)$/, async ({ params, request }) => {
    const mod = params[0] as string;
    const entry = mockSettingsStore[mod];
    if (!entry) {
      return HttpResponse.json(
        { error: "not_found", message: `Settings module '${mod}' not found` },
        { status: 404 }
      );
    }

    const body = (await request.json().catch(() => ({}))) as any;
    const incomingValues = body?.values || body;

    // Validate required fields
    const required = entry.schema.required || [];
    for (const reqKey of required) {
      if (
        incomingValues[reqKey] === undefined ||
        incomingValues[reqKey] === null ||
        incomingValues[reqKey] === ""
      ) {
        return HttpResponse.json(
          {
            error: "validation_failed",
            message: `Missing required field: ${reqKey}`,
            field: reqKey,
          },
          { status: 422 }
        );
      }
    }

    // Update store
    mockSettingsStore[mod].values = {
      ...mockSettingsStore[mod].values,
      ...incomingValues,
    };

    return HttpResponse.json({
      success: true,
      module: mod,
      values: mockSettingsStore[mod].values,
    });
  }),

  // Feature Sets: List all feature sets (GET /api/v1/admin/feature-sets)
  // TODO(api:T04.02): Replace with real backend feature sets endpoint
  http.get(/\/api\/v1\/admin\/feature-sets$/, () => {
    return HttpResponse.json({
      items: featureSets,
      total: featureSets.length,
    });
  }),

  // Feature Sets: Create feature set (POST /api/v1/admin/feature-sets)
  // TODO(api:T04.02): Replace with real backend feature sets endpoint
  http.post(/\/api\/v1\/admin\/feature-sets$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { name, description = "", features = [], is_default = false } = body || {};

    if (!name || typeof name !== "string" || !name.trim()) {
      return HttpResponse.json(
        { error: "bad_request", message: "Feature set name is required" },
        { status: 400 }
      );
    }

    if (is_default) {
      featureSets.forEach((fs) => {
        fs.is_default = false;
      });
    }

    const newFs: MockFeatureSet = {
      id: `fs_${Date.now()}`,
      name: name.trim(),
      description: description.trim(),
      features: Array.isArray(features) ? features : [],
      is_default: Boolean(is_default),
      plans_count: 0,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    featureSets.unshift(newFs);
    return HttpResponse.json(newFs, { status: 201 });
  }),

  // Feature Sets: Update feature set (PUT /api/v1/admin/feature-sets/:id)
  // TODO(api:T04.02): Replace with real backend feature sets endpoint
  http.put(/\/api\/v1\/admin\/feature-sets\/([^/]+)$/, async ({ params, request }) => {
    const id = params[0] as string;
    const body = (await request.json().catch(() => ({}))) as any;
    const { name, description, features, is_default } = body || {};

    const index = featureSets.findIndex((fs) => fs.id === id);
    if (index === -1) {
      return HttpResponse.json(
        { error: "not_found", message: "Feature set not found" },
        { status: 404 }
      );
    }

    if (name !== undefined && (!name || !name.trim())) {
      return HttpResponse.json(
        { error: "bad_request", message: "Feature set name cannot be empty" },
        { status: 400 }
      );
    }

    if (is_default) {
      featureSets.forEach((fs) => {
        fs.is_default = false;
      });
    }

    const current = featureSets[index];
    const updated: MockFeatureSet = {
      ...current,
      name: name !== undefined ? name.trim() : current.name,
      description: description !== undefined ? description.trim() : current.description,
      features: Array.isArray(features) ? features : current.features,
      is_default: is_default !== undefined ? Boolean(is_default) : current.is_default,
      updated_at: new Date().toISOString(),
    };

    featureSets[index] = updated;
    return HttpResponse.json(updated);
  }),

  // Feature Sets: Delete feature set (DELETE /api/v1/admin/feature-sets/:id)
  // TODO(api:T04.02): Replace with real backend feature sets endpoint
  http.delete(/\/api\/v1\/admin\/feature-sets\/([^/]+)$/, ({ params }) => {
    const id = params[0] as string;
    const index = featureSets.findIndex((fs) => fs.id === id);
    if (index === -1) {
      return HttpResponse.json(
        { error: "not_found", message: "Feature set not found" },
        { status: 404 }
      );
    }

    if (featureSets[index].is_default) {
      return HttpResponse.json(
        { error: "cannot_delete_default", message: "Cannot delete the default feature set" },
        { status: 400 }
      );
    }

    featureSets.splice(index, 1);
    return HttpResponse.json({ success: true });
  }),

  // Plans: List all hosting plans (GET /api/v1/admin/plans)
  // TODO(api:T04.03): Replace with real backend plan endpoint
  http.get(/\/api\/v1\/admin\/plans$/, () => {
    return HttpResponse.json({
      items: plans,
      total: plans.length,
    });
  }),

  // Plans: Create hosting plan (POST /api/v1/admin/plans)
  // TODO(api:T04.03): Replace with real backend plan endpoint
  http.post(/\/api\/v1\/admin\/plans$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const {
      name,
      description = "",
      feature_set_id = "fs_default",
      feature_set_name = "Default (Full Stack)",
      is_default = false,
      resources = {},
      limits = {},
    } = body || {};

    if (!name || typeof name !== "string" || !name.trim()) {
      return HttpResponse.json(
        { error: "bad_request", message: "Plan name is required" },
        { status: 400 }
      );
    }

    if (is_default) {
      plans.forEach((p) => {
        p.is_default = false;
      });
    }

    const newPlan: MockPlan = {
      id: `plan_${Date.now()}`,
      name: name.trim(),
      description: description.trim(),
      feature_set_id,
      feature_set_name,
      is_default: Boolean(is_default),
      users_count: 0,
      resources: {
        cpu: resources.cpu ?? 1,
        memory_mb: resources.memory_mb ?? 1024,
        disk_mb: resources.disk_mb ?? 10240,
        bandwidth_mb: resources.bandwidth_mb ?? -1,
        pids_limit: resources.pids_limit ?? 100,
      },
      limits: {
        max_domains: limits.max_domains ?? 1,
        max_subdomains: limits.max_subdomains ?? 5,
        max_databases: limits.max_databases ?? 1,
        max_ftp_accounts: limits.max_ftp_accounts ?? 2,
        max_email_accounts: limits.max_email_accounts ?? 5,
      },
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    plans.unshift(newPlan);
    return HttpResponse.json(newPlan, { status: 201 });
  }),

  // Plans: Update hosting plan (PUT /api/v1/admin/plans/:id)
  // TODO(api:T04.03): Replace with real backend plan endpoint
  http.put(/\/api\/v1\/admin\/plans\/([^/]+)$/, async ({ params, request }) => {
    const id = (params.id || params[0]) as string;
    const body = (await request.json().catch(() => ({}))) as any;
    const { name, description, feature_set_id, feature_set_name, is_default, resources, limits } = body || {};

    const index = plans.findIndex((p) => p.id === id);
    if (index === -1) {
      return HttpResponse.json(
        { error: "not_found", message: "Hosting plan not found" },
        { status: 404 }
      );
    }

    if (name !== undefined && (!name || !name.trim())) {
      return HttpResponse.json(
        { error: "bad_request", message: "Plan name cannot be empty" },
        { status: 400 }
      );
    }

    if (is_default) {
      plans.forEach((p) => {
        p.is_default = false;
      });
    }

    const current = plans[index];
    const updated: MockPlan = {
      ...current,
      name: name !== undefined ? name.trim() : current.name,
      description: description !== undefined ? description.trim() : current.description,
      feature_set_id: feature_set_id !== undefined ? feature_set_id : current.feature_set_id,
      feature_set_name: feature_set_name !== undefined ? feature_set_name : current.feature_set_name,
      is_default: is_default !== undefined ? Boolean(is_default) : current.is_default,
      resources: resources !== undefined ? { ...current.resources, ...resources } : current.resources,
      limits: limits !== undefined ? { ...current.limits, ...limits } : current.limits,
      updated_at: new Date().toISOString(),
    };

    plans[index] = updated;
    return HttpResponse.json(updated);
  }),

  // Plans: Delete hosting plan (DELETE /api/v1/admin/plans/:id)
  // TODO(api:T04.03): Replace with real backend plan endpoint
  http.delete(/\/api\/v1\/admin\/plans\/([^/]+)$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const index = plans.findIndex((p) => p.id === id);
    if (index === -1) {
      return HttpResponse.json(
        { error: "not_found", message: "Hosting plan not found" },
        { status: 404 }
      );
    }

    if (plans[index].is_default) {
      return HttpResponse.json(
        { error: "cannot_delete_default", message: "Cannot delete the default hosting plan" },
        { status: 400 }
      );
    }

    if (plans[index].users_count > 0) {
      return HttpResponse.json(
        { error: "plan_in_use", message: "Cannot delete plan with active assigned users" },
        { status: 400 }
      );
    }

    plans.splice(index, 1);
    return HttpResponse.json({ success: true });
  }),

  // Users: List users with pagination and search/filters (GET /api/v1/admin/users)
  // TODO(api:T04.12): Replace with real backend users endpoint
  http.get(/\/api\/v1\/admin\/users(\?.*)?$/, ({ request }) => {
    const url = new URL(request.url);
    const search = (url.searchParams.get("search") || "").trim().toLowerCase();
    const status = url.searchParams.get("status");
    const planId = url.searchParams.get("plan_id");
    const nodeId = url.searchParams.get("node_id");
    const page = Math.max(1, parseInt(url.searchParams.get("page") || "1", 10));
    const perPage = Math.max(1, parseInt(url.searchParams.get("per_page") || "10", 10));

    let filtered = [...mockUsers];

    if (search) {
      filtered = filtered.filter(
        (u) =>
          u.username.toLowerCase().includes(search) ||
          u.email.toLowerCase().includes(search) ||
          (u.primary_domain && u.primary_domain.toLowerCase().includes(search))
      );
    }

    if (status && status !== "all") {
      filtered = filtered.filter((u) => u.status === status);
    }

    if (planId && planId !== "all") {
      filtered = filtered.filter((u) => u.plan_id === planId);
    }

    if (nodeId && nodeId !== "all") {
      filtered = filtered.filter((u) => u.node_id === nodeId);
    }

    const total = filtered.length;
    const totalPages = Math.max(1, Math.ceil(total / perPage));
    const startIndex = (page - 1) * perPage;
    const items = filtered.slice(startIndex, startIndex + perPage);

    return HttpResponse.json({
      items,
      total,
      page,
      per_page: perPage,
      total_pages: totalPages,
    });
  }),

  // Users: Bulk actions (POST /api/v1/admin/users/bulk)
  // TODO(api:T04.12): Replace with real backend bulk endpoint
  http.post(/\/api\/v1\/admin\/users\/bulk$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { action, user_ids = [], plan_id, reason } = body || {};

    const targetSet = new Set(user_ids);

    if (action === "suspend") {
      mockUsers.forEach((u) => {
        if (targetSet.has(u.id)) {
          u.status = "suspended";
          u.suspended_at = new Date().toISOString();
          u.suspended_reason = reason || "Suspended by admin";
        }
      });
    } else if (action === "unsuspend") {
      mockUsers.forEach((u) => {
        if (targetSet.has(u.id)) {
          u.status = "active";
          u.suspended_at = null;
          u.suspended_reason = null;
        }
      });
    } else if (action === "change_plan" && plan_id) {
      const targetPlan = mockPlansList.find((p) => p.id === plan_id);
      if (targetPlan) {
        mockUsers.forEach((u) => {
          if (targetSet.has(u.id)) {
            u.plan_id = targetPlan.id;
            u.plan_name = targetPlan.name;
            u.disk_limit_bytes = targetPlan.disk_limit;
          }
        });
      }
    } else if (action === "delete") {
      mockUsers = mockUsers.filter((u) => !targetSet.has(u.id));
    }

    return HttpResponse.json({ success: true, affected: targetSet.size });
  }),

  // Users: Single suspend (POST /api/v1/admin/users/:id/suspend)
  // TODO(api:T04.12): Replace with real backend endpoint
  http.post(/\/api\/v1\/admin\/users\/([^/]+)\/suspend$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const user = mockUsers.find((u) => u.id === id);
    if (!user && !mockDetailStore[id]) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    const suspendedAt = new Date().toISOString();
    if (user) {
      user.status = "suspended";
      user.suspended_at = suspendedAt;
    }
    if (mockDetailStore[id]) {
      mockDetailStore[id].status = "suspended";
      mockDetailStore[id].suspended_at = suspendedAt;
    }
    return HttpResponse.json(user || mockDetailStore[id]);
  }),

  // Users: Single unsuspend (POST /api/v1/admin/users/:id/unsuspend)
  // TODO(api:T04.12): Replace with real backend endpoint
  http.post(/\/api\/v1\/admin\/users\/([^/]+)\/unsuspend$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const user = mockUsers.find((u) => u.id === id);
    if (!user && !mockDetailStore[id]) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    if (user) {
      user.status = "active";
      user.suspended_at = null;
    }
    if (mockDetailStore[id]) {
      mockDetailStore[id].status = "active";
      mockDetailStore[id].suspended_at = null;
    }
    return HttpResponse.json(user || mockDetailStore[id]);
  }),

  // Users: Impersonate user (POST /api/v1/admin/users/:id/impersonate)
  // TODO(api:T04.13): Replace with real impersonation endpoint
  http.post(/\/api\/v1\/admin\/users\/([^/]+)\/impersonate$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const user = mockUsers.find((u) => u.id === id);
    if (!user) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    return HttpResponse.json({
      token: `mock_impersonate_${user.id}_${Date.now()}`,
      expires_in_seconds: 60,
      redirect_url: `/client?impersonate=${user.id}`,
    });
  }),

  // Users: Delete single user (DELETE /api/v1/admin/users/:id)
  // TODO(api:T04.12): Replace with real backend delete endpoint
  http.delete(/\/api\/v1\/admin\/users\/([^/]+)$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const index = mockUsers.findIndex((u) => u.id === id);
    if (index === -1) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    mockUsers.splice(index, 1);
    return new HttpResponse(null, { status: 204 });
  }),

  // Users: Create tenant user with operation & container provisioning (POST /api/v1/admin/users)
  // TODO(api:T04.12): Replace with real backend tenant creation endpoint
  http.post(/\/api\/v1\/admin\/users$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { username, email, plan_id = "plan_standard", node_id = "node-sgp-01", primary_domain } = body || {};

    if (!username || !email) {
      return HttpResponse.json(
        { title: "Validation Error", detail: "Username and email are required" },
        { status: 400 }
      );
    }

    const opId = `op_prov_${Date.now()}`;
    const containerName = `dh-tenant-${username}`;

    return HttpResponse.json(
      {
        user: {
          id: `usr_${Date.now()}`,
          username,
          email,
          status: "active",
          plan_id,
          node_id,
          primary_domain,
        },
        operation_id: opId,
        container: {
          id: `cnt_${username}_${Date.now()}`,
          name: containerName,
          status: "running",
          image: "dhpanel/stack-php83-nginx:latest",
          ip_address: "172.28.0.12",
          created_at: new Date().toISOString(),
        },
      },
      { status: 201 }
    );
  }),


  // Users: Get detailed tenant user (GET /api/v1/admin/users/:id)
  // TODO(api:T04.12): Replace with real backend tenant detail endpoint
  ...(() => {
    return [
      http.get(/\/api\/v1\/admin\/users\/([^/]+)$/, ({ params }) => {
        const id = (params.id || params[0]) as string;
        if (!mockDetailStore[id]) {
          const username = id.startsWith("usr_") ? "alpha" : id;
          mockDetailStore[id] = {
            id,
            username,
            email: `${username}@example.com`,
            status: "active",
            uid: 1001,
            root_path: `/srv/tenants/${username}`,
            plan_id: "plan_standard",
            plan_name: "Standard Business",
            node_id: "node-sgp-01",
            node_name: "node-sgp-01 (Singapore)",
            primary_domain: `${username}.com`,
            created_at: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
            suspended_at: null,
            suspended_reason: null,
            resources: {
              cpu_cores: 2,
              cpu_used_pct: 28,
              memory_limit_mb: 2048,
              memory_used_mb: 850,
              disk_limit_mb: 25600,
              disk_used_mb: 1240,
              inodes_limit: 250000,
              inodes_used: 12450,
              bandwidth_limit_mb: -1,
              bandwidth_used_mb: 4800,
              pids_limit: 256,
              pids_used: 42,
            },
            container: {
              id: `cnt_${username}_01`,
              name: `dh-tenant-${username}`,
              status: "running",
              image: "dhpanel/stack-php83-nginx:latest",
              ip_address: "172.28.0.5",
              started_at: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
              uptime: "Up 14 days",
            },
            domains: [
              {
                id: "dom_01",
                domain: `${username}.com`,
                is_primary: true,
                docroot: `/srv/tenants/${username}/domains/${username}.com/public_html`,
                ssl_status: "active",
                php_version: "PHP 8.3",
              },
              {
                id: "dom_02",
                domain: `blog.${username}.com`,
                is_primary: false,
                docroot: `/srv/tenants/${username}/domains/blog.${username}.com/public_html`,
                ssl_status: "active",
                php_version: "PHP 8.3",
              },
            ],
            databases: [
              {
                id: "db_01",
                name: `${username}_wp`,
                type: "mysql",
                size_mb: 48,
                charset: "utf8mb4",
              },
              {
                id: "db_02",
                name: `${username}_app`,
                type: "postgres",
                size_mb: 18,
                charset: "UTF8",
              },
            ],
            logs: [
              {
                id: "log_01",
                timestamp: new Date(Date.now() - 60 * 1000 * 2).toISOString(),
                level: "info",
                source: "nginx",
                message: '192.168.1.105 - - "GET / HTTP/2.0" 200 4820 "-" "Mozilla/5.0 Chrome/130.0"',
              },
              {
                id: "log_02",
                timestamp: new Date(Date.now() - 60 * 1000 * 5).toISOString(),
                level: "info",
                source: "php-fpm",
                message: "[pool www] child 42 ready to handle requests",
              },
              {
                id: "log_03",
                timestamp: new Date(Date.now() - 60 * 1000 * 15).toISOString(),
                level: "warn",
                source: "nginx",
                message: 'Client sent invalid header: "X-Forwarded-Host: evil.com", dropped',
              },
            ],
            activities: [
              {
                id: "act_01",
                action: "user.login",
                actor: username,
                timestamp: new Date(Date.now() - 3600 * 1000 * 2).toISOString(),
                status: "success",
                details: "Client portal session initiated from 192.168.1.105",
              },
              {
                id: "act_02",
                action: "tenant.container_start",
                actor: "system",
                timestamp: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
                status: "success",
                details: "Rootless container stack provisioned and healthy",
              },
            ],
          };
        }
        return HttpResponse.json(mockDetailStore[id]);
      }),

      // Suspend
      http.post(/\/api\/v1\/admin\/users\/([^/]+)\/suspend$/, ({ params }) => {
        const id = (params.id || params[0]) as string;
        if (mockDetailStore[id]) {
          mockDetailStore[id].status = "suspended";
          mockDetailStore[id].suspended_at = new Date().toISOString();
        }
        return HttpResponse.json({ success: true, status: "suspended" });
      }),

      // Unsuspend
      http.post(/\/api\/v1\/admin\/users\/([^/]+)\/unsuspend$/, ({ params }) => {
        const id = (params.id || params[0]) as string;
        if (mockDetailStore[id]) {
          mockDetailStore[id].status = "active";
          mockDetailStore[id].suspended_at = null;
        }
        return HttpResponse.json({ success: true, status: "active" });
      }),

      // Password Reset
      http.post(/\/api\/v1\/admin\/users\/([^/]+)\/password$/, () => {
        return HttpResponse.json({ success: true, message: "Password updated successfully" });
      }),

      // Impersonate
      http.post(/\/api\/v1\/admin\/users\/([^/]+)\/impersonate$/, ({ params }) => {
        const id = (params.id || params[0]) as string;
        return HttpResponse.json({
          token: `mock_imp_token_${Date.now()}`,
          expires_in_seconds: 60,
          redirect_url: `/client?user=${id}`,
        });
      }),

      // Delete
      http.delete(/\/api\/v1\/admin\/users\/([^/]+)$/, ({ params }) => {
        const id = (params.id || params[0]) as string;
        delete mockDetailStore[id];
        return new HttpResponse(null, { status: 204 });
      }),
    ];
  })(),

  // Client: Dashboard overview & resource quotas (GET /api/v1/client/dashboard)
  // TODO(api:T04.18): Replace with real backend client dashboard endpoint
  http.get(/\/api\/v1\/client\/dashboard$/, () => {
    return HttpResponse.json({
      tenant: {
        id: "usr_tenant_01",
        username: "client_alpha",
        email: "alpha@example.com",
        primary_domain: "alpha-hosting.com",
        plan_name: "Standard Business Plan",
        node_name: "node-sgp-01 (Singapore)",
        server_ip: "167.104.101.81",
        nameservers: ["ns1.dhpanel.net", "ns2.dhpanel.net"],
        created_at: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
        status: "active",
      },
      container: {
        status: "running",
        name: "dh-tenant-alpha",
        image: "dhpanel/stack-php83-nginx:latest",
        php_version: "8.3.12",
        uptime: "Up 14 days, 6 hours",
        ip_address: "172.28.0.5",
      },
      resources: {
        disk_limit_mb: 25600,
        disk_used_mb: 1280,
        bandwidth_limit_mb: -1,
        bandwidth_used_mb: 4800,
        memory_limit_mb: 2048,
        memory_used_mb: 850,
        cpu_cores: 2,
        cpu_used_pct: 28,
        inodes_limit: 250000,
        inodes_used: 12450,
        databases_limit: 10,
        databases_used: 2,
        domains_limit: 5,
        domains_used: 2,
        emails_limit: 10,
        emails_used: 1,
      },
      primary_domain: {
        domain: "alpha-hosting.com",
        docroot: "/srv/tenants/client_alpha/domains/alpha-hosting.com/public_html",
        ssl_status: "active",
        ssl_issuer: "Let's Encrypt Authority X3",
        ssl_expires_in_days: 76,
        php_version: "PHP 8.3",
      },
      shortcuts: [
        { id: "files", title: "File Manager", icon: "Folder", href: "/files", phase: 5 },
        { id: "domains", title: "Domain Names", icon: "Globe", href: "/domains", phase: 6 },
        { id: "databases", title: "Databases", icon: "Database", href: "/databases", phase: 7 },
        { id: "ssl", title: "SSL / TLS", icon: "Lock", href: "/ssl", phase: 6 },
        { id: "cron", title: "Cron Jobs", icon: "Clock", href: "/cron", phase: 5 },
        { id: "php", title: "PHP Configuration", icon: "Code", href: "/php", phase: 6 },
        { id: "backups", title: "Backup & Restore", icon: "Archive", href: "/backups", phase: 8 },
        { id: "security", title: "Account Security", icon: "ShieldCheck", href: "/security", phase: 1 },
      ],
      recent_events: [
        {
          id: "ev_01",
          action: "user.login",
          timestamp: new Date(Date.now() - 600 * 1000).toISOString(),
          ip_address: "192.168.1.105",
          description: "Client portal session active",
        },
        {
          id: "ev_02",
          action: "ssl.renewed",
          timestamp: new Date(Date.now() - 86400 * 1000 * 2).toISOString(),
          ip_address: "system",
          description: "SSL Certificate auto-renewed for alpha-hosting.com",
        },
        {
          id: "ev_03",
          action: "container.start",
          timestamp: new Date(Date.now() - 86400 * 1000 * 14).toISOString(),
          ip_address: "node-sgp-01",
          description: "Container stack started and healthy",
        },
      ],
    });
  }),

  // Client: List domains & limits (GET /api/v1/client/domains)
  // TODO(api:T05.03): Replace with real backend client domains endpoint
  http.get(/\/api\/v1\/client\/domains$/, () => {
    return HttpResponse.json({
      items: mockClientDomains,
      total: mockClientDomains.length,
      limits: {
        max_domains: 5,
        used_domains: mockClientDomains.length,
      },
    });
  }),

  // Client: Create domain (POST /api/v1/client/domains)
  // TODO(api:T05.03): Replace with real backend client domain creation endpoint
  http.post(/\/api\/v1\/client\/domains$/, async ({ request }) => {
    const body = (await request.json()) as any;
    if (!body || !body.name) {
      return HttpResponse.json({ message: "Domain name is required" }, { status: 400 });
    }

    if (mockClientDomains.length >= 5) {
      return HttpResponse.json(
        { message: "Domain limit reached for your plan" },
        { status: 400 }
      );
    }

    const domainName = String(body.name).trim().toLowerCase();
    const existing = mockClientDomains.find(
      (d) => d.name.toLowerCase() === domainName
    );
    if (existing) {
      return HttpResponse.json(
        { message: `Domain ${domainName} is already configured` },
        { status: 409 }
      );
    }

    const newDomain: MockClientDomain = {
      id: `dom_${Date.now()}`,
      name: domainName,
      type: body.type || "addon",
      docroot: body.docroot || `/domains/${domainName}/public_html`,
      is_primary: false,
      ssl_status: body.enable_ssl ? "pending" : "none",
      ssl_expires_at: null,
      ssl_issuer: null,
      dns_status: "pending",
      redirect: body.redirect?.enabled
        ? {
            enabled: true,
            target_url: body.redirect.target_url,
            status_code: body.redirect.status_code || 301,
          }
        : null,
      php_version: "PHP 8.3",
      created_at: new Date().toISOString(),
    };

    mockClientDomains = [newDomain, ...mockClientDomains];
    return HttpResponse.json(newDomain, { status: 201 });
  }),

  // Client: Update docroot (PUT /api/v1/client/domains/:id/docroot)
  // TODO(api:T05.04): Replace with real backend client update docroot endpoint
  http.put(/\/api\/v1\/client\/domains\/([^/]+)\/docroot$/, async ({ params, request }) => {
    const id = params[0] as string;
    const body = (await request.json()) as any;
    const domain = mockClientDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    if (!body.docroot || !String(body.docroot).startsWith("/")) {
      return HttpResponse.json({ message: "Valid document root path is required" }, { status: 400 });
    }

    domain.docroot = String(body.docroot).trim();
    return HttpResponse.json(domain);
  }),

  // Client: Update redirect (PUT /api/v1/client/domains/:id/redirect)
  // TODO(api:T05.04): Replace with real backend client update redirect endpoint
  http.put(/\/api\/v1\/client\/domains\/([^/]+)\/redirect$/, async ({ params, request }) => {
    const id = params[0] as string;
    const body = (await request.json()) as any;
    const domain = mockClientDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    if (body.enabled) {
      domain.redirect = {
        enabled: true,
        target_url: body.target_url,
        status_code: body.status_code || 301,
      };
    } else {
      domain.redirect = null;
    }

    return HttpResponse.json(domain);
  }),

  // Client: Delete domain (DELETE /api/v1/client/domains/:id)
  // TODO(api:T05.03): Replace with real backend client delete domain endpoint
  http.delete(/\/api\/v1\/client\/domains\/([^/]+)$/, ({ params }) => {
    const id = params[0] as string;
    const domain = mockClientDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    if (domain.is_primary) {
      return HttpResponse.json(
        { message: "Primary domain cannot be deleted" },
        { status: 400 }
      );
    }

    mockClientDomains = mockClientDomains.filter((d) => d.id !== id);
    return HttpResponse.json({ status: "deleted", id });
  }),

  // Admin: List all domains with filters (GET /api/v1/admin/domains)
  // TODO(api:T05.03): Replace with real backend admin domains endpoint
  http.get(/\/api\/v1\/admin\/domains$/, ({ request }) => {
    const url = new URL(request.url);
    const q = (url.searchParams.get("q") || "").toLowerCase().trim();
    const status = url.searchParams.get("status") || "all";
    const type = url.searchParams.get("type") || "all";
    const node = url.searchParams.get("node") || "all";
    const page = Math.max(1, parseInt(url.searchParams.get("page") || "1", 10));
    const perPage = Math.max(1, parseInt(url.searchParams.get("per_page") || "10", 10));

    const filtered = mockAdminDomains.filter((d) => {
      const matchSearch =
        !q ||
        d.name.toLowerCase().includes(q) ||
        d.tenant_username.toLowerCase().includes(q);
      const matchStatus = status === "all" || d.status === status;
      const matchType = type === "all" || d.type === type;
      const matchNode = node === "all" || d.node_id === node || d.node_name.includes(node);
      return matchSearch && matchStatus && matchType && matchNode;
    });

    const total = filtered.length;
    const totalPages = Math.max(1, Math.ceil(total / perPage));
    const start = (page - 1) * perPage;
    const paginated = filtered.slice(start, start + perPage);

    return HttpResponse.json({
      items: paginated,
      total,
      page,
      per_page: perPage,
      total_pages: totalPages,
      stats: {
        total_domains: mockAdminDomains.length,
        active_domains: mockAdminDomains.filter((d) => d.status === "active").length,
        suspended_domains: mockAdminDomains.filter((d) => d.status === "suspended").length,
        ssl_active_count: mockAdminDomains.filter((d) => d.ssl_status === "active").length,
      },
    });
  }),

  // Admin: Bulk add domains into operation queue (POST /api/v1/admin/domains/bulk-create)
  // TODO(api:T05.03): Replace with real backend bulk create endpoint
  http.post(/\/api\/v1\/admin\/domains\/bulk-create$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const { tenant_id = "usr_0001", node_id = "node-sgp-01", domains = [], enable_ssl = true } = body || {};

    const opId = `op_bulk_dom_${Date.now()}`;
    const tenantUsername = tenant_id === "usr_0001" ? "alpha" : "tenant";
    const nodeName = `${node_id} (Cluster)`;

    // Add domains to mockAdminDomains
    const newItems: MockAdminDomain[] = domains.map((domainName: string, idx: number) => ({
      id: `adm_bulk_${Date.now()}_${idx}`,
      name: String(domainName).toLowerCase(),
      type: "addon" as const,
      tenant_id,
      tenant_username: tenantUsername,
      node_id,
      node_name: nodeName,
      docroot: `/domains/${String(domainName).toLowerCase()}/public_html`,
      is_primary: false,
      status: "active" as const,
      ssl_status: enable_ssl ? ("pending" as const) : ("none" as const),
      dns_status: "pending" as const,
      redirect: null,
      php_version: "PHP 8.3",
      created_at: new Date().toISOString(),
    }));

    mockAdminDomains = [...newItems, ...mockAdminDomains];

    // Register successful background operation in mock store
    mockOperations.unshift({
      id: opId,
      idempotency_key: `idemp_${opId}`,
      op_type: "AttachDomain",
      node_id,
      tenant_id,
      status: "succeeded",
      retries_count: 0,
      payload: { count: domains.length, tenant_id, node_id },
      output: { message: `Successfully attached ${domains.length} domains`, count: domains.length },
      error: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    });

    return HttpResponse.json({
      operation_id: opId,
      total_requested: domains.length,
      enqueued_count: domains.length,
      skipped_count: 0,
      status: "queued",
    });
  }),

  // Admin: Suspend domain (POST /api/v1/admin/domains/:id/suspend)
  http.post(/\/api\/v1\/admin\/domains\/([^/]+)\/suspend$/, ({ params }) => {
    const id = params[0] as string;
    const domain = mockAdminDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }
    domain.status = "suspended";
    domain.suspended_at = new Date().toISOString();
    return HttpResponse.json(domain);
  }),

  // Admin: Unsuspend domain (POST /api/v1/admin/domains/:id/unsuspend)
  http.post(/\/api\/v1\/admin\/domains\/([^/]+)\/unsuspend$/, ({ params }) => {
    const id = params[0] as string;
    const domain = mockAdminDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }
    domain.status = "active";
    domain.suspended_at = null;
    return HttpResponse.json(domain);
  }),

  // Admin: Bulk suspend (POST /api/v1/admin/domains/bulk-suspend)
  http.post(/\/api\/v1\/admin\/domains\/bulk-suspend$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const ids: string[] = body?.ids || [];
    const now = new Date().toISOString();
    mockAdminDomains.forEach((d) => {
      if (ids.includes(d.id)) {
        d.status = "suspended";
        d.suspended_at = now;
      }
    });
    return HttpResponse.json({ success: true, count: ids.length });
  }),

  // Admin: Bulk unsuspend (POST /api/v1/admin/domains/bulk-unsuspend)
  http.post(/\/api\/v1\/admin\/domains\/bulk-unsuspend$/, async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as any;
    const ids: string[] = body?.ids || [];
    mockAdminDomains.forEach((d) => {
      if (ids.includes(d.id)) {
        d.status = "active";
        d.suspended_at = null;
      }
    });
    return HttpResponse.json({ success: true, count: ids.length });
  }),

  // Admin: Delete domain (DELETE /api/v1/admin/domains/:id)
  http.delete(/\/api\/v1\/admin\/domains\/([^/]+)$/, ({ params }) => {
    const id = params[0] as string;
    const domain = mockAdminDomains.find((d) => d.id === id);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }
    mockAdminDomains = mockAdminDomains.filter((d) => d.id !== id);
    return HttpResponse.json({ status: "deleted", id });
  }),

  // DNS: List zones (GET /api/v1/client/dns/zones)
  // TODO(api:T05.10): Replace with real DNS zone list endpoint
  http.get(/\/api\/v1\/client\/dns\/zones$/, () => {
    return HttpResponse.json({
      items: mockDnsZones,
      total: mockDnsZones.length,
    });
  }),

  // DNS: Get zone detail with records (GET /api/v1/client/dns/zones/:id)
  // TODO(api:T05.10): Replace with real DNS zone detail endpoint
  http.get(/\/api\/v1\/client\/dns\/zones\/([^/]+)$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const zone = mockDnsZones.find((z) => z.id === key || z.domain_id === key || z.domain_name === key);
    if (!zone) {
      return HttpResponse.json({ message: "DNS zone not found" }, { status: 404 });
    }
    const records = mockDnsRecords.filter((r) => r.zone_id === zone.id);
    return HttpResponse.json({
      zone,
      records,
    });
  }),

  // DNS: Create record (POST /api/v1/client/dns/zones/:id/records)
  // TODO(api:T05.10): Replace with real DNS create record endpoint
  http.post(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/records$/, async ({ params, request }) => {
    const key = (params.id || params[0]) as string;
    const zone = mockDnsZones.find((z) => z.id === key || z.domain_id === key || z.domain_name === key);
    if (!zone) {
      return HttpResponse.json({ message: "DNS zone not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as any;
    const { name, type, content, ttl = 3600, priority = null, weight = null, port = null, flags = null, tag = null } = body || {};

    if (!name || !type || !content) {
      return HttpResponse.json({ message: "Name, type, and content are required" }, { status: 422 });
    }

    // Validation: CNAME conflict check
    if (type === "CNAME") {
      const existing = mockDnsRecords.find((r) => r.zone_id === zone.id && r.name.toLowerCase() === String(name).toLowerCase());
      if (existing) {
        return HttpResponse.json(
          { message: "CNAME cannot coexist with other records for the same name" },
          { status: 422 }
        );
      }
    } else {
      const existingCname = mockDnsRecords.find((r) => r.zone_id === zone.id && r.name.toLowerCase() === String(name).toLowerCase() && r.type === "CNAME");
      if (existingCname) {
        return HttpResponse.json(
          { message: "Cannot add record: A CNAME record already exists for this name" },
          { status: 422 }
        );
      }
    }

    const newRecord: MockDnsRecord = {
      id: `rec_${Date.now()}_${Math.floor(Math.random() * 1000)}`,
      zone_id: zone.id,
      name: String(name).trim(),
      type: String(type).toUpperCase(),
      content: String(content).trim(),
      ttl: Number(ttl) || 3600,
      priority: priority !== undefined ? Number(priority) : null,
      weight: weight !== undefined ? Number(weight) : null,
      port: port !== undefined ? Number(port) : null,
      flags: flags !== undefined ? Number(flags) : null,
      tag: tag || null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    mockDnsRecords = [newRecord, ...mockDnsRecords];
    zone.records_count = mockDnsRecords.filter((r) => r.zone_id === zone.id).length;
    zone.soa_serial += 1;
    zone.updated_at = new Date().toISOString();

    return HttpResponse.json(newRecord, { status: 201 });
  }),

  // DNS: Update record (PUT /api/v1/client/dns/zones/:id/records/:recordId)
  // TODO(api:T05.10): Replace with real DNS update record endpoint
  http.put(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/records\/([^/]+)$/, async ({ params, request }) => {
    const recordId = params[1] as string;

    const record = mockDnsRecords.find((r) => r.id === recordId);
    if (!record) {
      return HttpResponse.json({ message: "DNS record not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as any;
    if (body.name !== undefined) record.name = String(body.name).trim();
    if (body.type !== undefined) record.type = String(body.type).toUpperCase();
    if (body.content !== undefined) record.content = String(body.content).trim();
    if (body.ttl !== undefined) record.ttl = Number(body.ttl);
    if (body.priority !== undefined) record.priority = body.priority !== null ? Number(body.priority) : null;
    if (body.weight !== undefined) record.weight = body.weight !== null ? Number(body.weight) : null;
    if (body.port !== undefined) record.port = body.port !== null ? Number(body.port) : null;
    if (body.flags !== undefined) record.flags = body.flags !== null ? Number(body.flags) : null;
    if (body.tag !== undefined) record.tag = body.tag || null;
    record.updated_at = new Date().toISOString();

    const zone = mockDnsZones.find((z) => z.id === record.zone_id);
    if (zone) {
      zone.soa_serial += 1;
      zone.updated_at = new Date().toISOString();
    }

    return HttpResponse.json(record);
  }),

  // DNS: Delete record (DELETE /api/v1/client/dns/zones/:id/records/:recordId)
  // TODO(api:T05.10): Replace with real DNS delete record endpoint
  http.delete(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/records\/([^/]+)$/, ({ params }) => {
    const recordId = params[1] as string;
    const record = mockDnsRecords.find((r) => r.id === recordId);
    if (!record) {
      return HttpResponse.json({ message: "DNS record not found" }, { status: 404 });
    }

    mockDnsRecords = mockDnsRecords.filter((r) => r.id !== recordId);
    const zone = mockDnsZones.find((z) => z.id === record.zone_id);
    if (zone) {
      zone.records_count = mockDnsRecords.filter((r) => r.zone_id === zone.id).length;
      zone.soa_serial += 1;
      zone.updated_at = new Date().toISOString();
    }

    return HttpResponse.json({ status: "deleted", id: recordId });
  }),

  // DNS: Export BIND zone (POST /api/v1/client/dns/zones/:id/export)
  // TODO(api:T05.10): Replace with real BIND export endpoint
  http.post(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/export$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const zone = mockDnsZones.find((z) => z.id === key || z.domain_id === key || z.domain_name === key);
    if (!zone) {
      return HttpResponse.json({ message: "DNS zone not found" }, { status: 404 });
    }

    const records = mockDnsRecords.filter((r) => r.zone_id === zone.id);
    const lines = [
      `; BIND Zone File for ${zone.domain_name}`,
      `; Exported from DHPanel at ${new Date().toISOString()}`,
      `$ORIGIN ${zone.domain_name}.`,
      `$TTL 3600`,
      `@   IN  SOA ${zone.nameservers[0]}. hostmaster.${zone.domain_name}. (`,
      `        ${zone.soa_serial} ; Serial`,
      `        7200       ; Refresh`,
      `        3600       ; Retry`,
      `        1209600    ; Expire`,
      `        3600 )     ; Minimum TTL`,
      `;`,
      `; Nameservers`,
      ...zone.nameservers.map((ns) => `@   IN  NS  ${ns}.`),
      `;`,
      `; Resource Records`,
      ...records.map((r) => {
        const prio = r.priority !== null && r.priority !== undefined ? `${r.priority} ` : "";
        return `${r.name.padEnd(16)} IN  ${r.type.padEnd(6)} ${prio}${r.content}`;
      }),
    ];

    return HttpResponse.json({
      zone_name: zone.domain_name,
      soa_serial: zone.soa_serial,
      bind_text: lines.join("\n"),
    });
  }),

  // DNS: Import BIND zone (POST /api/v1/client/dns/zones/:id/import)
  // TODO(api:T05.10): Replace with real BIND import endpoint
  http.post(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/import$/, async ({ params, request }) => {
    const key = (params.id || params[0]) as string;
    const zone = mockDnsZones.find((z) => z.id === key || z.domain_id === key || z.domain_name === key);
    if (!zone) {
      return HttpResponse.json({ message: "DNS zone not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as any;
    const { bind_text = "", overwrite = false } = body || {};

    if (!bind_text.trim()) {
      return HttpResponse.json({ message: "BIND zone content is required" }, { status: 422 });
    }

    const lines = bind_text.split("\n");
    const newRecords: MockDnsRecord[] = [];
    const warnings: string[] = [];

    for (let line of lines) {
      line = line.trim();
      if (!line || line.startsWith(";") || line.startsWith("$")) continue;
      const commentIdx = line.indexOf(";");
      if (commentIdx !== -1) line = line.substring(0, commentIdx).trim();

      const tokens = line.split(/\s+/);
      if (tokens.length < 3) continue;

      const name = tokens[0];
      let idx = 1;
      let ttl = 3600;

      if (/^\d+$/.test(tokens[idx])) {
        ttl = parseInt(tokens[idx], 10);
        idx++;
      }
      if (tokens[idx]?.toUpperCase() === "IN") {
        idx++;
      }

      const type = tokens[idx]?.toUpperCase();
      idx++;
      if (!["A", "AAAA", "CNAME", "MX", "TXT", "SRV", "CAA", "NS"].includes(type)) {
        continue;
      }

      let priority: number | null = null;
      if (type === "MX" && /^\d+$/.test(tokens[idx])) {
        priority = parseInt(tokens[idx], 10);
        idx++;
      }

      const content = tokens.slice(idx).join(" ");
      if (!content) continue;

      newRecords.push({
        id: `rec_imp_${Date.now()}_${newRecords.length}`,
        zone_id: zone.id,
        name,
        type,
        content,
        ttl,
        priority,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      });
    }

    if (overwrite) {
      mockDnsRecords = [...newRecords, ...mockDnsRecords.filter((r) => r.zone_id !== zone.id)];
    } else {
      mockDnsRecords = [...newRecords, ...mockDnsRecords];
    }

    zone.records_count = mockDnsRecords.filter((r) => r.zone_id === zone.id).length;
    zone.soa_serial += 1;
    zone.updated_at = new Date().toISOString();

    return HttpResponse.json({
      imported_count: newRecords.length,
      skipped_count: 0,
      warnings,
      records: mockDnsRecords.filter((r) => r.zone_id === zone.id),
    });
  }),

  // DNS: Verify NS resolution (POST /api/v1/client/dns/zones/:id/verify-ns)
  // TODO(api:T05.10): Replace with real NS verification endpoint
  http.post(/\/api\/v1\/client\/dns\/zones\/([^/]+)\/verify-ns$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const zone = mockDnsZones.find((z) => z.id === key || z.domain_id === key || z.domain_name === key);
    if (!zone) {
      return HttpResponse.json({ message: "DNS zone not found" }, { status: 404 });
    }

    return HttpResponse.json({
      valid: true,
      delegated: true,
      nameservers: zone.nameservers,
      queried_at: new Date().toISOString(),
      status: "active",
      message: "Nameservers successfully verified and delegating properly.",
    });
  }),

  // SSL: List all certificates (GET /api/v1/client/ssl)
  // TODO(api:T05.16): Replace with real backend SSL list endpoint
  http.get(/\/api\/v1\/client\/ssl$/, () => {
    return HttpResponse.json({
      items: mockSslCertificates,
      total: mockSslCertificates.length,
    });
  }),

  // SSL: Get domain certificate (GET /api/v1/client/ssl/:domainId)
  // TODO(api:T05.16): Replace with real backend SSL detail endpoint
  http.get(/\/api\/v1\/client\/ssl\/([^/]+)$/, ({ params }) => {
    const key = (params.domainId || params[0]) as string;
    const cert = mockSslCertificates.find(
      (c) => c.domain_id === key || c.domain_name === key || c.id === key
    );
    if (!cert) {
      return HttpResponse.json({ message: "Certificate not found" }, { status: 404 });
    }
    return HttpResponse.json(cert);
  }),

  // SSL: Re-issue / Renew certificate via ACME (POST /api/v1/client/ssl/:domainId/reissue)
  // TODO(api:T05.16): Replace with real ACME reissue endpoint
  http.post(/\/api\/v1\/client\/ssl\/([^/]+)\/reissue$/, ({ params }) => {
    const key = (params.domainId || params[0]) as string;
    const cert = mockSslCertificates.find(
      (c) => c.domain_id === key || c.domain_name === key || c.id === key
    );
    if (!cert) {
      return HttpResponse.json({ message: "Certificate not found" }, { status: 404 });
    }

    const opId = `op_ssl_renew_${Date.now()}`;
    cert.status = "active";
    cert.type = "lets_encrypt";
    cert.issuer = "Let's Encrypt Authority X3";
    cert.expires_at = new Date(Date.now() + 90 * 86400 * 1000).toISOString();
    cert.days_remaining = 90;
    cert.auto_renew = true;
    cert.updated_at = new Date().toISOString();

    return HttpResponse.json({
      success: true,
      operation_id: opId,
      status: "succeeded",
      message: `SSL Certificate for ${cert.domain_name} renewed successfully. Valid for 90 days.`,
      certificate: cert,
    });
  }),

  // SSL: Custom certificate upload (POST /api/v1/client/ssl/:domainId/custom)
  // TODO(api:T05.17): Replace with real custom cert upload endpoint
  http.post(/\/api\/v1\/client\/ssl\/([^/]+)\/custom$/, async ({ params, request }) => {
    const key = (params.domainId || params[0]) as string;
    const cert = mockSslCertificates.find(
      (c) => c.domain_id === key || c.domain_name === key || c.id === key
    );
    if (!cert) {
      return HttpResponse.json({ message: "Certificate not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as any;
    const { cert_pem = "", key_pem = "" } = body || {};

    if (!cert_pem.includes("BEGIN CERTIFICATE") || !cert_pem.includes("END CERTIFICATE")) {
      return HttpResponse.json(
        { message: "Invalid PEM format: Missing 'BEGIN CERTIFICATE' or 'END CERTIFICATE' header" },
        { status: 422 }
      );
    }

    if (!key_pem.includes("BEGIN PRIVATE KEY") && !key_pem.includes("BEGIN RSA PRIVATE KEY")) {
      return HttpResponse.json(
        { message: "Invalid PEM format: Missing 'BEGIN PRIVATE KEY' header" },
        { status: 422 }
      );
    }

    // Check for simulated key mismatch
    if (key_pem.includes("MISMATCH_KEY")) {
      return HttpResponse.json(
        { message: "Private key does not match the provided public certificate modulus" },
        { status: 422 }
      );
    }

    cert.status = "active";
    cert.type = "custom";
    cert.issuer = "Custom Uploaded CA";
    cert.expires_at = new Date(Date.now() + 365 * 86400 * 1000).toISOString();
    cert.days_remaining = 365;
    cert.auto_renew = false;
    cert.cert_fingerprint = `SHA256:custom:${Date.now().toString(16)}`;
    cert.updated_at = new Date().toISOString();

    return HttpResponse.json({
      success: true,
      message: `Custom SSL certificate installed successfully for ${cert.domain_name}.`,
      certificate: cert,
    });
  }),

  // SSL: Update settings like force_https, HSTS, auto_renew (PUT /api/v1/client/ssl/:domainId/settings)
  // TODO(api:T05.18): Replace with real SSL settings endpoint
  http.put(/\/api\/v1\/client\/ssl\/([^/]+)\/settings$/, async ({ params, request }) => {
    const key = (params.domainId || params[0]) as string;
    const cert = mockSslCertificates.find(
      (c) => c.domain_id === key || c.domain_name === key || c.id === key
    );
    if (!cert) {
      return HttpResponse.json({ message: "Certificate not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as any;
    if (body.auto_renew !== undefined) cert.auto_renew = Boolean(body.auto_renew);
    if (body.force_https !== undefined) cert.force_https = Boolean(body.force_https);
    if (body.hsts_enabled !== undefined) cert.hsts_enabled = Boolean(body.hsts_enabled);
    if (body.hsts_subdomains !== undefined) cert.hsts_subdomains = Boolean(body.hsts_subdomains);
    if (body.hsts_preload !== undefined) cert.hsts_preload = Boolean(body.hsts_preload);
    cert.updated_at = new Date().toISOString();

    return HttpResponse.json(cert);
  }),

  // SSL: Revoke / Revert to None (DELETE /api/v1/client/ssl/:domainId)
  // TODO(api:T05.17): Replace with real SSL revoke endpoint
  http.delete(/\/api\/v1\/client\/ssl\/([^/]+)$/, ({ params }) => {
    const key = (params.domainId || params[0]) as string;
    const cert = mockSslCertificates.find(
      (c) => c.domain_id === key || c.domain_name === key || c.id === key
    );
    if (!cert) {
      return HttpResponse.json({ message: "Certificate not found" }, { status: 404 });
    }

    cert.status = "none";
    cert.type = "none";
    cert.issuer = null;
    cert.expires_at = null;
    cert.days_remaining = 0;
    cert.auto_renew = false;
    cert.force_https = false;
    cert.hsts_enabled = false;
    cert.updated_at = new Date().toISOString();

    return HttpResponse.json({
      success: true,
      status: "revoked",
      id: cert.id,
      certificate: cert,
    });
  }),

  // VHost Templates: List all (GET /api/v1/admin/vhost-templates)
  // TODO(api:T06.07): Replace with real backend vhost templates list endpoint
  http.get(/\/api\/v1\/admin\/vhost-templates$/, () => {
    return HttpResponse.json({
      items: mockVhostTemplates,
      total: mockVhostTemplates.length,
    });
  }),

  // VHost Templates: Get by ID (GET /api/v1/admin/vhost-templates/:id)
  // TODO(api:T06.07): Replace with real backend vhost template detail endpoint
  http.get(/\/api\/v1\/admin\/vhost-templates\/([^/]+)$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const tpl = mockVhostTemplates.find((t) => t.id === key || t.slug === key);
    if (!tpl) {
      return HttpResponse.json({ message: "VHost template not found" }, { status: 404 });
    }
    return HttpResponse.json(tpl);
  }),

  // VHost Templates: Update template content (PUT /api/v1/admin/vhost-templates/:id)
  // TODO(api:T06.07): Replace with real backend vhost template update endpoint
  http.put(/\/api\/v1\/admin\/vhost-templates\/([^/]+)$/, async ({ params, request }) => {
    const key = (params.id || params[0]) as string;
    const tpl = mockVhostTemplates.find((t) => t.id === key || t.slug === key);
    if (!tpl) {
      return HttpResponse.json({ message: "VHost template not found" }, { status: 404 });
    }

    const body = (await request.json()) as { content?: string; changelog?: string; name?: string };

    // Check for forbidden directives (T06.06 security rule)
    if (body.content) {
      const forbiddenRules = [
        { pattern: /include\s+\//i, msg: "Forbidden directive: 'include /' is restricted" },
        { pattern: /alias\s+\/etc/i, msg: "Forbidden directive: 'alias /etc' is restricted" },
      ];
      for (const rule of forbiddenRules) {
        if (rule.pattern.test(body.content)) {
          return HttpResponse.json(
            { message: rule.msg, code: "forbidden_directive" },
            { status: 422 }
          );
        }
      }
    }

    if (body.name) tpl.name = body.name;
    if (body.content !== undefined) {
      tpl.content = body.content;
      tpl.version += 1;
      tpl.updated_at = new Date().toISOString();

      // Add to version history
      mockVhostVersions.unshift({
        id: `ver_${Date.now()}`,
        template_id: tpl.id,
        version: tpl.version,
        content: tpl.content,
        changelog: body.changelog || `Updated template to version ${tpl.version}`,
        created_by: "admin",
        created_at: new Date().toISOString(),
      });
    }

    return HttpResponse.json(tpl);
  }),

  // VHost Templates: Get Version History (GET /api/v1/admin/vhost-templates/:id/versions)
  // TODO(api:T06.07): Replace with real backend vhost versions list endpoint
  http.get(/\/api\/v1\/admin\/vhost-templates\/([^/]+)\/versions$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const versions = mockVhostVersions.filter((v) => v.template_id === key);
    return HttpResponse.json({
      items: versions,
      total: versions.length,
    });
  }),

  // VHost Templates: Revert to Version (POST /api/v1/admin/vhost-templates/:id/versions/:verId/revert)
  // TODO(api:T06.07): Replace with real backend vhost revert endpoint
  http.post(/\/api\/v1\/admin\/vhost-templates\/([^/]+)\/versions\/([^/]+)\/revert$/, ({ params }) => {
    const tplKey = (params.id || params[0]) as string;
    const verKey = (params.verId || params[1]) as string;

    const tpl = mockVhostTemplates.find((t) => t.id === tplKey || t.slug === tplKey);
    if (!tpl) {
      return HttpResponse.json({ message: "VHost template not found" }, { status: 404 });
    }

    const targetVer = mockVhostVersions.find((v) => v.id === verKey || String(v.version) === verKey);
    if (!targetVer) {
      return HttpResponse.json({ message: "Template version not found" }, { status: 404 });
    }

    tpl.content = targetVer.content;
    tpl.version += 1;
    tpl.updated_at = new Date().toISOString();

    mockVhostVersions.unshift({
      id: `ver_${Date.now()}`,
      template_id: tpl.id,
      version: tpl.version,
      content: tpl.content,
      changelog: `Reverted to revision v${targetVer.version}`,
      created_by: "admin",
      created_at: new Date().toISOString(),
    });

    return HttpResponse.json({
      success: true,
      message: `Template reverted to v${targetVer.version}`,
      template: tpl,
    });
  }),

  // VHost Templates: Live Preview & Validation (POST /api/v1/admin/vhost-templates/:id/preview)
  // TODO(api:T06.07): Replace with real backend vhost preview endpoint
  http.post(/\/api\/v1\/admin\/vhost-templates\/([^/]+)\/preview$/, async ({ request }) => {
    const body = (await request.json()) as {
      content: string;
      sample_variables?: Record<string, string>;
    };

    const vars: Record<string, string> = {
      domain: "example.com",
      docroot: "/home/demo/public_html",
      php_socket: "/run/php/php8.3-fpm-demo.sock",
      ssl_cert: "/etc/ssl/certs/example.com.crt",
      ssl_key: "/etc/ssl/private/example.com.key",
      port: "80",
      ...(body.sample_variables || {}),
    };

    let rendered = body.content || "";
    for (const [k, v] of Object.entries(vars)) {
      const reg = new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, "g");
      rendered = rendered.replace(reg, v);
    }

    return HttpResponse.json({
      rendered,
      valid: true,
      errors: [],
    });
  }),

  // VHost Templates: Apply to All Domains (POST /api/v1/admin/vhost-templates/:id/apply-all)
  // TODO(api:T06.07): Replace with real backend vhost apply-all endpoint
  http.post(/\/api\/v1\/admin\/vhost-templates\/([^/]+)\/apply-all$/, ({ params }) => {
    const key = (params.id || params[0]) as string;
    const tpl = mockVhostTemplates.find((t) => t.id === key || t.slug === key);
    if (!tpl) {
      return HttpResponse.json({ message: "VHost template not found" }, { status: 404 });
    }

    const opId = `op_vhost_apply_${Date.now()}`;
    const targetCount = tpl.domain_count || 4;

    const op: any = {
      id: opId,
      idempotency_key: `idemp_${opId}`,
      op_type: "ApplyVhostTemplateAll",
      node_id: "node_01",
      node_name: "node-primary-01",
      tenant_id: null,
      status: "succeeded",
      retries_count: 0,
      payload: { template_id: tpl.id, target_count: targetCount },
      output: { applied_domains: targetCount, reloaded_server: tpl.server_type },
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };

    mockOperations.unshift(op);

    return HttpResponse.json({
      success: true,
      operation_id: opId,
      target_count: targetCount,
      message: `VHost template '${tpl.name}' deployed across all ${targetCount} domains.`,
    });
  }),

  // PHP: Get Client PHP Configuration (GET /api/v1/client/php/config)
  // TODO(api:T06.08): Replace with real backend client PHP config endpoint
  http.get(/\/api\/v1\/client\/php\/config$/, () => {
    return HttpResponse.json(mockClientPhpConfig);
  }),

  // PHP: Update PHP Version (PUT /api/v1/client/php/version)
  // TODO(api:T06.09): Replace with real backend set php version endpoint
  http.put(/\/api\/v1\/client\/php\/version$/, async ({ request }) => {
    const body = (await request.json()) as { version: string; domain_id?: string };
    const ver = mockClientPhpConfig.supported_versions.find((v) => v.version === body.version);
    if (!ver) {
      return HttpResponse.json({ message: `Unsupported PHP version: ${body.version}` }, { status: 400 });
    }

    if (body.domain_id) {
      const dom = mockClientPhpConfig.domain_configs.find((d) => d.domain_id === body.domain_id);
      if (dom) {
        dom.php_version = body.version;
        dom.is_custom = body.version !== mockClientPhpConfig.default_version;
      }
    } else {
      mockClientPhpConfig.default_version = body.version;
      mockClientPhpConfig.supported_versions.forEach((v) => {
        v.is_default = v.version === body.version;
      });
    }

    return HttpResponse.json({
      success: true,
      message: `PHP version set to ${body.version} successfully.`,
      config: mockClientPhpConfig,
    });
  }),

  // PHP: Update php.ini options (PUT /api/v1/client/php/ini)
  // TODO(api:T06.10): Replace with real backend php.ini update endpoint
  http.put(/\/api\/v1\/client\/php\/ini$/, async ({ request }) => {
    const body = (await request.json()) as Record<string, any>;

    // T06.10: Validate against plan limits (e.g. max_memory_limit 512M, max_execution_time 300)
    if (body.max_execution_time !== undefined && Number(body.max_execution_time) > mockClientPhpConfig.plan_limits.max_execution_time) {
      return HttpResponse.json(
        { message: `max_execution_time exceeds plan limit of ${mockClientPhpConfig.plan_limits.max_execution_time}s`, code: "plan_limit_exceeded" },
        { status: 422 }
      );
    }

    const parseM = (s: string) => parseInt(s.replace(/[^0-9]/g, ""), 10) || 0;
    if (body.memory_limit && parseM(body.memory_limit) > parseM(mockClientPhpConfig.plan_limits.max_memory_limit)) {
      return HttpResponse.json(
        { message: `memory_limit exceeds plan limit of ${mockClientPhpConfig.plan_limits.max_memory_limit}`, code: "plan_limit_exceeded" },
        { status: 422 }
      );
    }

    mockClientPhpConfig.ini_settings = {
      ...mockClientPhpConfig.ini_settings,
      ...body,
    };

    return HttpResponse.json({
      success: true,
      message: "php.ini directives updated successfully.",
      ini_settings: mockClientPhpConfig.ini_settings,
    });
  }),

  // PHP: Reset php.ini options to Plan Defaults (POST /api/v1/client/php/ini/reset)
  // TODO(api:T06.10): Replace with real backend php.ini reset endpoint
  http.post(/\/api\/v1\/client\/php\/ini\/reset$/, () => {
    mockClientPhpConfig.ini_settings = { ...mockClientPhpConfig.plan_defaults };
    return HttpResponse.json({
      success: true,
      message: "php.ini directives reset to hosting plan defaults.",
      ini_settings: mockClientPhpConfig.ini_settings,
    });
  }),

  // Services: List web services (GET /api/v1/client/services)
  // TODO(api:T06.12): Replace with real backend client services endpoint
  http.get(/\/api\/v1\/client\/services$/, () => {
    const now = Date.now();
    const items = mockServices.map((srv) => {
      const recent = (mockServiceRestartsLog[srv.name] || []).filter((t) => now - t < 60000);
      return {
        ...srv,
        restarts_count_this_minute: recent.length,
      };
    });
    return HttpResponse.json({
      items,
      total: items.length,
    });
  }),

  // Services: Restart service (POST /api/v1/client/services/:serviceName/restart)
  // TODO(api:T06.12): Replace with real backend service restart endpoint
  http.post(/\/api\/v1\/client\/services\/([^/]+)\/restart$/, ({ params }) => {
    const serviceName = (params.serviceName || params[0]) as "nginx" | "php-fpm";
    const srv = mockServices.find((s) => s.name === serviceName);
    if (!srv) {
      return HttpResponse.json({ message: `Service ${serviceName} not found` }, { status: 404 });
    }

    const now = Date.now();
    const recent = (mockServiceRestartsLog[serviceName] || []).filter((t) => now - t < 60000);

    // T06.12 DoD: Rate limit 3 restarts per minute -> 429
    if (recent.length >= 3) {
      const oldest = recent[0];
      const retryAfter = Math.max(1, Math.ceil((oldest + 60000 - now) / 1000));
      return HttpResponse.json(
        {
          error: "rate_limit_exceeded",
          message: "Rate limit exceeded: You can restart this service at most 3 times per minute.",
          retry_after_seconds: retryAfter,
        },
        { status: 429 }
      );
    }

    recent.push(now);
    mockServiceRestartsLog[serviceName] = recent;
    srv.last_restarted_at = new Date().toISOString();
    srv.uptime_seconds = 0;

    return HttpResponse.json({
      success: true,
      message: `${srv.display_name} restarted successfully.`,
      service: {
        ...srv,
        restarts_count_this_minute: recent.length,
      },
    });
  }),

  // Error & Suspended Page Templates: List all (GET /api/v1/admin/templates/pages)
  // TODO(api:T06.13): Replace with real backend error page templates endpoint
  http.get(/\/api\/v1\/admin\/templates\/pages$/, () => {
    return HttpResponse.json({
      items: mockPageTemplates,
      total: mockPageTemplates.length,
    });
  }),

  // Error & Suspended Page Templates: Get single (GET /api/v1/admin/templates/pages/:type)
  // TODO(api:T06.13): Replace with real backend template detail endpoint
  http.get(/\/api\/v1\/admin\/templates\/pages\/([^/]+)$/, ({ params }) => {
    const type = (params.type || params[0]) as string;
    const tpl = mockPageTemplates.find((t) => t.type === type || t.id === type);
    if (!tpl) {
      return HttpResponse.json({ message: "Page template not found" }, { status: 404 });
    }
    return HttpResponse.json(tpl);
  }),

  // Error & Suspended Page Templates: Update template (PUT /api/v1/admin/templates/pages/:type)
  // TODO(api:T06.13): Replace with real backend template update endpoint
  http.put(/\/api\/v1\/admin\/templates\/pages\/([^/]+)$/, async ({ params, request }) => {
    const type = (params.type || params[0]) as string;
    const tpl = mockPageTemplates.find((t) => t.type === type || t.id === type);
    if (!tpl) {
      return HttpResponse.json({ message: "Page template not found" }, { status: 404 });
    }

    const body = (await request.json()) as { content?: string };
    if (typeof body.content === "string") {
      tpl.content = body.content;
      tpl.isCustomized = true;
      tpl.updatedAt = new Date().toISOString();
    }

    return HttpResponse.json(tpl);
  }),

  // Error & Suspended Page Templates: Reset template (POST /api/v1/admin/templates/pages/:type/reset)
  // TODO(api:T06.13): Replace with real backend template reset endpoint
  http.post(/\/api\/v1\/admin\/templates\/pages\/([^/]+)\/reset$/, ({ params }) => {
    const type = (params.type || params[0]) as string;
    const tpl = mockPageTemplates.find((t) => t.type === type || t.id === type);
    if (!tpl) {
      return HttpResponse.json({ message: "Page template not found" }, { status: 404 });
    }

    tpl.content = tpl.defaultContent;
    tpl.isCustomized = false;
    tpl.updatedAt = new Date().toISOString();

    return HttpResponse.json(tpl);
  }),

  // Temporary Preview Link: Get status (GET /api/v1/client/domains/:id/preview-link)
  // TODO(api:T06.14): Replace with real backend preview link status endpoint
  http.get(/\/api\/v1\/client\/domains\/([^/]+)\/preview-link$/, ({ params }) => {
    const domainId = (params.id || params[0]) as string;
    const domain = mockClientDomains.find((d) => d.id === domainId);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    const link = mockPreviewLinks[domainId];
    if (!link || !link.enabled) {
      return HttpResponse.json({
        enabled: false,
        domain_id: domainId,
        target_domain: domain.name,
      });
    }

    const isExpired = Date.now() > new Date(link.expires_at).getTime();
    return HttpResponse.json({
      ...link,
      is_expired: isExpired,
    });
  }),

  // Temporary Preview Link: Generate / Extend (POST /api/v1/client/domains/:id/preview-link)
  // TODO(api:T06.14): Replace with real backend preview link generation endpoint
  http.post(/\/api\/v1\/client\/domains\/([^/]+)\/preview-link$/, async ({ params, request }) => {
    const domainId = (params.id || params[0]) as string;
    const domain = mockClientDomains.find((d) => d.id === domainId);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    const body = (await request.json().catch(() => ({}))) as { ttl_hours?: number };
    const ttl = body.ttl_hours && body.ttl_hours > 0 ? body.ttl_hours : 24;

    const token = `prv_${Math.random().toString(36).substring(2, 8)}`;
    const now = Date.now();
    const expiresAt = new Date(now + ttl * 3600 * 1000).toISOString();

    const link: MockPreviewLink = {
      domain_id: domainId,
      target_domain: domain.name,
      enabled: true,
      token,
      url: `https://${token}.preview.dhpanel.net`,
      created_at: new Date(now).toISOString(),
      expires_at: expiresAt,
      ttl_hours: ttl,
    };

    mockPreviewLinks[domainId] = link;

    return HttpResponse.json({
      ...link,
      is_expired: false,
    });
  }),

  // Temporary Preview Link: Revoke (DELETE /api/v1/client/domains/:id/preview-link)
  // TODO(api:T06.14): Replace with real backend preview link revocation endpoint
  http.delete(/\/api\/v1\/client\/domains\/([^/]+)\/preview-link$/, ({ params }) => {
    const domainId = (params.id || params[0]) as string;
    const domain = mockClientDomains.find((d) => d.id === domainId);
    if (!domain) {
      return HttpResponse.json({ message: "Domain not found" }, { status: 404 });
    }

    if (mockPreviewLinks[domainId]) {
      mockPreviewLinks[domainId].enabled = false;
    }

    return HttpResponse.json({
      success: true,
      message: "Preview link revoked",
    });
  }),

  // Temporary Preview Link Proxy: Test / Access Link (GET /api/v1/preview/:token)
  // TODO(api:T06.14): Replace with real backend preview proxy route
  http.get(/\/api\/v1\/preview\/([^/]+)$/, ({ params }) => {
    const token = (params.token || params[0]) as string;
    const link = Object.values(mockPreviewLinks).find((l) => l.token === token && l.enabled);
    if (!link) {
      return HttpResponse.json({ message: "Preview link not found" }, { status: 404 });
    }

    // T06.14 DoD: Expired preview link must not work (returns HTTP 410 Gone)
    const isExpired = Date.now() > new Date(link.expires_at).getTime();
    if (isExpired) {
      return HttpResponse.json(
        {
          error: "preview_link_expired",
          message: "Preview link has expired and is no longer valid.",
        },
        { status: 410 }
      );
    }

    return HttpResponse.json({
      status: 200,
      message: "Preview link active",
      target_domain: link.target_domain,
    });
  }),

  // Databases: List all databases, users, limits (GET /api/v1/client/databases)
  // TODO(api:T07.10): Replace with real backend client databases endpoint
  http.get(/\/api\/v1\/client\/databases$/, () => {
    const totalBytes = mockDatabases.reduce((acc, db) => acc + db.size_bytes, 0);
    const sizeMb = Number((totalBytes / (1024 * 1024)).toFixed(1));

    return HttpResponse.json({
      databases: mockDatabases,
      users: mockDbUsers,
      limits: {
        max_databases: 5,
        used_databases: mockDatabases.length,
        max_db_users: 5,
        used_db_users: mockDbUsers.length,
        max_db_size_mb: 500,
        used_db_size_mb: sizeMb,
      },
      remote_db_hosts: mockRemoteDbHosts,
      phpmyadmin_url: "https://pma.dhpanel.net",
    });
  }),

  // Databases: Create database (POST /api/v1/client/databases)
  // TODO(api:T07.10): Replace with real backend database creation endpoint
  http.post(/\/api\/v1\/client\/databases$/, async ({ request }) => {
    const body = (await request.json()) as any;
    const name = String(body.name || "").trim();
    if (!name) {
      return HttpResponse.json({ message: "Database name is required" }, { status: 400 });
    }

    if (mockDatabases.some((d) => d.name === name)) {
      return HttpResponse.json({ message: `Database ${name} already exists` }, { status: 409 });
    }

    const newDb: MockDb = {
      id: `db_${Date.now()}`,
      name,
      raw_name: name.replace(/^demo_/, ""),
      prefix: "demo_",
      charset: body.charset || "utf8mb4",
      collation: body.collation || "utf8mb4_unicode_ci",
      size_bytes: 1048576,
      size_formatted: "1.0 MB",
      tables_count: 0,
      assigned_users: [],
      created_at: new Date().toISOString(),
    };

    mockDatabases = [newDb, ...mockDatabases];
    return HttpResponse.json(newDb, { status: 201 });
  }),

  // Databases: Delete database (DELETE /api/v1/client/databases/:id)
  // TODO(api:T07.10): Replace with real backend database deletion endpoint
  http.delete(/\/api\/v1\/client\/databases\/([^/]+)$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const target = mockDatabases.find((d) => d.id === id || d.name === id);
    if (!target) {
      return HttpResponse.json({ message: "Database not found" }, { status: 404 });
    }

    mockDatabases = mockDatabases.filter((d) => d.id !== target.id);
    // Remove references from users
    mockDbUsers.forEach((u) => {
      u.grants = u.grants.filter((g) => g.db_name !== target.name);
    });

    return HttpResponse.json({ success: true, message: `Database ${target.name} deleted` });
  }),

  // Databases: Create user (POST /api/v1/client/databases/users)
  // TODO(api:T07.10): Replace with real backend database user creation endpoint
  http.post(/\/api\/v1\/client\/databases\/users$/, async ({ request }) => {
    const body = (await request.json()) as any;
    const username = String(body.username || "").trim();
    if (!username) {
      return HttpResponse.json({ message: "Username is required" }, { status: 400 });
    }

    if (mockDbUsers.some((u) => u.username === username)) {
      return HttpResponse.json({ message: `User ${username} already exists` }, { status: 409 });
    }

    const assignments = body.assignments || [];
    const newUser: MockDbUser = {
      id: `dbusr_${Date.now()}`,
      username,
      raw_username: username.replace(/^demo_/, ""),
      prefix: "demo_",
      host: body.host || "10.0.0.%",
      grants: assignments,
      created_at: new Date().toISOString(),
    };

    mockDbUsers = [newUser, ...mockDbUsers];

    // Update assigned_users in databases
    assignments.forEach((a: any) => {
      const db = mockDatabases.find((d) => d.name === a.db_name);
      if (db && !db.assigned_users.includes(username)) {
        db.assigned_users.push(username);
      }
    });

    return HttpResponse.json(newUser, { status: 201 });
  }),

  // Databases: Delete user (DELETE /api/v1/client/databases/users/:id)
  // TODO(api:T07.10): Replace with real backend database user deletion endpoint
  http.delete(/\/api\/v1\/client\/databases\/users\/([^/]+)$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const target = mockDbUsers.find((u) => u.id === id || u.username === id);
    if (!target) {
      return HttpResponse.json({ message: "Database user not found" }, { status: 404 });
    }

    mockDbUsers = mockDbUsers.filter((u) => u.id !== target.id);
    mockDatabases.forEach((db) => {
      db.assigned_users = db.assigned_users.filter((u) => u !== target.username);
    });

    return HttpResponse.json({ success: true, message: `Database user ${target.username} deleted` });
  }),

  // Databases: Update user grants (PUT /api/v1/client/databases/users/:id/grants)
  // TODO(api:T07.10): Replace with real backend user grants update endpoint
  http.put(/\/api\/v1\/client\/databases\/users\/([^/]+)\/grants$/, async ({ params, request }) => {
    const id = (params.id || params[0]) as string;
    const user = mockDbUsers.find((u) => u.id === id || u.username === id);
    if (!user) {
      return HttpResponse.json({ message: "Database user not found" }, { status: 404 });
    }

    const body = (await request.json()) as { db_name: string; privileges: string[] };
    const { db_name, privileges } = body;

    const existingIdx = user.grants.findIndex((g) => g.db_name === db_name);
    if (privileges.length === 0) {
      if (existingIdx !== -1) {
        user.grants.splice(existingIdx, 1);
      }
      const db = mockDatabases.find((d) => d.name === db_name);
      if (db) {
        db.assigned_users = db.assigned_users.filter((u) => u !== user.username);
      }
    } else {
      if (existingIdx !== -1) {
        user.grants[existingIdx].privileges = privileges;
      } else {
        user.grants.push({ db_name, privileges });
      }
      const db = mockDatabases.find((d) => d.name === db_name);
      if (db && !db.assigned_users.includes(user.username)) {
        db.assigned_users.push(user.username);
      }
    }

    return HttpResponse.json(user);
  }),

  // Databases: Update remote hosts (PUT /api/v1/client/databases/remote-hosts)
  // TODO(api:T07.05): Replace with real backend remote db hosts update endpoint
  http.put(/\/api\/v1\/client\/databases\/remote-hosts$/, async ({ request }) => {
    const body = (await request.json()) as { remote_db_hosts: string[] };
    mockRemoteDbHosts = body.remote_db_hosts || [];
    return HttpResponse.json({
      success: true,
      remote_db_hosts: mockRemoteDbHosts,
    });
  }),

  // Databases: phpMyAdmin SSO (POST /api/v1/client/databases/phpmyadmin-sso)
  // TODO(api:T07.09): Replace with real backend phpMyAdmin SSO token endpoint
  http.post(/\/api\/v1\/client\/databases\/phpmyadmin-sso$/, () => {
    const ssoToken = `pma_sso_${Math.random().toString(36).substring(2, 10)}`;
    return HttpResponse.json({
      redirect_url: `https://pma.dhpanel.net/sso?token=${ssoToken}`,
      token: ssoToken,
      expires_in_seconds: 300,
    });
  }),

  // Databases: Export SQL Dump (POST /api/v1/client/databases/:id/export)
  // TODO(api:T07.08): Replace with real backend SQL dump export endpoint
  http.post(/\/api\/v1\/client\/databases\/([^/]+)\/export$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const db = mockDatabases.find((d) => d.id === id || d.name === id);
    if (!db) {
      return HttpResponse.json({ message: "Database not found" }, { status: 404 });
    }

    const filename = `${db.name}_dump_${Date.now()}.sql.gz`;
    return HttpResponse.json({
      download_url: `https://downloads.dhpanel.net/dumps/${filename}?signature=mock_signed_token`,
      filename,
      size_bytes: db.size_bytes,
      expires_at: new Date(Date.now() + 300 * 1000).toISOString(),
    });
  }),

  // Databases: Import SQL Dump (POST /api/v1/client/databases/:id/import)
  // TODO(api:T07.07): Replace with real backend SQL dump import endpoint
  http.post(/\/api\/v1\/client\/databases\/([^/]+)\/import$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const db = mockDatabases.find((d) => d.id === id || d.name === id);
    if (!db) {
      return HttpResponse.json({ message: "Database not found" }, { status: 404 });
    }

    db.tables_count = Math.max(db.tables_count + 4, 12);
    db.size_bytes += 5242880;
    db.size_formatted = `${(db.size_bytes / (1024 * 1024)).toFixed(1)} MB`;

    return HttpResponse.json({
      success: true,
      message: `SQL dump imported successfully into ${db.name}`,
      operation_id: `op_db_import_${Date.now()}`,
    });
  }),
];
