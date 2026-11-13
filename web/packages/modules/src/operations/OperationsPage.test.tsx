import { describe, it, expect, vi, beforeEach } from "vitest";
import React from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { OperationsPage } from "./OperationsPage";
import { ToastProvider } from "@dhpanel/ui";
import * as dhpanelApi from "@dhpanel/api";

let mockHasPermission = true;

vi.mock("@dhpanel/auth", () => ({
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

vi.mock("@dhpanel/i18n", () => ({
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
    payload: { echo: "dhpanel-heartbeat" },
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

    vi.spyOn(dhpanelApi, "apiFetch").mockImplementation(async (url: string, opts?: any) => {
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
});
