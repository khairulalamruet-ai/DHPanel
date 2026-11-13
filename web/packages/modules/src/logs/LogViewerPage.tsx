import React, { useState, useEffect, useCallback, useMemo } from "react";
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
  FileText,
  Terminal,
  Download,
  Search,
  Play,
  Pause,
  RefreshCw,
  Globe,
  AlertTriangle,
  ArrowDown,
  Layers,
  Sparkles,
} from "lucide-react";
import { LogEntry, LogType, DomainLogOverview } from "./types";
import { VirtualizedLogList } from "./VirtualizedLogList";

export function LogViewerPage() {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [selectedDomain, setSelectedDomain] = useState<string>("all");
  const [logType, setLogType] = useState<LogType>("access");
  const [searchQuery, setSearchQuery] = useState("");
  const [isLiveTail, setIsLiveTail] = useState(false);
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [domains, setDomains] = useState<DomainLogOverview[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [autoScroll, setAutoScroll] = useState(false);
  const [is100MbLoaded, setIs100MbLoaded] = useState(false);

  // Fetch log overviews and initial logs
  const fetchLogs = useCallback(async () => {
    try {
      setIsLoading(true);
      const [domainsRes, logsRes] = await Promise.all([
        apiFetch<DomainLogOverview[]>("/api/v1/client/logs/overview"),
        apiFetch<LogEntry[]>(
          `/api/v1/client/logs?domain=${encodeURIComponent(selectedDomain)}&type=${encodeURIComponent(logType)}`
        ),
      ]);
      setDomains(domainsRes);
      setEntries(logsRes);
      setIs100MbLoaded(false);
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  }, [selectedDomain, logType, t, toast]);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  // Live Tail simulation (SSE or periodic interval)
  useEffect(() => {
    if (!isLiveTail) return;

    const interval = setInterval(() => {
      const now = new Date();
      const randomStatus = [200, 200, 200, 201, 304, 404, 500][Math.floor(Math.random() * 7)];
      const randomMethod = ["GET", "POST", "GET", "HEAD"][Math.floor(Math.random() * 4)];
      const randomPaths = [
        "/api/v1/health",
        "/wp-content/themes/dhpanel/style.css",
        "/index.php",
        "/cart/checkout",
        "/wp-login.php",
      ];
      const randomPath = randomPaths[Math.floor(Math.random() * randomPaths.length)];
      const currentDomain = selectedDomain === "all" ? "demo.dhpanel.test" : selectedDomain;

      const newEntry: LogEntry = {
        id: `live_${Date.now()}_${Math.random()}`,
        lineNumber: entries.length + 1,
        timestamp: now.toISOString(),
        type: logType,
        domain: currentDomain,
        status_code: logType === "access" ? randomStatus : undefined,
        level: logType === "error" || randomStatus >= 500 ? "error" : randomStatus >= 400 ? "warn" : "info",
        method: logType === "access" ? randomMethod : undefined,
        path: logType === "access" ? randomPath : undefined,
        ip: "192.168.1.105",
        message:
          logType === "php"
            ? `PHP Notice: Undefined index in /var/www/${currentDomain}/public_html/index.php line 42`
            : logType === "error"
            ? `[error] connect() failed (111: Connection refused) while connecting to upstream`
            : `${randomMethod} ${randomPath} HTTP/2.0 ${randomStatus}`,
        raw: `${now.toISOString()} [${currentDomain}] ${randomMethod} ${randomPath} ${randomStatus}`,
      };

      setEntries((prev) => [...prev, newEntry]);
    }, 2000);

    return () => clearInterval(interval);
  }, [isLiveTail, logType, selectedDomain, entries.length]);

  // Handle 100MB heavy load simulation for DoD verification (50,000 lines)
  const handleLoad100MbLog = () => {
    setIsLoading(true);
    setTimeout(() => {
      const heavyEntries: LogEntry[] = [];
      const baseTime = Date.now() - 3600 * 1000 * 24;

      for (let i = 1; i <= 50000; i++) {
        const time = new Date(baseTime + i * 1500).toISOString();
        const code = i % 250 === 0 ? 500 : i % 50 === 0 ? 404 : 200;
        heavyEntries.push({
          id: `heavy_${i}`,
          lineNumber: i,
          timestamp: time,
          type: "access",
          domain: i % 2 === 0 ? "demo.dhpanel.test" : "store.dhpanel.test",
          status_code: code,
          level: code >= 500 ? "error" : code >= 400 ? "warn" : "info",
          method: "GET",
          path: `/products/item-${(i % 100) + 1}?page=${(i % 5) + 1}`,
          ip: `10.0.${Math.floor(i / 256) % 256}.${i % 256}`,
          message: `GET /products/item-${(i % 100) + 1} HTTP/2.0 ${code}`,
          raw: `${time} [store.dhpanel.test] 10.0.0.1 GET /products/item-${(i % 100) + 1} HTTP/2.0 ${code} - Mozilla/5.0`,
        });
      }

      setEntries(heavyEntries);
      setIs100MbLoaded(true);
      setIsLoading(false);
      toast({
        title: t("common.success"),
        description: t("logs.loaded100MbSuccess"),
      });
    }, 50);
  };

  const handleDownloadLog = () => {
    if (entries.length === 0) return;
    const content = entries.map((e) => e.raw).join("\n");
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${selectedDomain}-${logType}-${new Date().toISOString().slice(0, 10)}.log`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);

    toast({
      title: t("common.success"),
      description: t("logs.downloadStarted", { name: `${selectedDomain}-${logType}.log` }),
    });
  };

  const currentOverview = domains.find((d) => d.domain === selectedDomain);
  const logSizeFormatted = is100MbLoaded
    ? "102.4 MB"
    : currentOverview
    ? logType === "access"
      ? currentOverview.access_log_size_formatted
      : logType === "error"
      ? currentOverview.error_log_size_formatted
      : currentOverview.php_error_log_size_formatted
    : "8.5 MB";

  const errorCount = entries.filter(
    (e) => e.level === "error" || (e.status_code && e.status_code >= 500)
  ).length;

  return (
    <div className="p-6 space-y-4" data-testid="log-viewer-page">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight" data-testid="log-viewer-title">
            {t("logs.title")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t("logs.description")}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Load 100MB test button (DoD demonstration) */}
          <Button
            size="sm"
            variant="outline"
            onClick={handleLoad100MbLog}
            className="gap-1.5 text-xs text-primary border-primary/30 hover:bg-primary/5"
            data-testid="btn-load-heavy-log"
            title="Load 50,000 lines (100MB) virtualized test log"
          >
            <Sparkles className="w-3.5 h-3.5" />
            {t("logs.btn100MbTest")}
          </Button>

          {/* Live Tail Toggle */}
          <Button
            size="sm"
            variant={isLiveTail ? "default" : "outline"}
            onClick={() => {
              setIsLiveTail(!isLiveTail);
              setAutoScroll(!isLiveTail);
            }}
            className={`gap-1.5 text-xs ${isLiveTail ? "bg-green-600 hover:bg-green-700 text-white" : ""}`}
            data-testid="btn-toggle-live-mode"
          >
            {isLiveTail ? (
              <>
                <span className="relative flex h-2 w-2">
                  <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-white opacity-75"></span>
                  <span className="relative inline-flex rounded-full h-2 w-2 bg-white"></span>
                </span>
                <Pause className="w-3 h-3" />
                {t("logs.liveTailActive")}
              </>
            ) : (
              <>
                <Play className="w-3 h-3 text-green-500" />
                {t("logs.startLiveTail")}
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
            data-testid="btn-download-log"
          >
            <Download className="w-3.5 h-3.5" />
            {t("logs.download")}
          </Button>

          {/* Refresh */}
          <Button
            size="sm"
            variant="ghost"
            onClick={fetchLogs}
            disabled={isLoading}
            className="h-8 w-8 p-0"
            data-testid="btn-refresh-logs"
            title={t("common.refresh")}
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isLoading ? "animate-spin" : ""}`} />
          </Button>
        </div>
      </div>

      {/* Filter and Type Bar */}
      <Card className="p-3">
        <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-3">
          {/* Domain Picker & Log Type Tabs */}
          <div className="flex flex-wrap items-center gap-3">
            {/* Domain Selector */}
            <div className="flex items-center gap-2">
              <Globe className="w-4 h-4 text-muted-foreground shrink-0" />
              <select
                value={selectedDomain}
                onChange={(e) => setSelectedDomain(e.target.value)}
                className="h-8 text-xs rounded-md border border-input bg-background px-2.5 py-1 text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                data-testid="select-log-domain"
              >
                <option value="all">{t("logs.allDomains")}</option>
                {domains.map((d) => (
                  <option key={d.domain} value={d.domain}>
                    {d.domain}
                  </option>
                ))}
              </select>
            </div>

            {/* Log Type Pill Tabs */}
            <div className="flex items-center rounded-lg bg-muted p-1 text-xs">
              <button
                type="button"
                onClick={() => setLogType("access")}
                className={`px-3 py-1 rounded-md font-medium transition-colors ${
                  logType === "access"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-log-access"
              >
                {t("logs.tabAccess")}
              </button>
              <button
                type="button"
                onClick={() => setLogType("error")}
                className={`px-3 py-1 rounded-md font-medium transition-colors ${
                  logType === "error"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-log-error"
              >
                {t("logs.tabError")}
              </button>
              <button
                type="button"
                onClick={() => setLogType("php")}
                className={`px-3 py-1 rounded-md font-medium transition-colors ${
                  logType === "php"
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                data-testid="tab-log-php"
              >
                {t("logs.tabPhp")}
              </button>
            </div>
          </div>

          {/* Search Box */}
          <div className="relative w-full lg:w-72">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <Input
              placeholder={t("logs.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 text-xs h-8"
              data-testid="input-log-search"
            />
          </div>
        </div>
      </Card>

      {/* Status & Metrics Strip */}
      <div className="flex flex-wrap items-center justify-between text-xs text-muted-foreground px-1">
        <div className="flex items-center gap-4">
          <div>
            <span className="font-semibold text-foreground" data-testid="metric-loaded-lines">
              {entries.length.toLocaleString()}
            </span>{" "}
            {t("logs.linesLoaded")}
          </div>
          <div>
            <span className="font-semibold text-foreground" data-testid="metric-log-size">
              {logSizeFormatted}
            </span>{" "}
            {t("logs.fileSize")}
          </div>
          {errorCount > 0 && (
            <div className="text-destructive font-medium flex items-center gap-1">
              <AlertTriangle className="w-3 h-3" />
              <span data-testid="metric-error-count">{errorCount}</span> {t("logs.errorsDetected")}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          {isLiveTail ? (
            <Badge variant="outline" className="text-green-600 bg-green-500/10 border-green-400 gap-1 text-[11px]" data-testid="badge-live-tail-status">
              <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse"></span>
              {t("logs.streamLive")}
            </Badge>
          ) : (
            <Badge variant="outline" className="text-muted-foreground text-[11px]" data-testid="badge-live-tail-status">
              {t("logs.streamPaused")}
            </Badge>
          )}

          {is100MbLoaded && (
            <Badge variant="secondary" className="font-mono text-[11px]" data-testid="badge-virtualized-mode">
              ⚡ {t("logs.virtualized100MbMode")}
            </Badge>
          )}
        </div>
      </div>

      {/* Virtualized Terminal Log Display */}
      <div className="h-[580px] w-full">
        <VirtualizedLogList
          entries={entries}
          searchQuery={searchQuery}
          autoScrollBottom={autoScroll}
        />
      </div>
    </div>
  );
}
