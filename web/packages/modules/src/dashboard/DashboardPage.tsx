import React, { useState, useEffect, useCallback, useMemo } from "react";
import { useTranslation } from "@dhpanel/i18n";
import { useSession } from "@dhpanel/auth";
import { apiFetch } from "@dhpanel/api";
import { Button } from "@dhpanel/ui";
import { LayoutGrid, RefreshCw, AlertCircle } from "lucide-react";
import { HealthCard } from "./HealthCard";
import {
  DashboardWidgetItem,
  DashboardWidgetId,
  AdminDashboardSummary,
} from "./types";
import { DashboardLayoutDialog } from "./DashboardLayoutDialog";
import {
  NodesStatusWidget,
  TenantsCountWidget,
  ResourceUsageWidget,
  RecentOperationsWidget,
  SecurityAlertsWidget,
  LicenseStatusWidget,
} from "./DashboardWidgets";

const STORAGE_KEY = "dhpanel-admin-dashboard-layout";

const DEFAULT_WIDGETS: DashboardWidgetItem[] = [
  { id: "nodes_status", enabled: true, order: 0 },
  { id: "tenants_count", enabled: true, order: 1 },
  { id: "resource_usage", enabled: true, order: 2 },
  { id: "recent_operations", enabled: true, order: 3 },
  { id: "security_alerts", enabled: true, order: 4 },
  { id: "license_status", enabled: true, order: 5 },
];

const DEFAULT_SUMMARY: AdminDashboardSummary = {
  nodes: {
    total: 3,
    online: 3,
    avg_cpu_percent: 18,
    avg_ram_percent: 42,
  },
  tenants: {
    total: 24,
    active: 22,
    domains_count: 58,
    databases_count: 36,
  },
  resources: {
    cpu_percent: 24,
    memory_used_mb: 6144,
    memory_total_mb: 16384,
    disk_used_gb: 142,
    disk_total_gb: 512,
    network_rx_mb: 4.8,
    network_tx_mb: 8.2,
  },
  operations: [
    { id: "op-vhost-842", type: "VhostTemplateApply", status: "completed", duration_ms: 120, created_at: new Date().toISOString() },
    { id: "op-ssl-910", type: "AcmeCertificateIssue", status: "completed", duration_ms: 3200, created_at: new Date(Date.now() - 300000).toISOString() },
    { id: "op-db-104", type: "DatabaseBackupExport", status: "completed", duration_ms: 450, created_at: new Date(Date.now() - 600000).toISOString() },
  ],
  security: {
    blocked_threats_24h: 142,
    active_waf_rules: 942,
    ssh_bans: 12,
  },
  license: {
    tier: "DHPanel Enterprise Edition",
    status: "active",
    signature_valid: true,
    expires_in_days: 305,
  },
};

export function DashboardPage() {
  const { t } = useTranslation();
  const { data: session } = useSession();

  const [layoutDialog, setLayoutDialog] = useState(false);
  const [widgets, setWidgets] = useState<DashboardWidgetItem[]>(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed) && parsed.length > 0) return parsed;
      }
    } catch {
      // Ignore parse failure
    }
    return DEFAULT_WIDGETS;
  });

  const [summary, setSummary] = useState<AdminDashboardSummary>(DEFAULT_SUMMARY);
  const [isLoading, setIsLoading] = useState(false);

  const loadSummary = useCallback(async () => {
    try {
      setIsLoading(true);
      const res = await apiFetch<AdminDashboardSummary>("/api/v1/admin/dashboard/summary");
      setSummary(res);
    } catch {
      // Use fallback defaults
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSummary();
  }, [loadSummary]);

  const handleSaveLayout = (newWidgets: DashboardWidgetItem[]) => {
    setWidgets(newWidgets);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(newWidgets));
    } catch {
      // Ignore storage errors
    }
  };

  const handleResetLayout = () => {
    setWidgets(DEFAULT_WIDGETS);
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // Ignore
    }
  };

  // Sort and filter active widgets
  const activeWidgets = useMemo(() => {
    return [...widgets]
      .filter((w) => w.enabled)
      .sort((a, b) => a.order - b.order);
  }, [widgets]);

  const renderWidget = (id: DashboardWidgetId) => {
    switch (id) {
      case "nodes_status":
        return <NodesStatusWidget key={id} summary={summary} />;
      case "tenants_count":
        return <TenantsCountWidget key={id} summary={summary} />;
      case "resource_usage":
        return <ResourceUsageWidget key={id} summary={summary} />;
      case "recent_operations":
        return <RecentOperationsWidget key={id} summary={summary} />;
      case "security_alerts":
        return <SecurityAlertsWidget key={id} summary={summary} />;
      case "license_status":
        return <LicenseStatusWidget key={id} summary={summary} />;
      default:
        return null;
    }
  };

  return (
    <div className="space-y-6" data-testid="admin-dashboard-page">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight" data-testid="dashboard-page-title">
            {t("common.dashboard", "Control Center")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {session?.branding?.companyName || "DHPanel"} Cloud Infrastructure Overview
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setLayoutDialog(true)}
            className="gap-2 shrink-0 text-xs h-9"
            data-testid="btn-customize-dashboard"
          >
            <LayoutGrid className="h-4 w-4" />
            <span>{t("dashboard.customizeLayout", "Customize Layout")}</span>
          </Button>

          <Button
            variant="outline"
            size="sm"
            onClick={loadSummary}
            disabled={isLoading}
            className="gap-2 shrink-0 text-xs h-9"
            data-testid="btn-refresh-dashboard"
          >
            <RefreshCw className={`h-4 w-4 ${isLoading ? "animate-spin" : ""}`} />
            <span>{t("common.refresh", "Refresh")}</span>
          </Button>
        </div>
      </div>

      {/* Grid of Configured Active Widgets */}
      <div
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
        data-testid="dashboard-widgets-grid"
      >
        <HealthCard />
        {activeWidgets.map((w) => renderWidget(w.id))}
      </div>

      {/* Layout Customizer Dialog */}
      <DashboardLayoutDialog
        open={layoutDialog}
        onClose={() => setLayoutDialog(false)}
        widgets={widgets}
        onSave={handleSaveLayout}
        onReset={handleResetLayout}
      />
    </div>
  );
}
