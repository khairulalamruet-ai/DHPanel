export type OperationStatus = "pending" | "dispatched" | "succeeded" | "failed" | "retryable";

export interface AgentError {
  code: string;
  message: string;
  details?: Record<string, unknown> | null;
}

export interface OperationItem {
  id: string;
  idempotency_key: string;
  op_type: string; // e.g. "Ping", "ReportCapabilities", "AttachDomain", "CreateTenant", "ReloadWebServer"
  node_id: string;
  node_name?: string;
  tenant_id?: string | null;
  status: OperationStatus;
  retries_count: number;
  payload: Record<string, unknown>;
  output?: Record<string, unknown> | null;
  error?: AgentError | null;
  created_at: string;
  updated_at: string;
}

export interface OperationListFilter {
  status?: string;
  op_type?: string;
  node_id?: string;
}
