export type DashboardWidgetId =
  | "nodes_status"
  | "tenants_count"
  | "resource_usage"
  | "recent_operations"
  | "security_alerts"
  | "license_status";

export interface DashboardWidgetItem {
  id: DashboardWidgetId;
  enabled: boolean;
  order: number;
}

export interface DashboardLayoutConfig {
  widgets: DashboardWidgetItem[];
}

export interface AdminDashboardSummary {
  nodes: {
    total: number;
    online: number;
    avg_cpu_percent: number;
    avg_ram_percent: number;
  };
  tenants: {
    total: number;
    active: number;
    domains_count: number;
    databases_count: number;
  };
  resources: {
    cpu_percent: number;
    memory_used_mb: number;
    memory_total_mb: number;
    disk_used_gb: number;
    disk_total_gb: number;
    network_rx_mb: number;
    network_tx_mb: number;
  };
  operations: Array<{
    id: string;
    type: string;
    status: "pending" | "running" | "completed" | "failed";
    duration_ms?: number;
    created_at: string;
  }>;
  security: {
    blocked_threats_24h: number;
    active_waf_rules: number;
    ssh_bans: number;
    last_threat?: string;
  };
  license: {
    tier: string;
    status: "active" | "grace_period" | "restricted";
    signature_valid: boolean;
    expires_in_days: number;
  };
}
