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

// --- Apply-to-all Operation Types (T16.06) ---

export type RollingApplyStatus = "idle" | "running" | "succeeded" | "failed" | "halted";

export interface RollingNodeProgress {
  node_id: string;
  node_name: string;
  batch_index: number;
  status: "pending" | "running" | "succeeded" | "failed" | "skipped";
  duration_ms?: number;
  error?: string | null;
  output?: string | null;
}

export interface ApplyToAllPayload {
  op_type: string;
  target_node_ids?: string[];
  batch_size: number; // e.g. 2
  delay_ms: number; // e.g. 500
  stop_on_failure: boolean; // default true
  payload?: Record<string, unknown>;
  simulate_fail_node_id?: string | null;
}

export interface ApplyToAllExecutionResult {
  id: string;
  op_type: string;
  total_nodes: number;
  completed_nodes: number;
  succeeded_nodes: number;
  failed_nodes: number;
  skipped_nodes: number;
  status: RollingApplyStatus;
  batches_count: number;
  node_results: RollingNodeProgress[];
  created_at: string;
  completed_at?: string;
  message: string;
}

