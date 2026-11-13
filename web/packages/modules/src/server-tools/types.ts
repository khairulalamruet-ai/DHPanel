export interface ServerMemoryInfo {
  total_mb: number;
  free_mb: number;
  available_mb: number;
  buffers_mb: number;
  cached_mb: number;
}

export interface ServerSwapInfo {
  total_mb: number;
  used_mb: number;
  free_mb: number;
  swappiness: number;
}

export interface ServerOverview {
  node_id: string;
  hostname: string;
  os: string;
  kernel: string;
  uptime_seconds: number;
  uptime_human: string;
  current_time: string;
  timezone: string;
  memory: ServerMemoryInfo;
  swap: ServerSwapInfo;
}

export interface SshConfigInfo {
  port: number;
  permit_root_login: "yes" | "no" | "prohibit-password";
  password_authentication: boolean;
  pubkey_authentication: boolean;
  max_auth_tries: number;
  authorized_keys_count: number;
  ciphers: string[];
  security_rating: "hardened" | "moderate" | "weak";
}

export interface ServerAuditEvent {
  id: string;
  action: string;
  actor: string;
  details: string;
  created_at: string;
  status: "success" | "failure";
}
