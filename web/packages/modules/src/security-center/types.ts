export type SecuritySeverity = "critical" | "warning" | "info";

export type SecurityCategory =
  | "auth"
  | "waf"
  | "malware"
  | "ssl"
  | "container"
  | "firewall";

export type FixActionType = "link" | "modal_action";

export interface SecurityFixAction {
  label_key: string;
  default_label: string;
  type: FixActionType;
  target_url?: string;
  action_id?: string;
  payload?: Record<string, unknown>;
}

export interface SecurityIssue {
  id: string;
  category: SecurityCategory;
  severity: SecuritySeverity;
  title: string;
  title_bn?: string;
  description: string;
  description_bn?: string;
  timestamp: string;
  resource: string;
  fix_action: SecurityFixAction;
  status: "open" | "resolved";
}

export interface SecurityOverviewStats {
  score: number; // 0-100
  grade: "A" | "B" | "C" | "D" | "F";
  critical_count: number;
  warning_count: number;
  info_count: number;
  resolved_count: number;
  login_failures_24h: number;
  waf_blocks_24h: number;
  malware_detected: number;
  admins_without_2fa: number;
  expiring_ssl_certs: number;
  outdated_images: number;
}

export interface SecurityOverviewResponse {
  stats: SecurityOverviewStats;
  issues: SecurityIssue[];
}
