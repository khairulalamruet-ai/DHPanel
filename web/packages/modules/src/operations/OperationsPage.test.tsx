import { describe, it, expect, vi, beforeEach } from "vitest";
import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { OperationsPage } from "./OperationsPage";
import { ToastProvider } from "@builtpanel/ui";
import * as builtpanelApi from "@builtpanel/api";

let mockHasPermission = true;

vi.mock("@builtpanel/auth", () => ({
  useSession: () => ({
    data: {
      user: { id: "00000000-0000-0000-0000-000000000001", username: "admin" },
      roles: ["super_admin"],
      permissions: ["*"],
    },
    isLoading: false,
    hasPermission: (_perm: string) => mockHasPermission,
    hasRole: (_role: string) => true,
  }),
}));

vi.mock("@builtpanel/i18n", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const dict: Record<string, string> = {
        "operations.title": "Operation Ledger",
        "operations.desc": "Track real-time background jobs and retry failed operations.",
        "operations.status.all": "All Statuses",
        "operations.status.pending": "Pending",
        "operations.status.dispatched": "Dispatched",
        "operations.status.succeeded": "Succeeded",
        "operations.status.failed": "Failed",
        "operations.status.retryable": "Retryable",
        "operations.filters.title": "Filter Operations",
        "operations.filters.status": "Status",
        "operations.filters.opType": "Operation Type",
        "operations.filters.opTypePlaceholder": "e.g. Ping, PullImage",
        "operations.filters.node": "Node",
        "operations.filters.nodePlaceholder": "Node ID or name",
        "operations.filters.apply": "Apply Filters",
        "operations.filters.reset": "Reset Filters",
        "operations.table.id": "Operation ID",
        "operations.table.type": "Type",
        "operations.table.status": "Status",
        "operations.table.node": "Node",
        "operations.table.retries": "Retries",
        "operations.table.time": "Created",
        "operations.table.actions": "Actions",
        "operations.table.viewDetails": "View Payload & Logs",
        "operations.table.retry": "Retry",
        "operations.table.retrying": "Retrying...",
        "operations.modal.title": "Operation Details",
        "operations.modal.idempotencyKey": "Idempotency Key",
        "operations.modal.nodeId": "Node ID",
        "operations.modal.attempts": "Attempts Count",
        "operations.modal.payload": "Operation Payload",
        "operations.modal.output": "Execution Output",
        "operations.modal.error": "Error Diagnostic",
        "operations.modal.copyJson": "Copy JSON",
        "operations.modal.copied": "Copied!",
        "operations.modal.close": "Close",
        "operations.modal.retryOp": "Retry Operation",
        "operations.retrySuccess": "Operation re-queued for execution successfully",
        "operations.applyAll.openBtn": "Apply to All",
        "operations.applyAll.dialogTitle": "Rolling Cluster Orchestrator",
        "operations.applyAll.dialogDesc": "Execute operation progressively across all cluster nodes",
        "operations.applyAll.opTypeLabel": "Operation Type",
        "operations.applyAll.targetScopeLabel": "Target Scope",
        "operations.applyAll.allNodesScope": "All Active Cluster Nodes (10 nodes)",
        "operations.applyAll.policySection": "Rolling Execution Policy",
        "operations.applyAll.batchSizeLabel": "Batch Size (Concurrency)",
        "operations.applyAll.batch1": "1 node / batch (Serial)",
        "operations.applyAll.batch2": "2 nodes / batch (Recommended)",
        "operations.applyAll.batch5": "5 nodes / batch (Fast)",
        "operations.applyAll.delayLabel": "Inter-Batch Delay",
        "operations.applyAll.stopOnFailureLabel": "Stop on Failure (Circuit Breaker)",
        "operations.applyAll.stopOnFailureDesc": "Immediately abort remaining batches if any node fails execution.",
        "operations.applyAll.simulateFailLabel": "Fault Injection (Circuit Breaker Test)",
        "operations.applyAll.noSimulatedFail": "Normal Execution (All nodes healthy)",
        "operations.applyAll.startBtn": "Start Rolling Apply",
        "operations.applyAll.executing": "Executing Rolling Batches...",
        "operations.applyAll.reconfigureBtn": "New Execution",
        "operations.applyAll.completedTitle": "Rolling Apply Completed",
        "operations.applyAll.haltedTitle": "Execution Halted by Circuit Breaker",
        "operations.applyAll.totalNodes": "Total Nodes",
        "operations.applyAll.succeededNodes": "Succeeded",
        "operations.applyAll.failedNodes": "Failed",
        "operations.applyAll.skippedNodes": "Skipped",
        "operations.applyAll.op.syncSettings": "Sync Cluster Settings",
        "operations.applyAll.op.updateVhost": "Update VirtualHost Templates",
        "operations.applyAll.op.restartWeb": "Graceful Restart Web Services",
        "operations.applyAll.op.applySecurity": "Deploy Security Firewall Rules",
        "operations.applyAll.status.succeeded": "Succeeded",
        "operations.applyAll.status.failed": "Failed",
        "operations.applyAll.status.skipped": "Skipped",
        "operations.applyAll.status.running": "Running",
        "operations.applyAll.status.pending": "Pending",
        "operations.applyAll.table.batch": "Batch",
        "operations.applyAll.table.node": "Node",
        "operations.applyAll.table.status": "Status",
        "operations.applyAll.table.latency": "Latency",
        "operations.applyAll.table.output": "Output / Diagnostic",
        "common.cancel": "Cancel",
        "common.close": "Close",
        "common.loading": "Loading...",
        "common.reload": "Reload",
        "common.error": "An error occurred",
        "common.noPermission": "Permission denied",
      };
      return dict[key] || key;
    },
  }),
  useFormatters: () => ({
    formatDate: (d: Date) => d.toISOString(),
  }),
}));

const mockOperationsList = [
  {
    id: "00000000-0000-0000-0000-000000000101",
    idempotency_key: "idemp_ping_001",
    op_type: "Ping",
    node_id: "00000000-0000-0000-0000-0000000000aa",
    node_name: "node-master-01",
    tenant_id: null,
    status: "succeeded" as const,
    retries_count: 0,
    payload: { echo: "builtpanel-heartbeat" },
    output: { pong: true, latency_ms: 1.4 },
    error: null,
    created_at: "2026-11-12T00:00:00Z",
    updated_at: "2026-11-12T00:00:01Z",
  },
  {
    id: "00000000-0000-0000-0000-000000000103",
    idempotency_key: "idemp_pull_003",
    op_type: "PullImage",
    node_id: "00000000-0000-0000-0000-0000000000bb",
    node_name: "node-worker-02",
    tenant_id: null,
    status: "failed" as const,
    retries_count: 2,
    payload: { image: "redis:7.4-alpine" },
    output: null,
    error: {
      code: "image_pull_timeout",
      message: "Network timeout while pulling layer 3",
      details: { registry: "docker.io" },
    },
    created_at: "2026-11-12T00:05:00Z",
    updated_at: "2026-11-12T00:06:00Z",
  },
];

describe("OperationsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    mockHasPermission = true;

    vi.spyOn(builtpanelApi, "apiFetch").mockImplementation(async (url: string, opts?: any) => {
      if (opts?.method === "POST" && url.includes("/retry")) {
        return {
          success: true,
          operation: {
            ...mockOperationsList[1],
            status: "pending",
            retries_count: 3,
            error: null,
          },
        };
      }
      if (opts?.method === "POST" && url.includes("/cluster/apply-all")) {
        const body = JSON.parse(opts.body || "{}");
        const isSimulation = !!body.simulate_fail_node_id;
        if (isSimulation) {
          return {
            id: "roll_sim_fail",
            op_type: body.op_type || "SyncSettings",
            total_nodes: 10,
            completed_nodes: 4,
            succeeded_nodes: 3,
            failed_nodes: 1,
            skipped_nodes: 6,
            status: "halted",
            batches_count: 5,
            message: "Rolling execution halted after node failure in batch. Circuit breaker tripped.",
            node_results: [
              { node_id: "node-01", node_name: "node-master-01", batch_index: 0, status: "succeeded", duration_ms: 45 },
              { node_id: "node-02", node_name: "node-worker-02", batch_index: 0, status: "succeeded", duration_ms: 50 },
              { node_id: "node-03", node_name: "node-worker-03", batch_index: 1, status: "succeeded", duration_ms: 55 },
              { node_id: "node-04", node_name: "node-worker-04", batch_index: 1, status: "failed", duration_ms: 120, error: "Connection timeout" },
              { node_id: "node-05", node_name: "node-worker-05", batch_index: 2, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
              { node_id: "node-06", node_name: "node-worker-06", batch_index: 2, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
              { node_id: "node-07", node_name: "node-worker-07", batch_index: 3, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
              { node_id: "node-08", node_name: "node-worker-08", batch_index: 3, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
              { node_id: "node-09", node_name: "node-worker-09", batch_index: 4, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
              { node_id: "node-10", node_name: "node-worker-10", batch_index: 4, status: "skipped", duration_ms: 0, output: "Skipped due to circuit breaker trip" },
            ],
          };
        }
        return {
          id: "roll_success",
          op_type: body.op_type || "SyncSettings",
          total_nodes: 10,
          completed_nodes: 10,
          succeeded_nodes: 10,
          failed_nodes: 0,
          skipped_nodes: 0,
          status: "succeeded",
          batches_count: 5,
          message: "Rolling execution completed across all 10 nodes successfully.",
          node_results: Array.from({ length: 10 }).map((_, i) => ({
            node_id: `node-${i + 1}`,
            node_name: `node-worker-${String(i + 1).padStart(2, "0")}`,
            batch_index: Math.floor(i / 2),
            status: "succeeded",
            duration_ms: 40 + i * 5,
            output: "SyncSettings executed successfully",
          })),
        };
      }
      if (url.includes("/api/v1/admin/operations")) {
        return { items: mockOperationsList, total: 2 };
      }
      return {};
    });
  });

  it("renders operations table with operation types and statuses", async () => {
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );

    expect(screen.getByText("Operation Ledger")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText("Ping")).toBeInTheDocument();
      expect(screen.getByText("PullImage")).toBeInTheDocument();
      expect(screen.getAllByText("Succeeded").length).toBeGreaterThanOrEqual(1);
      expect(screen.getAllByText("Failed").length).toBeGreaterThanOrEqual(1);
    });
  });

  it("opens operation detail modal and displays payload and error", async () => {
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );

    await waitFor(() => {
      expect(screen.getByText("PullImage")).toBeInTheDocument();
    });

    const viewButtons = screen.getAllByRole("button", { name: /View Payload & Logs/i });
    fireEvent.click(viewButtons[1]); // PullImage op

    await waitFor(() => {
      expect(screen.getByText(/Operation Details/i)).toBeInTheDocument();
      expect(screen.getByText(/Network timeout while pulling layer 3/i)).toBeInTheDocument();
      expect(screen.getByText(/idemp_pull_003/i)).toBeInTheDocument();
    });
  });

  it("allows retrying a failed operation directly from UI", async () => {
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );

    await waitFor(() => {
      expect(screen.getByText("PullImage")).toBeInTheDocument();
    });

    // Failed operation has a Retry button
    const retryBtn = screen.getByRole("button", { name: /^Retry$/i });
    expect(retryBtn).toBeEnabled();

    fireEvent.click(retryBtn);

    await waitFor(() => {
      expect(screen.getByText("Operation re-queued for execution successfully")).toBeInTheDocument();
    });
  });

  it("renders permission denied when user lacks admin.operations.read", () => {
    mockHasPermission = false;
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );
    expect(screen.getByText("Permission denied")).toBeInTheDocument();
  });

  it("opens Apply-to-All dialog and executes rolling apply across 10 nodes (DoD T16.06)", async () => {
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("btn-open-apply-all")).toBeInTheDocument();
    });

    // Open Apply to All Dialog
    fireEvent.click(screen.getByTestId("btn-open-apply-all"));

    await waitFor(() => {
      expect(screen.getByText("Rolling Cluster Orchestrator")).toBeInTheDocument();
      expect(screen.getByTestId("btn-start-rolling-apply")).toBeInTheDocument();
    });

    // Start rolling execution
    fireEvent.click(screen.getByTestId("btn-start-rolling-apply"));

    // Verify 10 nodes executed successfully (DoD T16.06)
    await waitFor(() => {
      expect(screen.getByTestId("rolling-status-title")).toHaveTextContent("Rolling Apply Completed");
      expect(screen.getByTestId("metric-total-nodes")).toHaveTextContent("10");
      expect(screen.getByTestId("metric-succeeded-nodes")).toHaveTextContent("10");
      expect(screen.getByTestId("rolling-result-badge")).toHaveTextContent("succeeded");
    });
  });

  it("stops rolling apply and skips remaining nodes when a node fails with circuit breaker (DoD T16.06)", async () => {
    render(
      <ToastProvider>
        <OperationsPage />
      </ToastProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("btn-open-apply-all")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("btn-open-apply-all"));

    await waitFor(() => {
      expect(screen.getByTestId("select-simulate-fail")).toBeInTheDocument();
    });

    // Select fault injection on node-04
    fireEvent.change(screen.getByTestId("select-simulate-fail"), {
      target: { value: "node-04" },
    });

    // Start execution
    fireEvent.click(screen.getByTestId("btn-start-rolling-apply"));

    // Verify circuit breaker tripped: execution halted, remaining nodes skipped
    await waitFor(() => {
      expect(screen.getByTestId("rolling-status-title")).toHaveTextContent("Execution Halted by Circuit Breaker");
      expect(screen.getByTestId("metric-succeeded-nodes")).toHaveTextContent("3");
      expect(screen.getByTestId("metric-skipped-nodes")).toHaveTextContent("6");
      expect(screen.getByTestId("rolling-result-badge")).toHaveTextContent("halted");
    });
  });
});
