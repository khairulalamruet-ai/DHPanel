import React, { useState, useEffect } from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Button,
  Badge,
  useToast,
} from "@dhpanel/ui";
import { apiFetch } from "@dhpanel/api";
import { useTranslation } from "@dhpanel/i18n";
import {
  Globe,
  Database,
  Lock,
  Clock,
  Code,
  Archive,
  ShieldCheck,
  Folder,
  Server,
  HardDrive,
  Cpu,
  Layers,
  Activity,
  ExternalLink,
  RefreshCw,
  Box,
  CheckCircle2,
  Mail,
} from "lucide-react";
import { ClientDashboardData } from "./types";

export function formatBytes(mb: number): string {
  if (mb === -1) return "∞ Unlimited";
  if (mb < 1024) return `${mb} MB`;
  return `${(mb / 1024).toFixed(1)} GB`;
}

export function ClientDashboardPage() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const [data, setData] = useState<ClientDashboardData | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const fetchDashboardData = async () => {
    setIsLoading(true);
    try {
      const res = await apiFetch<ClientDashboardData>("/api/v1/client/dashboard");
      setData(res);
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err?.message || t("clientDashboard.toast.loadError"),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchDashboardData();
  }, []);

  if (isLoading || !data) {
    return (
      <div className="h-64 flex items-center justify-center space-x-2">
        <RefreshCw className="h-5 w-5 animate-spin text-primary" />
        <span className="text-sm text-muted-foreground">{t("common.loading")}</span>
      </div>
    );
  }

  const { tenant, container, resources, primary_domain, recent_events } = data;

  const diskPercent =
    resources.disk_limit_mb > 0
      ? Math.min(100, Math.round((resources.disk_used_mb / resources.disk_limit_mb) * 100))
      : 0;

  const memPercent =
    resources.memory_limit_mb > 0
      ? Math.min(100, Math.round((resources.memory_used_mb / resources.memory_limit_mb) * 100))
      : 0;

  const getProgressColor = (percent: number) => {
    if (percent > 85) return "bg-destructive";
    if (percent > 65) return "bg-amber-500";
    return "bg-primary";
  };

  const shortcutIcons: Record<string, React.ReactNode> = {
    files: <Folder className="h-5 w-5 text-blue-500" />,
    domains: <Globe className="h-5 w-5 text-emerald-500" />,
    databases: <Database className="h-5 w-5 text-indigo-500" />,
    ssl: <Lock className="h-5 w-5 text-green-500" />,
    cron: <Clock className="h-5 w-5 text-amber-500" />,
    php: <Code className="h-5 w-5 text-purple-500" />,
    backups: <Archive className="h-5 w-5 text-teal-500" />,
    security: <ShieldCheck className="h-5 w-5 text-primary" />,
  };

  return (
    <div className="space-y-6 max-w-6xl mx-auto pb-12">
      {/* 1. Tenant Welcome Banner */}
      <Card className="border shadow-sm bg-card overflow-hidden">
        <CardContent className="p-6">
          <div className="flex flex-col md:flex-row md:items-center justify-between gap-6">
            <div className="flex items-start gap-4">
              <div className="h-16 w-16 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center text-primary font-bold text-xl uppercase shadow-sm shrink-0">
                {tenant.username.slice(0, 2)}
              </div>
              <div className="space-y-1">
                <div className="flex items-center gap-3">
                  <h1 className="text-2xl font-bold tracking-tight text-foreground">
                    {t("clientDashboard.welcome", { username: tenant.username })}
                  </h1>
                  <Badge variant="default" className="bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30">
                    {t(`clientDashboard.status.${tenant.status}`)}
                  </Badge>
                  <Badge variant="outline" className="font-mono text-xs">
                    {tenant.plan_name}
                  </Badge>
                </div>

                <p className="text-xs text-muted-foreground flex items-center gap-2">
                  <span>{tenant.email}</span>
                  <span>•</span>
                  <span className="font-mono">{tenant.node_name}</span>
                </p>

                {primary_domain && (
                  <p className="text-xs font-mono text-primary flex items-center gap-1.5 pt-1">
                    <Globe className="h-3.5 w-3.5" />
                    <a
                      href={`http://${primary_domain.domain}`}
                      target="_blank"
                      rel="noreferrer"
                      className="hover:underline flex items-center gap-1"
                    >
                      {primary_domain.domain}
                      <ExternalLink className="h-3 w-3" />
                    </a>
                  </p>
                )}
              </div>
            </div>

            {/* Container Stack Pill & Shortcuts */}
            <div className="flex flex-col md:items-end gap-2 shrink-0">
              <div className="flex items-center gap-2 bg-muted/50 border rounded-lg px-3 py-1.5 text-xs">
                <span className="relative flex h-2.5 w-2.5">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-emerald-500" />
                </span>
                <span className="font-semibold text-foreground font-mono">
                  {`Stack: ${container.status}`}
                </span>
                <span className="text-muted-foreground">•</span>
                <span className="font-mono text-muted-foreground">
                  {`PHP ${container.php_version}`}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => (window.location.href = "/security")}
                  className="gap-1.5 text-xs"
                >
                  <ShieldCheck className="h-4 w-4" />
                  {t("clientDashboard.accountSecurity")}
                </Button>
                <Button
                  size="sm"
                  variant="default"
                  onClick={() => window.open(`http://${primary_domain.domain}`, "_blank")}
                  className="gap-1.5 text-xs"
                >
                  <ExternalLink className="h-4 w-4" />
                  {t("clientDashboard.viewWebsite")}
                </Button>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 2. Resource Quotas Section */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-bold tracking-tight text-foreground flex items-center gap-2">
              <Cpu className="h-5 w-5 text-primary" />
              <span>{t("clientDashboard.resources.title")}</span>
            </h2>
            <p className="text-xs text-muted-foreground">
              {t("clientDashboard.resources.desc")}
            </p>
          </div>
          <Badge variant="outline" className="text-xs font-mono">
            Enforced Quotas
          </Badge>
        </div>

        {/* 4 Primary Resource Progress Bars */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          {/* Disk Quota */}
          <Card className="border bg-card p-4 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-muted-foreground flex items-center gap-1.5">
                <HardDrive className="h-4 w-4 text-blue-500" />
                <span>{t("clientDashboard.resources.disk")}</span>
              </span>
              <span className="font-mono font-bold text-foreground">{diskPercent}%</span>
            </div>
            <div className="w-full h-2 rounded-full bg-muted overflow-hidden">
              <div
                className={`h-full rounded-full transition-all ${getProgressColor(diskPercent)}`}
                style={{ width: `${diskPercent}%` }}
              />
            </div>
            <div className="flex justify-between text-[11px] font-mono text-muted-foreground pt-1">
              <span>{formatBytes(resources.disk_used_mb)}</span>
              <span>/ {formatBytes(resources.disk_limit_mb)}</span>
            </div>
          </Card>

          {/* Bandwidth Quota */}
          <Card className="border bg-card p-4 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-muted-foreground flex items-center gap-1.5">
                <Globe className="h-4 w-4 text-emerald-500" />
                <span>{t("clientDashboard.resources.bandwidth")}</span>
              </span>
              <span className="font-mono font-bold text-foreground">
                {resources.bandwidth_limit_mb === -1 ? "Active" : "Quota"}
              </span>
            </div>
            <div className="w-full h-2 rounded-full bg-muted overflow-hidden">
              <div
                className="h-full rounded-full bg-emerald-500 transition-all"
                style={{ width: resources.bandwidth_limit_mb === -1 ? "18%" : "25%" }}
              />
            </div>
            <div className="flex justify-between text-[11px] font-mono text-muted-foreground pt-1">
              <span>{formatBytes(resources.bandwidth_used_mb)} used</span>
              <span>/ {formatBytes(resources.bandwidth_limit_mb)}</span>
            </div>
          </Card>

          {/* Memory RAM */}
          <Card className="border bg-card p-4 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-muted-foreground flex items-center gap-1.5">
                <Layers className="h-4 w-4 text-purple-500" />
                <span>{t("clientDashboard.resources.memory")}</span>
              </span>
              <span className="font-mono font-bold text-foreground">{memPercent}%</span>
            </div>
            <div className="w-full h-2 rounded-full bg-muted overflow-hidden">
              <div
                className={`h-full rounded-full transition-all ${getProgressColor(memPercent)}`}
                style={{ width: `${memPercent}%` }}
              />
            </div>
            <div className="flex justify-between text-[11px] font-mono text-muted-foreground pt-1">
              <span>{formatBytes(resources.memory_used_mb)}</span>
              <span>/ {formatBytes(resources.memory_limit_mb)}</span>
            </div>
          </Card>

          {/* CPU Load */}
          <Card className="border bg-card p-4 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-muted-foreground flex items-center gap-1.5">
                <Cpu className="h-4 w-4 text-amber-500" />
                <span>{t("clientDashboard.resources.cpu")}</span>
              </span>
              <span className="font-mono font-bold text-foreground">{resources.cpu_used_pct}%</span>
            </div>
            <div className="w-full h-2 rounded-full bg-muted overflow-hidden">
              <div
                className={`h-full rounded-full transition-all ${getProgressColor(resources.cpu_used_pct)}`}
                style={{ width: `${resources.cpu_used_pct}%` }}
              />
            </div>
            <div className="flex justify-between text-[11px] font-mono text-muted-foreground pt-1">
              <span>Load: {resources.cpu_used_pct}%</span>
              <span>/ {resources.cpu_cores} Cores</span>
            </div>
          </Card>
        </div>

        {/* Secondary Quota Metric Counters */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 pt-1">
          <div className="p-3 bg-muted/30 border rounded-lg flex items-center justify-between">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Folder className="h-3.5 w-3.5 text-muted-foreground" />
              <span>Inodes (Files)</span>
            </span>
            <span className="text-xs font-mono font-semibold">
              {resources.inodes_used.toLocaleString()} /{" "}
              {resources.inodes_limit === -1 ? "∞" : resources.inodes_limit.toLocaleString()}
            </span>
          </div>

          <div className="p-3 bg-muted/30 border rounded-lg flex items-center justify-between">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Database className="h-3.5 w-3.5 text-muted-foreground" />
              <span>Databases</span>
            </span>
            <span className="text-xs font-mono font-semibold">
              {resources.databases_used} / {resources.databases_limit === -1 ? "∞" : resources.databases_limit}
            </span>
          </div>

          <div className="p-3 bg-muted/30 border rounded-lg flex items-center justify-between">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Globe className="h-3.5 w-3.5 text-muted-foreground" />
              <span>Domains</span>
            </span>
            <span className="text-xs font-mono font-semibold">
              {resources.domains_used} / {resources.domains_limit === -1 ? "∞" : resources.domains_limit}
            </span>
          </div>

          <div className="p-3 bg-muted/30 border rounded-lg flex items-center justify-between">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Mail className="h-3.5 w-3.5 text-muted-foreground" />
              <span>Email Accounts</span>
            </span>
            <span className="text-xs font-mono font-semibold">
              {resources.emails_used} / {resources.emails_limit === -1 ? "∞" : resources.emails_limit}
            </span>
          </div>
        </div>
      </div>

      {/* 3. Quick Shortcuts (Favorites) */}
      <div className="space-y-3">
        <div>
          <h2 className="text-lg font-bold tracking-tight text-foreground flex items-center gap-2">
            <Box className="h-5 w-5 text-primary" />
            <span>{t("clientDashboard.shortcuts.title")}</span>
          </h2>
          <p className="text-xs text-muted-foreground">
            {t("clientDashboard.shortcuts.desc")}
          </p>
        </div>

        <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
          {data.shortcuts.map((sc) => {
            const isAvailable = sc.phase <= 4;
            return (
              <Card
                key={sc.id}
                onClick={() => {
                  if (sc.href && isAvailable) {
                    window.location.href = sc.href;
                  } else {
                    toast({
                      title: sc.title,
                      description: `Scheduled for delivery in Phase 0${sc.phase}`,
                    });
                  }
                }}
                className={`p-4 border bg-card hover:bg-muted/40 transition-all cursor-pointer relative group ${
                  !isAvailable ? "opacity-85" : ""
                }`}
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="p-2 rounded-lg bg-primary/10 border border-primary/20">
                    {shortcutIcons[sc.id] || <Box className="h-5 w-5 text-primary" />}
                  </div>
                  {isAvailable ? (
                    <Badge variant="secondary" className="text-[10px] font-mono">
                      Active
                    </Badge>
                  ) : (
                    <Badge variant="outline" className="text-[10px] font-mono text-muted-foreground">
                      Phase 0{sc.phase}
                    </Badge>
                  )}
                </div>
                <h3 className="text-sm font-semibold text-foreground group-hover:text-primary transition-colors">
                  {sc.title}
                </h3>
                <p className="text-[11px] text-muted-foreground mt-0.5">
                  {isAvailable ? "Manage settings" : `Phase 0${sc.phase} Preview`}
                </p>
              </Card>
            );
          })}
        </div>
      </div>

      {/* 4. Host Info & Server Details Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Primary Domain & SSL Details */}
        <Card className="border bg-card">
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Globe className="h-4 w-4 text-primary" />
              <span>{t("clientDashboard.primaryDomain.title")}</span>
            </CardTitle>
            <CardDescription>{t("clientDashboard.primaryDomain.desc")}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-xs">
            <div className="flex justify-between py-1.5 border-b">
              <span className="text-muted-foreground">Domain Name</span>
              <span className="font-mono font-semibold">{primary_domain.domain}</span>
            </div>
            <div className="flex justify-between py-1.5 border-b">
              <span className="text-muted-foreground">Document Root</span>
              <span className="font-mono font-semibold truncate max-w-[280px]" title={primary_domain.docroot}>
                {primary_domain.docroot}
              </span>
            </div>
            <div className="flex justify-between py-1.5 border-b">
              <span className="text-muted-foreground">SSL / TLS Certificate</span>
              <Badge variant="default" className="bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30 text-[10px]">
                {primary_domain.ssl_issuer} ({primary_domain.ssl_expires_in_days}d remaining)
              </Badge>
            </div>
            <div className="flex justify-between py-1.5 border-b">
              <span className="text-muted-foreground">PHP Runtime</span>
              <span className="font-mono font-semibold">{primary_domain.php_version}</span>
            </div>
            <div className="flex justify-between py-1.5">
              <span className="text-muted-foreground">Nameservers</span>
              <span className="font-mono text-muted-foreground">{tenant.nameservers.join(", ")}</span>
            </div>
          </CardContent>
        </Card>

        {/* Recent Activity / Stream */}
        <Card className="border bg-card">
          <CardHeader className="pb-3">
            <CardTitle className="text-sm font-semibold flex items-center gap-2">
              <Activity className="h-4 w-4 text-emerald-500" />
              <span>{t("clientDashboard.activity.title")}</span>
            </CardTitle>
            <CardDescription>{t("clientDashboard.activity.desc")}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {recent_events.map((ev) => (
              <div key={ev.id} className="flex items-start gap-3 p-2.5 rounded-lg border bg-muted/20 text-xs">
                <CheckCircle2 className="h-4 w-4 text-emerald-500 shrink-0 mt-0.5" />
                <div className="space-y-0.5 flex-1 min-w-0">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-semibold text-foreground font-mono">{ev.action}</span>
                    <span className="text-[10px] text-muted-foreground font-mono">
                      {new Date(ev.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                  <p className="text-muted-foreground truncate">{ev.description}</p>
                </div>
              </div>
            ))}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
