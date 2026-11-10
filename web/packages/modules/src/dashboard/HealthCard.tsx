import React, { useEffect, useState } from "react";
import { apiFetch } from "@dhpanel/api";
import { Card, CardHeader, CardTitle, CardContent, Badge } from "@dhpanel/ui";
import { useTranslation } from "@dhpanel/i18n";
import { Activity, CheckCircle2, AlertCircle } from "lucide-react";

interface HealthResponse {
  status: string;
  version: string;
  timestamp?: string;
}

export function HealthCard() {
  const { t } = useTranslation();
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Calls GET /api/v1/public/health (MSW mock tagged TODO(api:T00.06) until backend is merged)
    apiFetch<HealthResponse>("/api/v1/public/health")
      .then((res) => {
        setHealth(res);
        setLoading(false);
      })
      .catch((err) => {
        setError(err.message || "Failed to check health");
        setLoading(false);
      });
  }, []);

  return (
    <Card className="max-w-md">
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
        <CardTitle className="text-sm font-medium flex items-center gap-2">
          <Activity className="h-4 w-4 text-primary" />
          {t("common.healthStatus")}
        </CardTitle>
        {health?.status === "ok" ? (
          <Badge variant="success" className="gap-1">
            <CheckCircle2 className="h-3 w-3" />
            {t("common.ok")}
          </Badge>
        ) : (
          <Badge variant="destructive" className="gap-1">
            <AlertCircle className="h-3 w-3" />
            {error || "Degraded"}
          </Badge>
        )}
      </CardHeader>
      <CardContent>
        {loading ? (
          <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
        ) : (
          <div className="space-y-1 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">{t("common.status")}:</span>
              <span className="font-medium uppercase">{health?.status || "Unknown"}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">{t("common.version")}:</span>
              <span className="font-mono text-xs">{health?.version || "N/A"}</span>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
