export type TimeRange = "24h" | "7d" | "30d";

export interface UsageDataPoint {
  timestamp: string;
  label: string;
  cpu_percent: number;
  ram_mb: number;
  ram_percent: number;
  disk_gb: number;
  disk_percent: number;
  net_in_kbps: number;
  net_out_kbps: number;
}

export interface MetricStat {
  current: number;
  average: number;
  peak: number;
  limit: number;
  unit: string;
}

export interface ClientUsageSummary {
  range: TimeRange;
  cpu: MetricStat;
  ram: MetricStat;
  disk: MetricStat;
  bandwidth: MetricStat;
  history: UsageDataPoint[];
}

export interface TopTenantUsageItem {
  tenant_id: string;
  tenant_name: string;
  plan: string;
  cpu_percent: number;
  ram_mb: number;
  ram_limit_mb: number;
  disk_gb: number;
  disk_quota_gb: number;
  bandwidth_30d_gb: number;
  status: "normal" | "warning" | "critical";
}

export interface AdminUsageOverview {
  node_id: string;
  node_name: string;
  range: TimeRange;
  node_cpu: MetricStat;
  node_ram: MetricStat;
  node_disk: MetricStat;
  node_bandwidth: MetricStat;
  history: UsageDataPoint[];
  top_tenants_cpu: TopTenantUsageItem[];
  top_tenants_ram: TopTenantUsageItem[];
  top_tenants_disk: TopTenantUsageItem[];
  top_tenants_bandwidth: TopTenantUsageItem[];
}
