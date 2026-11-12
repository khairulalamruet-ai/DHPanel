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
  ...(() => {
    const mockOperations: any[] = [
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

    return [
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
        const { 0: id } = params as any;
        const op = mockOperations.find((o) => o.id === id);
        if (!op) {
          return HttpResponse.json({ error: "not_found", message: "Operation not found" }, { status: 404 });
        }
        return HttpResponse.json(op);
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
    ];
  })(),

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
    if (!user) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    user.status = "suspended";
    user.suspended_at = new Date().toISOString();
    return HttpResponse.json(user);
  }),

  // Users: Single unsuspend (POST /api/v1/admin/users/:id/unsuspend)
  // TODO(api:T04.12): Replace with real backend endpoint
  http.post(/\/api\/v1\/admin\/users\/([^/]+)\/unsuspend$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    const user = mockUsers.find((u) => u.id === id);
    if (!user) {
      return HttpResponse.json({ error: "not_found" }, { status: 404 });
    }
    user.status = "active";
    user.suspended_at = null;
    return HttpResponse.json(user);
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

  // Operations: Get operation status (GET /api/v1/admin/operations/:id)
  // TODO(api:T02.17): Replace with real ledger endpoint
  http.get(/\/api\/v1\/admin\/operations\/([^/]+)$/, ({ params }) => {
    const id = (params.id || params[0]) as string;
    return HttpResponse.json({
      id,
      status: "succeeded",
      action: "tenant.create",
      progress_pct: 100,
      container_status: "running",
      completed_at: new Date().toISOString(),
    });
  }),
];
