export type WafMode = "off" | "detect" | "block";
export type WafCategory = "sqli" | "xss" | "rce" | "scanner" | "protocol" | "generic";
export type WafSeverity = "CRITICAL" | "HIGH" | "MEDIUM" | "LOW";
export type WafAction = "blocked" | "detected";

export interface WafEvent {
  id: string;
  timestamp: string;
  client_ip: string;
  country: string;
  domain: string;
  http_method: string;
  uri: string;
  rule_id: number;
  rule_message: string;
  category: WafCategory;
  severity: WafSeverity;
  action: WafAction;
  matched_data?: string;
  user_agent?: string;
  tags?: string[];
  request_headers?: Record<string, string>;
  request_body?: string;
}

export interface DisabledRuleInfo {
  rule_id: number;
  domain: string;
  description: string;
  disabled_at: string;
  reason?: string;
}

export interface WafPolicy {
  domain: string;
  mode: WafMode;
  disabled_rules: number[];
  paranoia_level: number;
  updated_at: string;
}

export interface WafStats {
  total_events: number;
  blocked_events: number;
  detected_events: number;
  sqli_count: number;
  xss_count: number;
  rce_count: number;
  scanner_count: number;
}
