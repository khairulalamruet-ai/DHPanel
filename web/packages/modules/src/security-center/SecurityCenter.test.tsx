import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SecurityScoreCard } from "./SecurityScoreCard";
import { SecurityStatsGrid } from "./SecurityStatsGrid";
import { SecurityIssuesList } from "./SecurityIssuesList";
import { FixIssueDialog } from "./FixIssueDialog";
import { AdminSecurityCenterPage } from "./AdminSecurityCenterPage";
import { SecurityOverviewResponse } from "./types";

// Mock @dhpanel/api
const mockApiFetch = vi.fn();
vi.mock("@dhpanel/api", () => ({
  apiFetch: (...args: any[]) => mockApiFetch(...args),
}));

// Mock @dhpanel/i18n
vi.mock("@dhpanel/i18n", () => ({
  useTranslation: () => ({
    t: (key: string, defaultVal?: string | Record<string, any>) => {
      if (typeof defaultVal === "string") return defaultVal;
      return key;
    },
    i18n: { language: "en" },
  }),
}));

// Mock @dhpanel/ui toast
const mockToast = vi.fn();
vi.mock("@dhpanel/ui", async () => {
  const actual = await vi.importActual<any>("@dhpanel/ui");
  return {
    ...actual,
    useToast: () => ({ toast: mockToast }),
  };
});

// Mock @tanstack/react-router useNavigate
const mockNavigate = vi.fn();
vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => mockNavigate,
}));

describe("Security Center UI (T11.11)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const mockOverview: SecurityOverviewResponse = {
    stats: {
      score: 78,
      grade: "B",
      critical_count: 2,
      warning_count: 3,
      info_count: 1,
      resolved_count: 4,
      login_failures_24h: 14,
      waf_blocks_24h: 8,
      malware_detected: 1,
      admins_without_2fa: 2,
      expiring_ssl_certs: 1,
      outdated_images: 3,
    },
    issues: [
      {
        id: "sec-issue-auth-failures",
        category: "auth",
        severity: "critical",
        title: "Brute-Force SSH/Panel Login Attempts Detected",
        description: "14 consecutive failed login attempts detected from IP 203.0.113.88.",
        timestamp: new Date().toISOString(),
        resource: "203.0.113.88",
        fix_action: {
          label_key: "securityCenter.fixBanIp",
          default_label: "Ban IP in Firewall",
          type: "modal_action",
          action_id: "ban_ip",
        },
        status: "open",
      },
      {
        id: "sec-issue-waf-attacks",
        category: "waf",
        severity: "warning",
        title: "Coraza WAF Triggered on Domain client-store.com",
        description: "SQL Injection payloads detected on search endpoint.",
        timestamp: new Date().toISOString(),
        resource: "client-store.com",
        fix_action: {
          label_key: "securityCenter.fixReviewWaf",
          default_label: "Review WAF Events",
          type: "link",
          target_url: "/admin/waf",
        },
        status: "open",
      },
      {
        id: "sec-issue-malware-eicar",
        category: "malware",
        severity: "critical",
        title: "Suspicious Webshell / Malware Detected",
        description: "ClamAV flagged backdoor.php as PHP.Trojan.WebShell-1.",
        timestamp: new Date().toISOString(),
        resource: "/var/www/site1/public/backdoor.php",
        fix_action: {
          label_key: "securityCenter.fixQuarantineFile",
          default_label: "Quarantine Malware File",
          type: "modal_action",
          action_id: "quarantine_file",
        },
        status: "open",
      },
      {
        id: "sec-issue-admins-no-2fa",
        category: "auth",
        severity: "warning",
        title: "Admin Accounts Missing Two-Factor Authentication",
        description: "2 admins do not have 2FA enabled.",
        timestamp: new Date().toISOString(),
        resource: "admin_user_2",
        fix_action: {
          label_key: "securityCenter.fixEnforce2fa",
          default_label: "Enforce 2FA on Admins",
          type: "modal_action",
          action_id: "enforce_2fa",
        },
        status: "open",
      },
      {
        id: "sec-issue-ssl-expiring",
        category: "ssl",
        severity: "warning",
        title: "SSL/TLS Certificate Expiring Soon",
        description: "Certificate expires in 3 days.",
        timestamp: new Date().toISOString(),
        resource: "billing.example.com",
        fix_action: {
          label_key: "securityCenter.fixRenewSsl",
          default_label: "Renew SSL Certificate",
          type: "link",
          target_url: "/ssl",
        },
        status: "open",
      },
      {
        id: "sec-issue-outdated-image",
        category: "container",
        severity: "warning",
        title: "Outdated Base Docker Image with Known CVEs",
        description: "php:8.2-fpm has known vulnerabilities.",
        timestamp: new Date().toISOString(),
        resource: "docker.io/library/php:8.2-fpm",
        fix_action: {
          label_key: "securityCenter.fixPullImage",
          default_label: "Update Container Image",
          type: "modal_action",
          action_id: "update_image",
        },
        status: "open",
      },
    ],
  };

  it("SecurityScoreCard: renders system security health score, grade, and rescan button", () => {
    const handleRescan = vi.fn();
    render(
      <SecurityScoreCard
        stats={mockOverview.stats}
        onRescan={handleRescan}
        isScanning={false}
      />
    );

    expect(screen.getByTestId("security-score-card")).toBeDefined();
    expect(screen.getByTestId("security-score-value")).toHaveTextContent("78");
    expect(screen.getByTestId("stat-critical-count")).toHaveTextContent("2");
    expect(screen.getByTestId("stat-warning-count")).toHaveTextContent("3");
    expect(screen.getByTestId("stat-resolved-count")).toHaveTextContent("4");

    const rescanBtn = screen.getByTestId("btn-rescan-security");
    fireEvent.click(rescanBtn);
    expect(handleRescan).toHaveBeenCalledTimes(1);
  });

  it("SecurityStatsGrid: renders all 6 core security categories", () => {
    render(<SecurityStatsGrid stats={mockOverview.stats} />);

    expect(screen.getByTestId("kpi-login-failures")).toHaveTextContent("14");
    expect(screen.getByTestId("kpi-waf-blocks")).toHaveTextContent("8");
    expect(screen.getByTestId("kpi-malware")).toHaveTextContent("1");
    expect(screen.getByTestId("kpi-admins-without-2fa")).toHaveTextContent("2");
    expect(screen.getByTestId("kpi-expiring-ssl")).toHaveTextContent("1");
    expect(screen.getByTestId("kpi-outdated-images")).toHaveTextContent("3");
  });

  it("SecurityIssuesList: meets DoD by providing a 'Fix' link/button next to every problem", () => {
    const handleOpenFix = vi.fn();
    const handleNavigate = vi.fn();

    render(
      <SecurityIssuesList
        issues={mockOverview.issues}
        onOpenFix={handleOpenFix}
        onNavigateTo={handleNavigate}
      />
    );

    // Each issue row must have a Fix button
    for (const issue of mockOverview.issues) {
      const fixBtn = screen.getByTestId(`btn-fix-issue-${issue.id}`);
      expect(fixBtn).toBeDefined();
      expect(fixBtn.textContent).toContain("Fix:");
    }

    // Clicking a link fix should navigate
    const wafFixBtn = screen.getByTestId("btn-fix-issue-sec-issue-waf-attacks");
    fireEvent.click(wafFixBtn);
    expect(handleNavigate).toHaveBeenCalledWith("/admin/waf");

    // Clicking an action fix should open modal
    const malwareFixBtn = screen.getByTestId("btn-fix-issue-sec-issue-malware-eicar");
    fireEvent.click(malwareFixBtn);
    expect(handleOpenFix).toHaveBeenCalledWith(
      expect.objectContaining({ id: "sec-issue-malware-eicar" })
    );
  });

  it("FixIssueDialog: allows executing remediation action", async () => {
    const handleExecuteFix = vi.fn().mockResolvedValue(undefined);
    const handleClose = vi.fn();
    const handleNavigate = vi.fn();

    const targetIssue = mockOverview.issues[0]; // Ban IP in Firewall

    render(
      <FixIssueDialog
        open={true}
        issue={targetIssue}
        onClose={handleClose}
        onExecuteFix={handleExecuteFix}
        onNavigateTo={handleNavigate}
      />
    );

    expect(screen.getByTestId("fix-dialog-title")).toBeDefined();
    expect(screen.getByText("203.0.113.88")).toBeDefined();

    const confirmBtn = screen.getByTestId("btn-confirm-fix");
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(handleExecuteFix).toHaveBeenCalledWith(targetIssue.id);
    });
  });

  it("AdminSecurityCenterPage: loads and renders full security posture dashboard", async () => {
    mockApiFetch.mockResolvedValueOnce(mockOverview);

    render(<AdminSecurityCenterPage />);

    await waitFor(() => {
      expect(screen.getByTestId("admin-security-center-page")).toBeDefined();
    });

    expect(screen.getByTestId("security-center-title")).toBeDefined();
    expect(screen.getByTestId("security-score-card")).toBeDefined();
    expect(screen.getByTestId("security-stats-grid")).toBeDefined();
    expect(screen.getByTestId("security-issues-card")).toBeDefined();

    // Verify all 6 issues are present
    expect(screen.getByTestId("security-issue-row-sec-issue-auth-failures")).toBeDefined();
    expect(screen.getByTestId("security-issue-row-sec-issue-waf-attacks")).toBeDefined();
    expect(screen.getByTestId("security-issue-row-sec-issue-malware-eicar")).toBeDefined();
    expect(screen.getByTestId("security-issue-row-sec-issue-admins-no-2fa")).toBeDefined();
    expect(screen.getByTestId("security-issue-row-sec-issue-ssl-expiring")).toBeDefined();
    expect(screen.getByTestId("security-issue-row-sec-issue-outdated-image")).toBeDefined();
  });
});
