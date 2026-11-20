import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ClientWafPage } from "./ClientWafPage";
import { AdminWafPage } from "./AdminWafPage";
import { DisableRuleDialog } from "./DisableRuleDialog";
import { WafEventDetailDialog } from "./WafEventDetailDialog";
import { DisabledRulesList } from "./DisabledRulesList";
import { WafPolicyCard } from "./WafPolicyCard";
import { WafEvent } from "./types";

// Mock @builtpanel/api
const mockApiFetch = vi.fn();
vi.mock("@builtpanel/api", () => ({
  apiFetch: (...args: any[]) => mockApiFetch(...args),
}));

// Mock @builtpanel/i18n
vi.mock("@builtpanel/i18n", () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, any>) => {
      if (params) {
        let str = key;
        for (const [k, v] of Object.entries(params)) {
          str += ` ${k}=${v}`;
        }
        return str;
      }
      return key;
    },
  }),
}));

// Mock @builtpanel/ui toast
const mockToast = vi.fn();
vi.mock("@builtpanel/ui", async () => {
  const actual = await vi.importActual<any>("@builtpanel/ui");
  return {
    ...actual,
    useToast: () => ({ toast: mockToast }),
  };
});

describe("WAF Event Log & Policy UI (T11.04)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const mockEvents: WafEvent[] = [
    {
      id: "waf-ev-01",
      timestamp: "2026-11-13 11:20:14",
      client_ip: "198.51.100.42",
      country: "US",
      domain: "example.com",
      http_method: "POST",
      uri: "/wp-login.php",
      rule_id: 942100,
      rule_message: "SQL Injection Attack: SQL Operator Detected",
      category: "sqli",
      severity: "CRITICAL",
      action: "blocked",
      matched_data: "ARGS:log -> admin' OR '1'='1",
      user_agent: "Mozilla/5.0",
      tags: ["OWASP_CRS/WEB_ATTACK/SQL_INJECTION"],
    },
  ];

  const mockDisabledRules = [
    {
      rule_id: 920350,
      domain: "example.com",
      description: "Host header is a numeric IP address",
      disabled_at: "2026-11-12 09:00:00",
      reason: "Internal microservice webhook traffic using IP addresses",
    },
  ];

  const mockPolicy = {
    domain: "example.com",
    mode: "block" as const,
    disabled_rules: [920350],
    paranoia_level: 1,
    updated_at: "2026-11-12 09:00:00",
  };

  const mockStats = {
    total_events: 1,
    blocked_events: 1,
    detected_events: 0,
    sqli_count: 1,
    xss_count: 0,
    rce_count: 0,
    scanner_count: 0,
  };

  it("ClientWafPage: renders events, stats, policy, and verifies DoD: rule can be disabled from UI", async () => {
    mockApiFetch.mockImplementation((url: string) => {
      if (url.includes("/api/v1/client/waf/events")) return Promise.resolve(mockEvents);
      if (url.includes("/api/v1/client/waf/rules/disabled")) return Promise.resolve(mockDisabledRules);
      if (url.includes("/api/v1/client/waf/policy")) return Promise.resolve(mockPolicy);
      if (url.includes("/api/v1/client/waf/stats")) return Promise.resolve(mockStats);
      return Promise.resolve({});
    });

    render(<ClientWafPage />);

    await waitFor(() => {
      expect(screen.getByTestId("waf-page-title")).toBeInTheDocument();
      expect(screen.getByTestId("card-waf-blocked")).toHaveTextContent("1");
      expect(screen.getByTestId("title-waf-events")).toBeInTheDocument();
      expect(screen.getByTestId("waf-event-row-waf-ev-01")).toBeInTheDocument();
      expect(screen.getByText("942100")).toBeInTheDocument();
      expect(screen.getByText("SQL Injection Attack: SQL Operator Detected")).toBeInTheDocument();
    });

    // DoD: Click "Disable Rule" button on the blocked event row
    const disableBtn = screen.getByTestId("btn-disable-rule-waf-ev-01");
    expect(disableBtn).toBeInTheDocument();
    fireEvent.click(disableBtn);

    // Modal opens
    await waitFor(() => {
      expect(screen.getByTestId("disable-rule-dialog-title")).toBeInTheDocument();
      expect(screen.getByTestId("badge-modal-rule-id")).toHaveTextContent("942100");
    });

    // Enter reason and confirm
    const reasonInput = screen.getByTestId("input-disable-reason");
    fireEvent.change(reasonInput, { target: { value: "False positive on custom login page" } });

    mockApiFetch.mockResolvedValueOnce({ success: true, rule_id: 942100, domain: "example.com" });
    const confirmBtn = screen.getByTestId("btn-confirm-disable-rule");
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mockApiFetch).toHaveBeenCalledWith("/api/v1/client/waf/rules/disable", expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          rule_id: 942100,
          domain: "example.com",
          reason: "False positive on custom login page",
        }),
      }));
    });
  });

  it("DisabledRulesList: displays disabled rules and re-enables rule", async () => {
    const onEnable = vi.fn().mockResolvedValue(undefined);
    render(
      <DisabledRulesList
        rules={mockDisabledRules}
        domain="example.com"
        onEnableRule={onEnable}
      />
    );

    expect(screen.getByTestId("title-disabled-rules")).toBeInTheDocument();
    expect(screen.getByTestId("disabled-rule-row-920350")).toBeInTheDocument();

    const reEnableBtn = screen.getByTestId("btn-enable-rule-920350");
    fireEvent.click(reEnableBtn);

    await waitFor(() => {
      expect(onEnable).toHaveBeenCalledWith(920350, "example.com");
    });
  });

  it("WafPolicyCard: allows switching protection mode (off/detect/block)", async () => {
    const onUpdateMode = vi.fn().mockResolvedValue(undefined);
    render(
      <WafPolicyCard
        policy={mockPolicy}
        onUpdateMode={onUpdateMode}
      />
    );

    expect(screen.getByTestId("title-waf-policy")).toBeInTheDocument();
    const detectBtn = screen.getByTestId("btn-waf-mode-detect");
    fireEvent.click(detectBtn);

    await waitFor(() => {
      expect(onUpdateMode).toHaveBeenCalledWith("detect");
    });
  });

  it("WafEventDetailDialog: shows matched data and provides disable rule trigger", () => {
    const onDisable = vi.fn();
    render(
      <WafEventDetailDialog
        open={true}
        onOpenChange={vi.fn()}
        event={mockEvents[0]}
        onDisableRule={onDisable}
      />
    );

    expect(screen.getByTestId("waf-detail-title")).toBeInTheDocument();
    expect(screen.getByTestId("detail-rule-id")).toHaveTextContent("942100");
    expect(screen.getByText("ARGS:log -> admin' OR '1'='1")).toBeInTheDocument();

    const disableBtn = screen.getByTestId("btn-detail-disable-rule");
    fireEvent.click(disableBtn);
    expect(onDisable).toHaveBeenCalledWith(942100, "example.com", "SQL Injection Attack: SQL Operator Detected");
  });

  it("AdminWafPage: renders admin cluster WAF with tenant selectors and stats", async () => {
    mockApiFetch.mockImplementation((url: string) => {
      if (url.includes("/api/v1/admin/waf/events")) return Promise.resolve(mockEvents);
      if (url.includes("/api/v1/admin/waf/rules/disabled")) return Promise.resolve(mockDisabledRules);
      if (url.includes("/api/v1/admin/waf/policy")) return Promise.resolve(mockPolicy);
      if (url.includes("/api/v1/admin/waf/stats")) return Promise.resolve(mockStats);
      return Promise.resolve({});
    });

    render(<AdminWafPage />);

    await waitFor(() => {
      expect(screen.getByTestId("admin-waf-page-title")).toBeInTheDocument();
      expect(screen.getByTestId("select-admin-waf-tenant")).toBeInTheDocument();
      expect(screen.getByTestId("select-admin-waf-domain")).toBeInTheDocument();
      expect(screen.getByTestId("card-admin-waf-blocked")).toHaveTextContent("1");
    });
  });
});
