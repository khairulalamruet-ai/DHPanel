export interface ClientTenantInfo {
  id: string;
  username: string;
  email: string;
  primary_domain: string;
  plan_name: string;
  node_name: string;
  server_ip: string;
  nameservers: string[];
  created_at: string;
  status: "active" | "suspended" | "pending";
}

export interface ClientContainerInfo {
  status: "running" | "stopped" | "restarting" | "degraded";
  name: string;
  image: string;
  php_version: string;
  uptime: string;
  ip_address: string;
}

export interface ClientResources {
  disk_limit_mb: number;
  disk_used_mb: number;
  bandwidth_limit_mb: number;
  bandwidth_used_mb: number;
  memory_limit_mb: number;
  memory_used_mb: number;
  cpu_cores: number;
  cpu_used_pct: number;
  inodes_limit: number;
  inodes_used: number;
  databases_limit: number;
  databases_used: number;
  domains_limit: number;
  domains_used: number;
  emails_limit: number;
  emails_used: number;
}

export interface ClientPrimaryDomainInfo {
  domain: string;
  docroot: string;
  ssl_status: string;
  ssl_issuer: string;
  ssl_expires_in_days: number;
  php_version: string;
}

export interface ClientShortcut {
  id: string;
  title: string;
  icon: string;
  href: string;
  phase: number;
}

export interface ClientRecentEvent {
  id: string;
  action: string;
  timestamp: string;
  ip_address: string;
  description: string;
}

export interface ClientLastBackupInfo {
  created_at: string;
  size_mb: number;
  status: "completed" | "in_progress" | "failed" | "none";
  type: "automatic" | "manual";
}

export interface ClientSslWarning {
  domain: string;
  issuer: string;
  expires_in_days: number;
  severity: "warning" | "critical";
}

export interface ClientDashboardData {
  tenant: ClientTenantInfo;
  container: ClientContainerInfo;
  resources: ClientResources;
  primary_domain: ClientPrimaryDomainInfo;
  shortcuts: ClientShortcut[];
  recent_events: ClientRecentEvent[];
  last_backup?: ClientLastBackupInfo;
  ssl_warnings?: ClientSslWarning[];
}
