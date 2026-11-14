export type NodeStatus = "online" | "offline" | "draining" | "revoked";

export interface NodeCapabilities {
  cpu_cores: number;
  cpu_model: string;
  cpu_usage_pct: number;
  ram_total_bytes: number;
  ram_used_bytes: number;
  disk_total_bytes: number;
  disk_used_bytes: number;
  kernel: string;
  docker_version: string;
  security_features: string[]; // e.g. ["seccomp", "apparmor", "userns"]
}

export interface NodeOversellConfig {
  cpu_ratio: number;  // e.g. 2.0 (allows allocating 2x physical cores)
  ram_ratio: number;  // e.g. 1.25 (allows allocating 1.25x physical RAM)
  disk_ratio: number; // e.g. 1.0 (no disk overcommit)
}

export interface NodeCapacity {
  allocated_cpu_cores: number;
  allocated_ram_bytes: number;
  allocated_disk_bytes: number;
  tenants_count: number;
  max_tenants?: number;
  saturation_pct: number; // 0-100% fullness / saturation percentage
  oversell: NodeOversellConfig;
}

export interface NodeItem {
  id: string;
  name: string;
  hostname: string;
  ip_address: string;
  status: NodeStatus;
  version: string;
  fingerprint: string;
  last_seen_at: string;
  enrolled_at: string;
  capabilities: NodeCapabilities;
  capacity?: NodeCapacity;
  tags?: string[];
  region?: string;
}

export type EnrollmentStatus = "pending" | "expired" | "consumed" | "revoked";

export interface EnrollmentView {
  id: string;
  name: string;
  created_at: string;
  created_by: string;
  expires_at: string;
  status: EnrollmentStatus;
}

export interface CreatedEnrollment {
  id: string;
  name: string;
  token: string;
  expires_at: string;
  expires_in: number;
  enroll_command?: string | null;
}

// Backwards-compatible alias if referenced elsewhere
export type EnrollmentTokenResponse = CreatedEnrollment;
