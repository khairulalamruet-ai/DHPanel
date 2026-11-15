import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ToastProvider } from "@dhpanel/ui";
import { UpdatePage } from "./UpdatePage";
import { UpdateSystemCard } from "./UpdateSystemCard";
import { PanelUpdateStatus } from "./types";

const mockStatus: PanelUpdateStatus = {
  current_version: "0.1.0",
  channel: "stable",
  last_checked_at: new Date().toISOString(),
  available_update: {
    version: "0.2.0",
    release_date: new Date().toISOString(),
    channel: "stable",
    ed25519_signature: "mock_signature",
    key_id: "dhpanel-root-2026",
    checksum_sha256: "mock_sha256",
    changelog: ["Clustering protocol v2", "Dynamic WAF reload"],
    required_migrations: ["000015_cluster_nodes_v2.sql"],
    can_rollback: true,
  },
  previous_version: "0.0.9",
  can_rollback: true,
  history: [
    {
      id: "upd-001",
      from_version: "0.0.9",
      to_version: "0.1.0",
      channel: "stable",
      status: "success",
      timestamp: new Date().toISOString(),
      details: "Initial baseline upgrade",
    },
  ],
};

const mockApiFetch = vi.fn(async (url: string, opts?: any) => {
  if (url === "/api/v1/update/status") {
    return mockStatus;
  }
  if (url === "/api/v1/update/channel") {
    return { ok: true, channel: "beta" };
  }
  if (url === "/api/v1/update/check") {
    return { checked: true, available: true };
  }
  if (url === "/api/v1/update/apply") {
    const body = opts?.body ? JSON.parse(opts.body) : {};
    if (body.simulate_failure) {
      return {
        ok: false,
        status: "rolled_back",
        rolled_back: true,
        error: "Migration failed. Auto-rollback to previous version succeeded.",
      };
    }
    return { ok: true, status: "success", current_version: "0.2.0" };
  }
  if (url === "/api/v1/update/rollback") {
    return { ok: true, current_version: "0.0.9", rolled_back: true };
  }
  return { ok: true };
});

vi.mock("@dhpanel/api", () => ({
  apiFetch: (url: string, opts?: any) => mockApiFetch(url, opts),
}));

describe("Panel Update & Rollback System (T18.10)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders current version, channel, and available update manifest", async () => {
    render(
      <ToastProvider>
        <UpdatePage />
      </ToastProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("panel-update-page")).toBeInTheDocument();
      expect(screen.getByTestId("current-version-display")).toHaveTextContent("0.1.0");
      expect(screen.getByTestId("update-available-banner")).toBeInTheDocument();
      expect(screen.getByText(/0.2.0/)).toBeInTheDocument();
      expect(screen.getByText(/Ed25519 Verified/i)).toBeInTheDocument();
    });
  });

  it("switches channel between stable and beta", async () => {
    const onRefresh = vi.fn();
    render(
      <ToastProvider>
        <UpdateSystemCard status={mockStatus} onRefresh={onRefresh} />
      </ToastProvider>
    );

    const betaBtn = screen.getByTestId("channel-beta-btn");
    fireEvent.click(betaBtn);

    await waitFor(() => {
      expect(mockApiFetch).toHaveBeenCalledWith(
        "/api/v1/update/channel",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({ channel: "beta" }),
        })
      );
      expect(onRefresh).toHaveBeenCalled();
    });
  });

  it("performs update check when clicking check updates button", async () => {
    const onRefresh = vi.fn();
    render(
      <ToastProvider>
        <UpdateSystemCard status={mockStatus} onRefresh={onRefresh} />
      </ToastProvider>
    );

    const checkBtn = screen.getByTestId("btn-check-updates");
    fireEvent.click(checkBtn);

    await waitFor(() => {
      expect(mockApiFetch).toHaveBeenCalledWith("/api/v1/update/check", { method: "POST" });
      expect(onRefresh).toHaveBeenCalled();
    });
  });

  it("executes update and handles rollback when update fails (DoD)", async () => {
    const onRefresh = vi.fn();
    render(
      <ToastProvider>
        <UpdateSystemCard status={mockStatus} onRefresh={onRefresh} />
      </ToastProvider>
    );

    // Enable simulate failure checkbox
    const failureCheckbox = screen.getByTestId("simulate-failure-checkbox");
    fireEvent.click(failureCheckbox);

    // Click Update Now
    const updateBtn = screen.getByTestId("btn-update-now");
    fireEvent.click(updateBtn);

    // Progress updates through steps
    await waitFor(
      () => {
        expect(screen.getByTestId("update-progress-container")).toBeInTheDocument();
      },
      { timeout: 5000 }
    );

    // Verify rollback completed message
    await waitFor(
      () => {
        expect(screen.getByTestId("update-step-message")).toHaveTextContent(
          /Update failed|Automatic rollback completed|স্বয়ংক্রিয় রোলব্যাক/i
        );
      },
      { timeout: 8000 }
    );
  });

  it("triggers manual rollback to previous version", async () => {
    const onRefresh = vi.fn();
    render(
      <ToastProvider>
        <UpdateSystemCard status={mockStatus} onRefresh={onRefresh} />
      </ToastProvider>
    );

    const rollbackBtn = screen.getByTestId("btn-manual-rollback");
    fireEvent.click(rollbackBtn);

    await waitFor(() => {
      expect(mockApiFetch).toHaveBeenCalledWith("/api/v1/update/rollback", { method: "POST" });
      expect(onRefresh).toHaveBeenCalled();
    });
  });
});
