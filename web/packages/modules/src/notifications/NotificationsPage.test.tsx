import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ToastProvider } from "@dhpanel/ui";
import { NotificationItem, NotificationChannelConfig } from "./types";
import { NotificationBell } from "./NotificationBell";
import { NotificationPreferencesPage } from "./NotificationPreferencesPage";

const mockNotifications: NotificationItem[] = [
  {
    id: "notif-1",
    title: "SSL Certificate Expiring Soon",
    message: "Certificate for mysite.com will expire in 7 days.",
    severity: "warning",
    category: "ssl",
    read: false,
    created_at: new Date(Date.now() - 300000).toISOString(),
    action_url: "/ssl",
    action_label: "Renew SSL",
  },
  {
    id: "notif-2",
    title: "Nightly S3 Backup Succeeded",
    message: "420 MB archive transferred to wasabi-us-east-1.",
    severity: "success",
    category: "backup",
    read: false,
    created_at: new Date(Date.now() - 3600000).toISOString(),
  },
  {
    id: "notif-3",
    title: "Cluster Node node-lon-02 Offline",
    message: "Agent connection dropped 4 minutes ago.",
    severity: "error",
    category: "system",
    read: true,
    created_at: new Date(Date.now() - 7200000).toISOString(),
  },
];

const mockConfig: NotificationChannelConfig = {
  email_enabled: true,
  email_recipients: ["admin@enterprise.io"],
  webhook_enabled: false,
  webhook_url: "",
  in_app_enabled: true,
  notify_on_ssl_expiry: true,
  notify_on_backup_failure: true,
  notify_on_node_offline: true,
  notify_on_security_alert: true,
  snooze_until: null,
};

vi.mock("@dhpanel/api", () => ({
  apiFetch: vi.fn(async (url: string, opts?: any) => {
    if (url === "/api/v1/notifications") {
      return mockNotifications;
    }
    if (url === "/api/v1/notifications/settings") {
      if (opts?.method === "PUT") {
        return JSON.parse(opts.body);
      }
      return mockConfig;
    }
    if (url === "/api/v1/notifications/read-all") {
      return { success: true };
    }
    if (url.endsWith("/read")) {
      return { success: true };
    }
    if (url === "/api/v1/notifications/snooze") {
      const body = JSON.parse(opts.body);
      return {
        snooze_until: body.duration_hours > 0 ? new Date(Date.now() + body.duration_hours * 3600000).toISOString() : null,
      };
    }
    if (url === "/api/v1/notifications/test") {
      return { success: true };
    }
    return { success: true };
  }),
}));

describe("Notification UI Module (T18.05)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("NotificationBell", () => {
    it("renders trigger button with unread count badge", async () => {
      render(<NotificationBell />);

      await waitFor(() => {
        expect(screen.getByTestId("notification-bell-btn")).toBeDefined();
        // 2 unread out of 3
        expect(screen.getByTestId("unread-count-badge").textContent).toBe("2");
      });
    });

    it("opens flyout dropdown and lists notifications", async () => {
      render(<NotificationBell />);

      await waitFor(() => {
        expect(screen.getByTestId("notification-bell-btn")).toBeDefined();
      });

      fireEvent.click(screen.getByTestId("notification-bell-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("notification-dropdown")).toBeDefined();
        expect(screen.getByText("SSL Certificate Expiring Soon")).toBeDefined();
        expect(screen.getByText("Nightly S3 Backup Succeeded")).toBeDefined();
      });
    });

    it("filters notifications by unread and critical tabs", async () => {
      render(<NotificationBell />);

      fireEvent.click(screen.getByTestId("notification-bell-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("notification-dropdown")).toBeDefined();
      });

      // Filter Unread
      fireEvent.click(screen.getByTestId("tab-notif-unread"));
      expect(screen.getByText("SSL Certificate Expiring Soon")).toBeDefined();
      expect(screen.queryByText("Cluster Node node-lon-02 Offline")).toBeNull();

      // Filter Critical
      fireEvent.click(screen.getByTestId("tab-notif-critical"));
      expect(screen.getByText("SSL Certificate Expiring Soon")).toBeDefined();
      expect(screen.getByText("Cluster Node node-lon-02 Offline")).toBeDefined();
      expect(screen.queryByText("Nightly S3 Backup Succeeded")).toBeNull();
    });

    it("marks all notifications as read", async () => {
      render(<NotificationBell />);

      fireEvent.click(screen.getByTestId("notification-bell-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("btn-mark-all-read")).toBeDefined();
      });

      fireEvent.click(screen.getByTestId("btn-mark-all-read"));

      await waitFor(() => {
        expect(screen.queryByTestId("unread-count-badge")).toBeNull();
      });
    });
  });

  describe("NotificationPreferencesPage", () => {
    it("loads and renders channels, alert triggers, and snooze controls", async () => {
      render(
        <ToastProvider>
          <NotificationPreferencesPage />
        </ToastProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId("notification-settings-title")).toBeDefined();
        expect(screen.getByTestId("notification-channels-card")).toBeDefined();
        expect(screen.getByTestId("notification-triggers-card")).toBeDefined();
        expect(screen.getByTestId("snooze-control-card")).toBeDefined();
      });
    });

    it("allows toggling snooze window", async () => {
      render(
        <ToastProvider>
          <NotificationPreferencesPage />
        </ToastProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId("btn-snooze-1h")).toBeDefined();
      });

      fireEvent.click(screen.getByTestId("btn-snooze-1h"));

      await waitFor(() => {
        expect(screen.getByTestId("btn-clear-snooze")).toBeDefined();
      });

      fireEvent.click(screen.getByTestId("btn-clear-snooze"));

      await waitFor(() => {
        expect(screen.getByTestId("btn-snooze-1h")).toBeDefined();
      });
    });

    it("triggers test notification dispatch", async () => {
      render(
        <ToastProvider>
          <NotificationPreferencesPage />
        </ToastProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId("btn-test-notification")).toBeDefined();
      });

      fireEvent.click(screen.getByTestId("btn-test-notification"));

      await waitFor(() => {
        expect(screen.getByTestId("btn-test-notification")).toBeDefined();
      });
    });
  });
});
