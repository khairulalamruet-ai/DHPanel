import React from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { ToastProvider } from "@builtpanel/ui";
import { AdminSystemLogsPage } from "./AdminSystemLogsPage";

function renderWithProviders(ui: React.ReactElement) {
  return render(<ToastProvider>{ui}</ToastProvider>);
}

describe("AdminSystemLogsPage (T10.07)", () => {
  it("renders page header, service tabs, node selector, and virtualized system log entries", async () => {
    renderWithProviders(<AdminSystemLogsPage />);

    await waitFor(() => {
      expect(screen.getByTestId("admin-system-logs-title")).toBeInTheDocument();
    });

    expect(screen.getByTestId("tab-service-control-plane")).toBeInTheDocument();
    expect(screen.getByTestId("tab-service-agent")).toBeInTheDocument();
    expect(screen.getByTestId("tab-service-caddy")).toBeInTheDocument();
    expect(screen.getByTestId("tab-service-auth")).toBeInTheDocument();
    expect(screen.getByTestId("select-admin-log-node")).toBeInTheDocument();
    expect(screen.getByTestId("select-admin-log-level")).toBeInTheDocument();
    expect(screen.getByTestId("btn-download-admin-log")).toBeInTheDocument();
    expect(screen.getByTestId("virtualized-log-container")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByTestId("virtualized-log-line-0")).toBeInTheDocument();
      expect(screen.getByTestId("metric-admin-lines")).toBeInTheDocument();
    });
  });

  it("switches across services: Node Agent, Caddy, Auth, and Control Plane", async () => {
    renderWithProviders(<AdminSystemLogsPage />);

    await waitFor(() => {
      expect(screen.getByTestId("tab-service-agent")).toBeInTheDocument();
    });

    // Switch to Node Agent
    fireEvent.click(screen.getByTestId("tab-service-agent"));

    await waitFor(() => {
      expect(screen.getByTestId("badge-service-active")).toHaveTextContent("agent");
      expect(screen.getByTestId("virtualized-log-container")).toBeInTheDocument();
    });

    // Switch to Caddy
    fireEvent.click(screen.getByTestId("tab-service-caddy"));

    await waitFor(() => {
      expect(screen.getByTestId("badge-service-active")).toHaveTextContent("caddy");
    });

    // Switch to Auth
    fireEvent.click(screen.getByTestId("tab-service-auth"));

    await waitFor(() => {
      expect(screen.getByTestId("badge-service-active")).toHaveTextContent("auth");
    });
  });

  it("filters system logs by log level", async () => {
    renderWithProviders(<AdminSystemLogsPage />);

    await waitFor(() => {
      expect(screen.getByTestId("select-admin-log-level")).toBeInTheDocument();
    });

    fireEvent.change(screen.getByTestId("select-admin-log-level"), {
      target: { value: "error" },
    });

    await waitFor(() => {
      expect(screen.getByTestId("virtualized-log-container")).toBeInTheDocument();
    });
  });

  it("toggles Live Tail mode on admin system stream", async () => {
    renderWithProviders(<AdminSystemLogsPage />);

    await waitFor(() => {
      expect(screen.getByTestId("btn-toggle-admin-live-mode")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("btn-toggle-admin-live-mode"));

    await waitFor(() => {
      expect(screen.getByTestId("badge-admin-live-status")).toHaveTextContent(
        /System Stream Active|সিস্টেম স্ট্রিম সক্রিয়/i
      );
    });

    // Toggle off
    fireEvent.click(screen.getByTestId("btn-toggle-admin-live-mode"));

    await waitFor(() => {
      expect(screen.getByTestId("badge-admin-live-status")).toHaveTextContent(
        /Stream Paused|স্ট্রিম স্থগিত/i
      );
    });
  });
});
