import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useOperation } from "./useOperation";
import * as dhpanelApi from "@dhpanel/api";

describe("useOperation hook", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("fetches operation data and handles polling until completion", async () => {
    let callCount = 0;
    vi.spyOn(dhpanelApi, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/api/v1/admin/operations/op_123")) {
        callCount++;
        if (callCount === 1) {
          return {
            id: "op_123",
            idempotency_key: "idemp_123",
            op_type: "Ping",
            node_id: "node_1",
            status: "pending",
            retries_count: 0,
            payload: {},
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          };
        }
        return {
          id: "op_123",
          idempotency_key: "idemp_123",
          op_type: "Ping",
          node_id: "node_1",
          status: "succeeded",
          retries_count: 0,
          payload: {},
          output: { pong: true },
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        };
      }
      return {};
    });

    const onSuccessMock = vi.fn();

    const { result } = renderHook(() =>
      useOperation("op_123", {
        pollInterval: 50,
        onSuccess: onSuccessMock,
      })
    );

    await waitFor(() => {
      expect(result.current.operation?.status).toBe("succeeded");
    });

    expect(onSuccessMock).toHaveBeenCalledTimes(1);
    expect(result.current.isPolling).toBe(false);
  });

  it("triggers retry on failed operation", async () => {
    vi.spyOn(dhpanelApi, "apiFetch").mockImplementation(async (url: string, opts?: any) => {
      if (opts?.method === "POST" && url.includes("/retry")) {
        return {
          success: true,
          operation: {
            id: "op_failed",
            idempotency_key: "idemp_failed",
            op_type: "PullImage",
            node_id: "node_1",
            status: "pending",
            retries_count: 1,
            payload: {},
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        };
      }
      if (url.includes("/api/v1/admin/operations/op_failed")) {
        return {
          id: "op_failed",
          idempotency_key: "idemp_failed",
          op_type: "PullImage",
          node_id: "node_1",
          status: "failed",
          retries_count: 0,
          payload: {},
          error: { code: "timeout", message: "timed out" },
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        };
      }
      return {};
    });

    const { result } = renderHook(() =>
      useOperation("op_failed", {
        enabled: true,
      })
    );

    await waitFor(() => {
      expect(result.current.operation?.status).toBe("failed");
    });

    await act(async () => {
      await result.current.retry();
    });

    expect(result.current.operation?.status).toBe("pending");
    expect(result.current.operation?.retries_count).toBe(1);
  });
});
