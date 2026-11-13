import React, { useState, useEffect, useCallback } from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Button,
  Badge,
  Input,
  useToast,
} from "@dhpanel/ui";
import { apiFetch } from "@dhpanel/api";
import { useTranslation } from "@dhpanel/i18n";
import {
  Server,
  Activity,
  Download,
  Search,
  Play,
  Pause,
  RefreshCw,
  ShieldCheck,
  AlertTriangle,
  Cpu,
  Layers,
  FileText,
} from "lucide-react";
import { SystemLogEntry, SystemLogService, SystemLogLevel, SystemLogServiceStats } from "./types";
import { VirtualizedLogList } from "./VirtualizedLogList";

export function AdminSystemLogsPage() {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [service, setService] = useState<SystemLogService>("control-plane");
  const [node, setNode] = useState<string>("node-01");
  const [level, setLevel] = useState<SystemLogLevel>("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [isLiveTail, setIsLiveTail] = useState(false);
  const [entries, setEntries] = useState<SystemLogEntry[]>([]);
  const [stats, setStats] = useState<SystemLogServiceStats[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const fetchLogs = useCallback(async () => {
    try {
      setIsLoading(true);
      const [statsRes, logsRes] = await Promise.all([
        apiFetch<SystemLogServiceStats[]>("/api/v1/admin/logs/system/stats"),
        apiFetch<SystemLogEntry[]>(
          `/api/v1/admin/logs/system?service=${encodeURIComponent(service)}&node=${encodeURIComponent(node)}&level=${encodeURIComponent(level)}`
        ),
      ]);
      setStats(statsRes);
      setEntries(logsRes);
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  }, [service, node, level, t, toast]);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  // Live Tail simulation for admin system logs
  useEffect(() => {
    if (!isLiveTail) return;

    const interval = setInterval(() => {
      const now = new Date();
      const levels: ("info" | "warn" | "error" | "debug")[] = ["info", "info", "info", "warn", "error"];
      const randomLevel = levels[Math.floor(Math.random() * levels.length)];

      const sampleMessages: Record<SystemLogService, string[]> = {
        "control-plane": [
          "HTTP Request finished in 18ms 200 OK /api/v1/auth/me",
          "Background worker processed queue job #4892 [SyncCronJobs]",
          "Session token validated for admin user #1 (cluster master)",
          "Audit event emitted: settings.update by user_id=1",
        ],
        agent: [
          "Agent health heartbeat ping ACK received from control-plane",
          "Collected cgroup v2 metrics for 12 tenant containers: CPU 1.4%, Mem 24%",
          "Syncing node iptables firewall rules for ingress ports 80, 443",
          "Storage scan finished: /var/lib/dhpanel/tenants disk usage 42.1 GB",
        ],
        caddy: [
          "tls.handshake: completed successfully client=192.168.1.102 cipher=TLS_AES_256_GCM_SHA384",
          "http.log.access: handled request method=GET uri=/ host=demo.dhpanel.test status=200",
          "reverse_proxy: upstream backend 127.0.0.1:8000 response header received in 4.2ms",
        ],
        auth: [
          "Authentication success: user 'admin' via session cookie",
          "RBAC permission checked: system.logs.read -> GRANTED",
          "CSRF token validated for origin http://localhost:5173",
        ],
      };

      const messages = sampleMessages[service];
      const randomMsg = messages[Math.floor(Math.random() * messages.length)];

      const newEntry: SystemLogEntry = {
        id: `sys_live_${Date.now()}_${Math.random()}`,
        lineNumber: entries.length + 1,
        timestamp: now.toISOString(),
        service,
        node,
        level: randomLevel,
        component: service === "control-plane" ? "api" : service,
        message: randomMsg,
        raw: `${now.toISOString()} [${node}] [${service}] [${randomLevel.toUpperCase()}] ${randomMsg}`,
      };

      setEntries((prev) => [...prev, newEntry]);
    }, 2000);

    return () => clearInterval(interval);
  }, [isLiveTail, service, node, entries.length]);

  const handleDownloadLog = () => {
    if (entries.length === 0) return;
    const content = entries.map((e) => e.raw).join("\n");
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `system-${service}-${node}-${new Date().toISOString().slice(0, 10)}.log`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    toast({
      title: t("common.success"),
      description: t("adminSystemLogs.downloadStarted", { name: `system-${service}-${node}.log` }),
    });
  };

  const currentStat = stats.find((s) => s.service === service);
  const errorCount = entries.filter((e) => e.level === "error" || e.level === "fatal").length;

  return (
    <div className="p-6 space-y-4" data-testid="admin-system-logs-page">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight" data-testid="admin-system-logs-title">
            {t("adminSystemLogs.title")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t("adminSystemLogs.description")}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Live Tail Toggle */}
          <Button
            size="sm"
            variant={isLiveTail ? "default" : "outline"}
            onClick={() => setIsLiveTail(!isLiveTail)}
            className={`gap-1.5 text-xs ${isLiveTail ? "bg-green-600 hover:bg-green-700 text-white" : ""}`}
            data-testid="btn-toggle-admin-live-mode"
          >
            {isLiveTail ? (
              <>
                <span className="relative flex h-2 w-2">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-white opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-white"></span>
                </span>
                <Pause className="w-3 h-3" />
                {t("adminSystemLogs.liveTailActive")}
              </>
            ) : (
              <>
                <Play className="w-3 h-3 text-green-500" />
                {t("adminSystemLogs.startLiveTail")}
              </>
            )}
          </Button>

          {/* Download Log */}
          <Button
            size="sm"
            variant="outline"
            onClick={handleDownloadLog}
            disabled={entries.length === 0}
            className="gap-1.5 text-xs"
            data-testid="btn-download-admin-log"
          >
            <Download className="w-3.5 h-3.5" />
            {t("common.download")}
          </Button>

          {/* Refresh */}
          <Button
            size="sm"
            variant="ghost"
            onClick={fetchLogs}
            disabled={isLoading}
            className="h-8 w-8 p-0"
            data-testid="btn-refresh-admin-logs"
            title={t("common.refresh")}
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isLoading ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      {/* Filter and Service Selection */}
      <Card className="p-3">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-3">
          {/* Service Tabs */}
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex items-center rounded-lg bg-muted p-1 text-xs">
              <button
                type="button"
                onClick={() => setService("control-plane")}
                className={`px-3 py-1 rounded-md font-medium transition-colors flex items-center gap-1.5 ${
                  service === "control-plane"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-service-control-plane"
              >
                <Server className="w-3.5 h-3.5" />
                {t("adminSystemLogs.serviceControlPlane")}
              </button>
              <button
                type="button"
                onClick={() => setService("agent")}
                className={`px-3 py-1 rounded-md font-medium transition-colors flex items-center gap-1.5 ${
                  service === "agent"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-service-agent"
              >
                <Cpu className="w-3.5 h-3.5" />
                {t("adminSystemLogs.serviceAgent")}
              </button>
              <button
                type="button"
                onClick={() => setService("caddy")}
                className={`px-3 py-1 rounded-md font-medium transition-colors flex items-center gap-1.5 ${
                  service === "caddy"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-service-caddy"
              >
                <Activity className="w-3.5 h-3.5" />
                {t("adminSystemLogs.serviceCaddy")}
              </button>
              <button
                type="button"
                onClick={() => setService("auth")}
                className={`px-3 py-1 rounded-md font-medium transition-colors flex items-center gap-1.5 ${
                  service === "auth"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-service-auth"
              >
                <ShieldCheck className="w-3.5 h-3.5" />
                {t("adminSystemLogs.serviceAuth")}
              </button>
            </div>

            {/* Node Selector */}
            <select
              value={node}
              onChange={(e) => setNode(e.target.value)}
              className="h-8 text-xs rounded-md border border-input bg-background px-2.5 py-1 text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              data-testid="select-admin-log-node"
            >
              <option value="node-01">node-01 (master)</option>
              <option value="node-02">node-02 (worker)</option>
            </select>

            {/* Level Filter */}
            <select
              value={level}
              onChange={(e) => setLevel(e.target.value as SystemLogLevel)}
              className="h-8 text-xs rounded-md border border-input bg-background px-2.5 py-1 text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              data-testid="select-admin-log-level"
            >
              <option value="all">{t("adminSystemLogs.levelAll")}</option>
              <option value="debug">DEBUG</option>
              <option value="info">INFO</option>
              <option value="warn">WARN</option>
              <option value="error">ERROR / FATAL</option>
            </select>
          </div>

          {/* Search Box */}
          <div className="relative w-full lg:w-72">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder={t("adminSystemLogs.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 text-xs h-8"
              data-testid="input-admin-log-search"
            />
          </div>
        </div>
      </Card>

      {/* Metrics Strip */}
      <div className="flex flex-wrap items-center justify-between text-xs text-muted-foreground px-1">
        <div className="flex items-center gap-4">
          <div>
            <span className="font-semibold text-foreground" data-testid="metric-admin-lines">
              {entries.length.toLocaleString()}
            </span>{" "}
            {t("adminSystemLogs.linesLoaded")}
          </div>
          <div>
            <span className="font-semibold text-foreground" data-testid="metric-admin-size">
              {currentStat ? currentStat.size_formatted : "6.4 MB"}
            </span>{" "}
            {t("adminSystemLogs.fileSize")}
          </div>
          {errorCount > 0 && (
            <div className="text-destructive font-medium flex items-center gap-1">
              <AlertTriangle className="w-3 h-3" />
              <span data-testid="metric-admin-errors">{errorCount}</span> {t("adminSystemLogs.errorsDetected")}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          {isLiveTail ? (
            <Badge variant="outline" className="text-green-600 bg-green-500/10 border-green-400 gap-1 text-[11px]" data-testid="badge-admin-live-status">
              <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse"></span>
              {t("adminSystemLogs.streamLive")}
            </Badge>
          ) : (
            <Badge variant="outline" className="text-muted-foreground text-[11px]" data-testid="badge-admin-live-status">
              {t("adminSystemLogs.streamPaused")}
            </Badge>
          )}

          <Badge variant="secondary" className="font-mono text-[11px]" data-testid="badge-service-active">
            {service}
          </Badge>
        </div>
      </div>

      {/* Virtualized Log Container */}
      <div className="h-[580px] w-full">
        <VirtualizedLogList
          entries={entries.map((e) => ({
            ...e,
            domain: e.service,
          }))}
          searchQuery={searchQuery}
          autoScrollBottom={isLiveTail}
        />
      </div>
    </div>
  );
}
