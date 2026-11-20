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
} from "@builtpanel/ui";
import {
  ShieldCheck,
  ShieldAlert,
  Ban,
  Globe,
  Plus,
  RefreshCw,
} from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";
import { apiFetch } from "@builtpanel/api";
import { ClientIpRule, IpRuleAction } from "./types";
import { ClientIpRulesTable } from "./ClientIpRulesTable";
import { AddIpRuleDialog } from "./AddIpRuleDialog";

export function ClientFirewallPage() {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [selectedDomain, setSelectedDomain] = useState("example.com");
  const [rules, setRules] = useState<ClientIpRule[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [defaultActionForAdd, setDefaultActionForAdd] = useState<IpRuleAction>("block");

  const fetchRules = useCallback(async () => {
    try {
      setIsLoading(true);
      const data = await apiFetch<ClientIpRule[]>(
        `/api/v1/client/firewall/rules?domain=${encodeURIComponent(selectedDomain)}`
      );
      setRules(data);
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
    fetchRules();
  }, [fetchRules]);

  // DoD: Client নিজের site-এর জন্য IP block করতে পারেন (Caddy পর্যায়ে)
  const handleAddRule = async (newRule: {
    domain: string;
    ip_cidr: string;
    action: IpRuleAction;
    notes: string;
  }) => {
    try {
      await apiFetch("/api/v1/client/firewall/rules", {
        method: "POST",
        body: JSON.stringify(newRule),
      });
      toast({
        title: t("common.success"),
        description:
          newRule.action === "block"
            ? t("firewall.ipBlockedSuccess", { ip: newRule.ip_cidr, domain: newRule.domain })
            : t("firewall.ipAllowedSuccess", { ip: newRule.ip_cidr, domain: newRule.domain }),
      });
      await fetchRules();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
      throw err;
    }
  };

  const handleDeleteRule = async (ruleId: string, ipCidr: string) => {
    try {
      await apiFetch(`/api/v1/client/firewall/rules/${encodeURIComponent(ruleId)}`, {
        method: "DELETE",
      });
      toast({
        title: t("common.success"),
        description: t("firewall.ruleDeletedSuccess", { ip: ipCidr }),
      });
      await fetchRules();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    }
  };

  const blockedCount = rules.filter((r) => r.action === "block").length;
  const allowedCount = rules.filter((r) => r.action === "allow").length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="flex items-center gap-2">
            <Ban className="h-6 w-6 text-primary" />
            <h1 className="text-2xl font-bold tracking-tight" data-testid="firewall-page-title">
              {t("firewall.pageTitle")}
            </h1>
            <Badge variant="outline" className="border-emerald-500/30 text-emerald-600 dark:text-emerald-400">
              {t("firewall.caddyLevelEnforced")}
            </Badge>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("firewall.pageSubtitle")}
          </p>
        </div>

        {/* Domain Selector, Add & Refresh */}
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-1.5 rounded-md border bg-background px-3 py-1.5 text-xs shadow-sm">
            <Globe className="h-3.5 w-3.5 text-muted-foreground" />
            <select
              data-testid="select-firewall-domain"
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
            onClick={fetchRules}
            disabled={isLoading}
            data-testid="btn-refresh-firewall"
            className="gap-1 text-xs"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isLoading ? "animate-spin" : ""}`} />
            {t("common.refresh")}
          </Button>

          <Button
            size="sm"
            onClick={() => {
              setDefaultActionForAdd("block");
              setIsAddOpen(true);
            }}
            data-testid="btn-open-add-ip-rule"
            className="gap-1.5 text-xs bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("firewall.btnBlockIp")}
          </Button>
        </div>
      </div>

      {/* Metrics Cards */}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <div className="rounded-lg border bg-card p-3 shadow-sm" data-testid="card-blocked-ips-count">
          <span className="text-[11px] text-muted-foreground uppercase">{t("firewall.blockedIps")}</span>
          <p className="text-2xl font-bold font-mono text-destructive mt-0.5">{blockedCount}</p>
        </div>
        <div className="rounded-lg border bg-card p-3 shadow-sm" data-testid="card-allowed-ips-count">
          <span className="text-[11px] text-muted-foreground uppercase">{t("firewall.allowedIps")}</span>
          <p className="text-2xl font-bold font-mono text-emerald-600 dark:text-emerald-400 mt-0.5">{allowedCount}</p>
        </div>
        <div className="rounded-lg border bg-card p-3 shadow-sm">
          <span className="text-[11px] text-muted-foreground uppercase">{t("firewall.enforcementScope")}</span>
          <p className="text-xs font-semibold mt-1 truncate font-mono">{selectedDomain}</p>
        </div>
        <div className="rounded-lg border bg-card p-3 shadow-sm">
          <span className="text-[11px] text-muted-foreground uppercase">{t("firewall.layer")}</span>
          <p className="flex items-center gap-1 text-xs font-semibold text-primary mt-1">
            <ShieldCheck className="h-3.5 w-3.5 text-emerald-500" />
            <span>Caddy HTTP Handler</span>
          </p>
        </div>
      </div>

      {/* Rules Table */}
      <ClientIpRulesTable
        rules={rules}
        domain={selectedDomain}
        onDeleteRule={handleDeleteRule}
        isLoading={isLoading}
      />

      {/* Add IP Modal */}
      <AddIpRuleDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        domain={selectedDomain}
        defaultAction={defaultActionForAdd}
        onConfirm={handleAddRule}
      />
    </div>
  );
}
