export type WizardStep = 1 | 2 | 3 | 4 | 5;

export interface HostingPlanOption {
  id: string;
  name: string;
  description: string;
  cpu: number; // -1 for unlimited
  memory_mb: number; // -1 for unlimited
  disk_gb: number; // -1 for unlimited
  domains_limit: number; // -1 for unlimited
  is_default?: boolean;
}

export interface NodeOption {
  id: string;
  name: string;
  region: string;
  cpu_usage_pct: number;
  memory_usage_pct: number;
  status: "healthy" | "busy" | "offline" | "draining";
}

export interface UserWizardFormData {
  // Step 1: Account
  username: string;
  email: string;
  password: string;

  // Step 2: Plan
  plan_id: string;

  // Step 3: Node
  node_mode: "auto" | "manual";
  node_id?: string;

  // Step 4: Primary Domain
  primary_domain?: string;
  create_vhost: boolean;
}

export interface OperationStep {
  id: string;
  title: string;
  description: string;
  status: "pending" | "running" | "succeeded" | "failed";
}

export interface ContainerInfo {
  id: string;
  name: string;
  status: "running" | "created" | "starting" | "stopped";
  image: string;
  ip_address: string;
  created_at: string;
}

export interface CreateUserWizardResponse {
  user: {
    id: string;
    username: string;
    email: string;
    status: "active" | "pending";
    plan_id: string;
    node_id: string;
    primary_domain?: string;
  };
  operation_id: string;
  container: ContainerInfo;
}
