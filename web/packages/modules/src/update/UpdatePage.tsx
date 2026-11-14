import React, { useState, useEffect, useCallback } from "react";
import { useTranslation } from "@dhpanel/i18n";
import { apiFetch } from "@dhpanel/api";
import { UpdateSystemCard } from "./UpdateSystemCard";
import { UpdateHistoryTable } from "./UpdateHistoryTable";
import { PanelUpdateStatus } from "./types";

const INITIAL_STATUS: PanelUpdateStatus = {
  current_version: "0.1.0",
  channel: "stable",
  last_checked_at: new Date().toISOString(),
  available_update: {
    version: "0.2.0",
    release_date: new Date().toISOString(),
    channel: "stable",
    ed25519_signature: "e93f820c7419...ed25519...valid",
    key_id: "dhpanel-root-2026",
    checksum_sha256: "9b3c4f028d716...sha256",
    changelog: [
      "Multi-node clustering protocol v2 support",
      "Dynamic WAF rule updates with zero-restart reload",
      "Optimized MariaDB connection pool allocation",
    ],
    required_migrations: [
      "000015_cluster_nodes_v2.sql",
      "000016_waf_realtime_metrics.sql",
    ],
    can_rollback: true,
  },
  previous_version: null,
  can_rollback: false,
  history: [
    {
      id: "upd-001",
      from_version: "0.0.9",
      to_version: "0.1.0",
      channel: "stable",
      status: "success",
      timestamp: new Date(Date.now() - 86400000 * 7).toISOString(),
      details: "Initial release baseline upgrade with auth & audit logging",
    },
  ],
};

export function UpdatePage() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<PanelUpdateStatus>(INITIAL_STATUS);
  const [loading, setLoading] = useState(false);

  const loadStatus = useCallback(async () => {
    setLoading(true);
    try {
      const res = await apiFetch<PanelUpdateStatus>("/api/v1/update/status");
      setStatus(res);
    } catch {
      // Fallback to initial
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  return (
    <div className="space-y-6" data-testid="panel-update-page">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            {t("update.pageTitle", "Panel Update & Releases")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t(
              "update.pageSubtitle",
              "Manage control plane versions, switch update channels, and execute verified releases."
            )}
          </p>
        </div>
      </div>

      <UpdateSystemCard status={status} onRefresh={loadStatus} />

      <UpdateHistoryTable history={status.history} />
    </div>
  );
}
