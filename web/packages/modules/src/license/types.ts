export type LicenseStatus = "active" | "grace_period" | "expired" | "suspended" | "restricted";

export type LicensePlanTier = "starter" | "pro" | "enterprise" | "internal_dev";

export interface LicenseFeatures {
  reseller_enabled: boolean;
  clustering_enabled: boolean;
  advanced_backups: boolean;
  commercial_waf: boolean;
  whitelabel_enabled: boolean;
  api_access: boolean;
  max_nodes: number;
  max_tenants: number;
  max_domains: number;
}

export interface LicenseUsage {
  nodes_used: number;
  tenants_used: number;
  domains_used: number;
}

export interface LicenseState {
  license_id: string;
  license_key: string;
  installation_id: string;
  customer_name: string;
  customer_email: string;
  product_name: string;
  plan_tier: LicensePlanTier;
  status: LicenseStatus;
  issued_at: string;
  expires_at: string;
  grace_period_expires_at?: string;
  last_heartbeat_at: string;
  next_refresh_at: string;
  signing_key_id: string;
  signature_valid: boolean;
  features: LicenseFeatures;
  usage: LicenseUsage;
  offline_grace_hours_remaining?: number;
  restricted_reason?: string;
}

export interface ActivateLicensePayload {
  license_key: string;
}

export interface RecoveryRequestPayload {
  reason: string;
  contact_email: string;
  notes?: string;
}

export interface RecoveryRequestResponse {
  ticket_id: string;
  status: "pending" | "approved" | "rejected";
  created_at: string;
  message: string;
}
