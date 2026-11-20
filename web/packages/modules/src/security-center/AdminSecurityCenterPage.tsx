import React, { useState, useEffect, useCallback } from "react";
import { apiFetch } from "@builtpanel/api";
import { useToast } from "@builtpanel/ui";
import { useTranslation } from "@builtpanel/i18n";
import { useNavigate } from "@tanstack/react-router";
import { ShieldCheck, AlertCircle } from "lucide-react";
import { SecurityOverviewResponse, SecurityIssue } from "./types";
import { SecurityScoreCard } from "./SecurityScoreCard";
import { SecurityStatsGrid } from "./SecurityStatsGrid";
import { SecurityIssuesList } from "./SecurityIssuesList";
import { FixIssueDialog } from "./FixIssueDialog";

export function AdminSecurityCenterPage() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const navigate = useNavigate();

  const [data, setData] = useState<SecurityOverviewResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isError, setIsError] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [selectedIssueForFix, setSelectedIssueForFix] = useState<SecurityIssue | null>(null);

  const fetchOverview = useCallback(async () => {
    try {
      setIsLoading(true);
      setIsError(false);
      const res = await apiFetch<SecurityOverviewResponse>("/api/v1/admin/security/overview");
      setData(res);
    } catch {
      setIsError(true);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchOverview();
  }, [fetchOverview]);

  const handleRescan = async () => {
    try {
      setIsScanning(true);
      const res = await apiFetch<SecurityOverviewResponse>("/api/v1/admin/security/scan", {
        method: "POST",
      });
      setData(res);
      toast({
        title: t("securityCenter.scanCompleteTitle", "Security Audit Completed"),
        description: t("securityCenter.scanCompleteDesc", "System posture re-evaluated across all security domains."),
      });
    } catch {
      toast({
        title: t("common.error", "Error"),
        description: t("securityCenter.scanFailed", "Failed to trigger security audit."),
        variant: "destructive",
      });
    } finally {
      setIsScanning(false);
    }
  };

  const handleExecuteFix = async (issueId: string) => {
    try {
      const res = await apiFetch<{ success: boolean; issue: SecurityIssue }>(
        `/api/v1/admin/security/issues/${issueId}/fix`,
        {
          method: "POST",
        }
      );

      toast({
        title: t("securityCenter.fixSuccessTitle", "Remediation Applied"),
        description: t("securityCenter.fixSuccessDesc", "Security issue was successfully mitigated."),
      });

      // Update state locally
      setData((old) => {
        if (!old) return old;
        return {
          ...old,
          stats: {
            ...old.stats,
            score: Math.min(100, old.stats.score + 5),
            resolved_count: old.stats.resolved_count + 1,
            critical_count:
              res.issue.severity === "critical"
                ? Math.max(0, old.stats.critical_count - 1)
                : old.stats.critical_count,
            warning_count:
              res.issue.severity === "warning"
                ? Math.max(0, old.stats.warning_count - 1)
                : old.stats.warning_count,
          },
          issues: old.issues.map((i) =>
            i.id === res.issue.id ? { ...i, status: "resolved" } : i
          ),
        };
      });
    } catch {
      toast({
        title: t("common.error", "Error"),
        description: t("securityCenter.fixFailed", "Could not apply remediation action."),
        variant: "destructive",
      });
      throw new Error("Fix failed");
    }
  };

  const handleNavigate = (path: string) => {
    navigate({ to: path });
  };

  if (isLoading) {
    return (
      <div className="flex h-64 items-center justify-center" data-testid="security-center-loading">
        <p className="text-sm text-muted-foreground">{t("common.loading", "Loading...")}</p>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="p-8 text-center" data-testid="security-center-error">
        <AlertCircle className="h-10 w-10 text-destructive mx-auto mb-2" />
        <p className="font-semibold">{t("securityCenter.loadError", "Failed to load security center data.")}</p>
        <button
          onClick={() => fetchOverview()}
          className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded text-xs font-semibold"
        >
          {t("common.retry", "Retry")}
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-6" data-testid="admin-security-center-page">
      {/* Header */}
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight flex items-center gap-2" data-testid="security-center-title">
            <ShieldCheck className="h-7 w-7 text-primary" />
            {t("securityCenter.title", "Security Center")}
          </h1>
          <p className="text-sm text-muted-foreground mt-1">
            {t(
              "securityCenter.subtitle",
              "Centralized posture analysis: failed logins, WAF blocks, malware events, 2FA enforcement, SSL expiration, and container CVEs."
            )}
          </p>
        </div>
      </div>

      {/* Security Health Score Card */}
      <SecurityScoreCard
        stats={data.stats}
        onRescan={handleRescan}
        isScanning={isScanning}
      />

      {/* 6 Core Security Domain KPIs (T11.11 requirement) */}
      <SecurityStatsGrid stats={data.stats} />

      {/* Detected Issues List with One-Click "Fix" Links (T11.11 DoD) */}
      <SecurityIssuesList
        issues={data.issues}
        onOpenFix={(issue) => setSelectedIssueForFix(issue)}
        onNavigateTo={handleNavigate}
      />

      {/* Remediate Action Dialog */}
      <FixIssueDialog
        open={Boolean(selectedIssueForFix)}
        issue={selectedIssueForFix}
        onClose={() => setSelectedIssueForFix(null)}
        onExecuteFix={handleExecuteFix}
        onNavigateTo={handleNavigate}
      />
    </div>
  );
}
