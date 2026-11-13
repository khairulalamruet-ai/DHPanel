import React, { useState, useEffect, useCallback } from "react";
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
import {
  ShieldAlert,
  ShieldCheck,
  ShieldOff,
  Globe,
  RefreshCw,
  Bug,
  Code2,
  Database,
  Radio,
} from "lucide-react";
import { useTranslation } from "@dhpanel/i18n";
import { apiFetch } from "@dhpanel/api";
import { WafEvent, WafPolicy, DisabledRuleInfo, WafStats, WafMode } from "./types";
import { WafEventsTable } from "./WafEventsTable";
import { DisabledRulesList } from "./DisabledRulesList";
import { WafPolicyCard } from "./WafPolicyCard";
import { DisableRuleDialog } from "./DisableRuleDialog";
import { WafEventDetailDialog } from "./WafEventDetailDialog";

export function ClientWafPage() {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [selectedDomain, setSelectedDomain] = useState("example.com");
  const [activeTab, setActiveTab] = useState<"events" | "disabled">("events");
  const [events, setEvents] = useState<WafEvent[]>([]);
  const [disabledRules, setDisabledRules] = useState<DisabledRuleInfo[]>([]);
  const [policy, setPolicy] = useState<WafPolicy | null>(null);
  const [stats, setStats] = useState<WafStats | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  // Modal states
  const [selectedEventForDetail, setSelectedEventForDetail] = useState<WafEvent | null>(null);
  const [disableModalTarget, setDisableModalTarget] = useState<{
    ruleId: number;
    domain: string;
    ruleMessage?: string;
  } | null>(null);

  const fetchData = useCallback(async () => {
    try {
      setIsLoading(true);
      const [eventsData, disabledData, policyData, statsData] = await Promise.all([
        apiFetch<WafEvent[]>(`/api/v1/client/waf/events?domain=${encodeURIComponent(selectedDomain)}`),
        apiFetch<DisabledRuleInfo[]>(`/api/v1/client/waf/rules/disabled?domain=${encodeURIComponent(selectedDomain)}`),
        apiFetch<WafPolicy>(`/api/v1/client/waf/policy?domain=${encodeURIComponent(selectedDomain)}`),
        apiFetch<WafStats>(`/api/v1/client/waf/stats?domain=${encodeURIComponent(selectedDomain)}`),
      ]);
      setEvents(eventsData);
      setDisabledRules(disabledData);
      setPolicy(policyData);
      setStats(statsData);
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    } finally {
      setIsLoading(false);
    }
  }, [selectedDomain, toast, t]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  // DoD: UI থেকে rule বন্ধ করা যায়
  const handleOpenDisableDialog = (ruleId: number, domain: string, ruleMessage?: string) => {
    setDisableModalTarget({ ruleId, domain, ruleMessage });
  };

  const handleConfirmDisableRule = async (ruleId: number, domain: string, reason: string) => {
    try {
      await apiFetch("/api/v1/client/waf/rules/disable", {
        method: "POST",
        body: JSON.stringify({ rule_id: ruleId, domain, reason }),
      });
      toast({
        title: t("common.success"),
        description: t("waf.ruleDisabledSuccess", { ruleId, domain }),
      });
      await fetchData();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
      throw err;
    }
  };

  // Re-enable rule
  const handleEnableRule = async (ruleId: number, domain: string) => {
    try {
      await apiFetch("/api/v1/client/waf/rules/enable", {
        method: "POST",
        body: JSON.stringify({ rule_id: ruleId, domain }),
      });
      toast({
        title: t("common.success"),
        description: t("waf.ruleEnabledSuccess", { ruleId, domain }),
      });
      await fetchData();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  // Update policy mode
  const handleUpdateMode = async (mode: WafMode) => {
    try {
      await apiFetch("/api/v1/client/waf/policy", {
        method: "POST",
        body: JSON.stringify({ domain: selectedDomain, mode }),
      });
      toast({
        title: t("common.success"),
        description: t("waf.modeUpdatedSuccess", { mode }),
      });
      await fetchData();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="flex items-center gap-2">
            <ShieldAlert className="h-6 w-6 text-primary" />
            <h1 className="text-2xl font-bold tracking-tight" data-testid="waf-page-title">
              {t("waf.pageTitle")}
            </h1>
            <Badge variant="outline" className="border-emerald-500/30 text-emerald-600 dark:text-emerald-400">
              Coraza WAF / OWASP CRS
            </Badge>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("waf.pageSubtitle")}
          </p>
        </div>

        {/* Domain Selector & Refresh */}
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1.5 rounded-md border bg-background px-3 py-1.5 text-xs shadow-sm">
            <Globe className="h-3.5 w-3.5 text-muted-foreground" />
            <select
              data-testid="select-waf-domain"
              value={selectedDomain}
              onChange={(e) => setSelectedDomain(e.target.value)}
              className="bg-transparent font-medium focus:outline-none"
            >
              <option value="example.com">example.com</option>
              <option value="shop.example.com">shop.example.com</option>
              <option value="blog.example.com">blog.example.com</option>
            </select>
          </div>

          <Button
            variant="outline"
            size="sm"
            onClick={fetchData}
            disabled={isLoading}
            data-testid="btn-refresh-waf"
            className="gap-1 text-xs"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? "animate-spin" : ""}`} />
            {t("common.refresh")}
          </Button>
        </div>
      </div>

      {/* Top Stats Cards & Policy Grid */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        {/* Policy mode card */}
        <WafPolicyCard
          policy={policy}
          onUpdateMode={handleUpdateMode}
          isLoading={isLoading}
        />

        {/* Stats 1: Blocked & Detected */}
        <Card className="flex flex-col justify-between">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase text-muted-foreground">
              {t("waf.trafficEnforcement")}
            </CardTitle>
          </CardHeader>
          <CardContent className="grid grid-cols-2 gap-2 text-center">
            <div className="rounded border bg-destructive/5 p-2" data-testid="card-waf-blocked">
              <span className="text-[11px] text-muted-foreground">{t("waf.blockedRequests")}</span>
              <p className="text-2xl font-bold font-mono text-destructive">
                {stats?.blocked_events ?? 0}
              </p>
            </div>
            <div className="rounded border bg-amber-500/5 p-2" data-testid="card-waf-detected">
              <span className="text-[11px] text-muted-foreground">{t("waf.detectedAlerts")}</span>
              <p className="text-2xl font-bold font-mono text-amber-600 dark:text-amber-400">
                {stats?.detected_events ?? 0}
              </p>
            </div>
          </CardContent>
        </Card>

        {/* Stats 2: Attack Categories */}
        <Card className="flex flex-col justify-between">
          <CardHeader className="pb-2">
            <CardTitle className="text-xs font-medium uppercase text-muted-foreground">
              {t("waf.attackVectorBreakdown")}
            </CardTitle>
          </CardHeader>
          <CardContent className="grid grid-cols-3 gap-1.5 text-center text-xs">
            <div className="rounded border p-1.5 bg-muted/20">
              <span className="text-[10px] text-muted-foreground font-mono">SQLi</span>
              <p className="font-mono font-bold">{stats?.sqli_count ?? 0}</p>
            </div>
            <div className="rounded border p-1.5 bg-muted/20">
              <span className="text-[10px] text-muted-foreground font-mono">XSS</span>
              <p className="font-mono font-bold">{stats?.xss_count ?? 0}</p>
            </div>
            <div className="rounded border p-1.5 bg-muted/20">
              <span className="text-[10px] text-muted-foreground font-mono">RCE</span>
              <p className="font-mono font-bold">{stats?.rce_count ?? 0}</p>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Tabs: Event Logs vs Disabled Rules */}
      <div className="flex border-b border-border">
        <button
          type="button"
          onClick={() => setActiveTab("events")}
          data-testid="tab-waf-events"
          className={`flex items-center gap-2 border-b-2 px-4 py-2.5 text-xs font-medium transition-colors ${
            activeTab === "events"
              ? "border-primary text-primary"
              : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
        >
          <ShieldAlert className="h-4 w-4" />
          <span>{t("waf.eventLogsTab")}</span>
          <Badge variant="secondary" className="font-mono text-[10px] ml-1">
            {events.length}
          </Badge>
        </button>

        <button
          type="button"
          onClick={() => setActiveTab("disabled")}
          data-testid="tab-waf-disabled-rules"
          className={`flex items-center gap-2 border-b-2 px-4 py-2.5 text-xs font-medium transition-colors ${
            activeTab === "disabled"
              ? "border-primary text-primary"
              : "border-transparent text-muted-foreground hover:text-foreground"
          }`}
        >
          <ShieldOff className="h-4 w-4" />
          <span>{t("waf.disabledRulesTab")}</span>
          <Badge variant="secondary" className="font-mono text-[10px] ml-1">
            {disabledRules.length}
          </Badge>
        </button>
      </div>

      {/* Tab Content */}
      {activeTab === "events" ? (
        <WafEventsTable
          events={events}
          onInspectEvent={(ev) => setSelectedEventForDetail(ev)}
          onDisableRule={handleOpenDisableDialog}
          isLoading={isLoading}
        />
      ) : (
        <DisabledRulesList
          rules={disabledRules}
          domain={selectedDomain}
          onEnableRule={handleEnableRule}
          isLoading={isLoading}
        />
      )}

      {/* Modals */}
      <DisableRuleDialog
        open={disableModalTarget !== null}
        onOpenChange={(open) => !open && setDisableModalTarget(null)}
        ruleId={disableModalTarget?.ruleId ?? null}
        ruleMessage={disableModalTarget?.ruleMessage}
        domain={disableModalTarget?.domain ?? selectedDomain}
        onConfirm={handleConfirmDisableRule}
      />

      <WafEventDetailDialog
        open={selectedEventForDetail !== null}
        onOpenChange={(open) => !open && setSelectedEventForDetail(null)}
        event={selectedEventForDetail}
        onDisableRule={(ruleId, domain, msg) => {
          setSelectedEventForDetail(null);
          handleOpenDisableDialog(ruleId, domain, msg);
        }}
      />
    </div>
  );
}
