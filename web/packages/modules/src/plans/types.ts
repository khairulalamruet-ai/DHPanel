export interface PlanResources {
  cpu: number; // cores, -1 = unlimited
  memory_mb: number; // MB, -1 = unlimited
  disk_mb: number; // MB, -1 = unlimited
  bandwidth_mb: number; // MB, -1 = unlimited
  pids_limit: number; // -1 = unlimited
}

export interface PlanLimits {
  max_domains: number; // -1 = unlimited
  max_subdomains: number; // -1 = unlimited
  max_databases: number; // -1 = unlimited
  max_ftp_accounts: number; // -1 = unlimited
  max_email_accounts: number; // -1 = unlimited
}

export interface Plan {
  id: string;
  name: string;
  description: string;
  feature_set_id: string;
  feature_set_name: string;
  is_default: boolean;
  users_count: number;
  resources: PlanResources;
  limits: PlanLimits;
  created_at: string;
  updated_at: string;
}

export interface CreatePlanPayload {
  name: string;
  description?: string;
  feature_set_id: string;
  feature_set_name?: string;
  is_default?: boolean;
  resources: PlanResources;
  limits: PlanLimits;
}
