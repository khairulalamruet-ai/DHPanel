export type LaLicenseStatus = "active" | "suspended" | "revoked" | "expired";
export type LaPlanTier = "starter" | "business" | "enterprise" | "unlimited";

export interface LaCustomer {
  id: string;
  name: string;
  email: string;
  company?: string;
  created_at: string;
  licenses_count: number;
}

export interface LaLicenseItem {
  id: string;
  license_key: string;
  customer_id: string;
  customer_name: string;
  customer_email: string;
  plan_tier: LaPlanTier;
  status: LaLicenseStatus;
  max_nodes: number;
  max_tenants: number;
  activations_count: number;
  max_activations: number;
  created_at: string;
  expires_at: string;
  revoked_at?: string | null;
  revoke_reason?: string | null;
}

export interface LaActivation {
  id: string;
  license_id: string;
  license_key: string;
  installation_id: string;
  server_ip: string;
  hostname: string;
  last_heartbeat_at: string;
  status: "active" | "offline" | "deactivated";
}

export interface LaOverviewMetrics {
  total_licenses: number;
  active_licenses: number;
  revoked_licenses: number;
  total_customers: number;
  active_activations: number;
}

export interface IssueLicensePayload {
  customer_name: string;
  customer_email: string;
  company?: string;
  plan_tier: LaPlanTier;
  max_nodes: number;
  max_tenants: number;
  validity_days: number; // 0 for lifetime
  max_activations: number;
}

export interface RevokeLicensePayload {
  reason: string;
}
