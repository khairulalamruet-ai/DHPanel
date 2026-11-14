export interface ResellerPackage {
  id: string;
  name: string;
  max_accounts: number;
  max_disk_gb: number;
  max_bandwidth_gb: number;
  allow_overselling: boolean;
  allowed_plans: string[];
  created_at: string;
}

export interface ResellerBranding {
  brand_name: string;
  logo_url?: string;
  primary_color: string;
  support_email: string;
  custom_login_message?: string;
}

export interface ResellerAccount {
  id: string;
  username: string;
  email: string;
  company_name: string;
  package_id: string;
  package_name: string;
  status: "active" | "suspended";
  accounts_count: number;
  disk_used_gb: number;
  bandwidth_used_gb: number;
  branding: ResellerBranding;
  created_at: string;
}

export interface ResellerUser {
  id: string;
  reseller_id: string;
  username: string;
  email: string;
  domain: string;
  plan_name: string;
  disk_used_mb: number;
  status: "active" | "suspended";
  created_at: string;
}

export interface ResellerPlan {
  id: string;
  reseller_id: string;
  name: string;
  disk_mb: number;
  bandwidth_mb: number;
  domains_limit: number;
  databases_limit: number;
  mailboxes_limit: number;
  created_at: string;
}
