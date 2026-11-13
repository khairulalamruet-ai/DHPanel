export type IpRuleAction = "block" | "allow";

export interface ClientIpRule {
  id: string;
  domain: string;
  ip_cidr: string;
  action: IpRuleAction;
  notes?: string;
  created_at: string;
}

export interface NodePortRule {
  id: string;
  port: number;
  protocol: "tcp" | "udp";
  service_name: string;
  is_system_protected: boolean;
  status: "open" | "closed";
}

export interface GlobalIpBan {
  id: string;
  ip: string;
  reason: string;
  source: "bruteforce_auto" | "admin_manual";
  service: "panel_login" | "sftp" | "mail" | "manual";
  banned_at: string;
  expires_at: string | null;
}

export interface AdminFirewallOverview {
  node_id: string;
  status: "active" | "inactive";
  nftables_version: string;
  active_ports: NodePortRule[];
  active_bans: GlobalIpBan[];
  rate_limit_enabled: boolean;
  rate_limit_rps: number;
}
