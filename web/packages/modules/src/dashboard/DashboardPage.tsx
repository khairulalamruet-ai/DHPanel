import React from "react";
import { HealthCard } from "./HealthCard";
import { AdminQuickstartChecklist } from "../onboarding";
import { useTranslation } from "@dhpanel/i18n";
import { useSession } from "@dhpanel/auth";
import { Card, CardHeader, CardTitle, CardContent } from "@dhpanel/ui";
import { ShieldCheck, Cpu } from "lucide-react";

export function DashboardPage() {
  const { t } = useTranslation();
  const { data: session } = useSession();

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t("common.dashboard")}</h1>
        <p className="text-sm text-muted-foreground">
          {session?.branding?.companyName || "DHPanel"} Control Center
        </p>
      </div>

      <AdminQuickstartChecklist />

      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        <HealthCard />

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <ShieldCheck className="h-4 w-4 text-primary" />
              Security & Session
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-1 text-sm">
              <div className="flex justify-between">
                <span className="text-muted-foreground">User:</span>
                <span className="font-medium">{session?.user?.username || "Guest"}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Role:</span>
                <span className="font-mono text-xs">{session?.roles?.join(", ") || "N/A"}</span>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium flex items-center gap-2">
              <Cpu className="h-4 w-4 text-primary" />
              Active Modules
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-wrap gap-1">
              {session?.modules?.map((mod) => (
                <span
                  key={mod}
                  className="inline-flex items-center rounded bg-secondary px-2 py-0.5 text-xs font-medium text-secondary-foreground"
                >
                  {mod}
                </span>
              ))}
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
