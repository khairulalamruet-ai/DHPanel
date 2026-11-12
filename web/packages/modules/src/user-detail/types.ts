export type UserDetailTab =
  | "overview"
  | "resources"
  | "domains"
  | "databases"
  | "logs"
  | "activity";

export interface UserResourceUsage {
  cpu_cores: number; // -1 for unlimited
  cpu_used_pct: number;
  memory_limit_mb: number;
  memory_used_mb: number;
  disk_limit_mb: number;
  disk_used_mb: number;
  inodes_limit: number;
  inodes_used: number;
  bandwidth_limit_mb: number;
  bandwidth_used_mb: number;
  pids_limit: number;
  pids_used: number;
}

export interface ContainerDetails {
  id: string;
  name: string;
  status: "running" | "stopped" | "starting";
  image: string;
  ip_address: string;
  started_at: string;
  uptime: string;
}

export interface TenantDomainItem {
  id: string;
  domain: string;
  is_primary: boolean;
  docroot: string;
  ssl_status: "active" | "pending" | "none";
  php_version: string;
}

export interface TenantDatabaseItem {
  id: string;
  name: string;
  type: "mysql" | "postgres";
  size_mb: number;
  charset: string;
}

export interface TenantLogEntry {
  id: string;
  timestamp: string;
  level: "info" | "warn" | "error";
  source: "nginx" | "php-fpm" | "system";
  message: string;
}

export interface TenantActivityEntry {
  id: string;
  action: string;
  actor: string;
  timestamp: string;
  status: "success" | "failure";
  details: string;
}

export interface UserDetail {
  id: string;
  username: string;
  email: string;
  status: "active" | "suspended" | "pending";
  uid: number;
  root_path: string;
  plan_id: string;
  plan_name: string;
  node_id: string;
  node_name: string;
  primary_domain?: string;
  created_at: string;
  suspended_at?: string | null;
  suspended_reason?: string | null;
  resources: UserResourceUsage;
  container: ContainerDetails;
  domains: TenantDomainItem[];
  databases: TenantDatabaseItem[];
  logs: TenantLogEntry[];
  activities: TenantActivityEntry[];
}
