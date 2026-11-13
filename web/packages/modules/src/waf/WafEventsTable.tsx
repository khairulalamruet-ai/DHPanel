import React, { useState, useMemo } from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Table,
  TableHeader,
  TableRow,
  TableHead,
  TableBody,
  TableCell,
  Button,
  Badge,
  Input,
} from "@dhpanel/ui";
import {
  ShieldAlert,
  ShieldOff,
  Search,
  Eye,
  Filter,
} from "lucide-react";
import { useTranslation } from "@dhpanel/i18n";
import { WafEvent, WafCategory, WafAction } from "./types";

interface WafEventsTableProps {
  events: WafEvent[];
  onInspectEvent: (event: WafEvent) => void;
  onDisableRule: (ruleId: number, domain: string, ruleMessage: string) => void;
  isLoading?: boolean;
}

export function WafEventsTable({
  events,
  onInspectEvent,
  onDisableRule,
  isLoading,
}: WafEventsTableProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [actionFilter, setActionFilter] = useState<string>("all");

  const filteredEvents = useMemo(() => {
    return events.filter((ev) => {
      if (categoryFilter !== "all" && ev.category !== categoryFilter) return false;
      if (actionFilter !== "all" && ev.action !== actionFilter) return false;
      if (!searchQuery.trim()) return true;

      const q = searchQuery.toLowerCase();
      return (
        ev.client_ip.toLowerCase().includes(q) ||
        ev.uri.toLowerCase().includes(q) ||
        ev.rule_id.toString().includes(q) ||
        ev.rule_message.toLowerCase().includes(q) ||
        ev.domain.toLowerCase().includes(q)
      );
    });
  }, [events, searchQuery, categoryFilter, actionFilter]);

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <CardTitle className="flex items-center gap-2 text-base font-semibold">
              <ShieldAlert className="h-4 w-4 text-destructive" />
              <span data-testid="title-waf-events">{t("waf.eventLogsTitle")}</span>
            </CardTitle>
            <CardDescription className="text-xs text-muted-foreground mt-0.5">
              {t("waf.eventLogsDescription")}
            </CardDescription>
          </div>

          <Badge variant="outline" className="font-mono text-xs w-fit" data-testid="badge-filtered-events-count">
            {filteredEvents.length} {t("waf.eventsLogged")}
          </Badge>
        </div>

        {/* Filters */}
        <div className="flex flex-wrap items-center gap-2 pt-2">
          <div className="relative flex-1 min-w-[200px]">
            <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
            <Input
              data-testid="input-waf-search"
              placeholder={t("waf.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 text-xs h-8"
            />
          </div>

          <select
            data-testid="select-category-filter"
            value={categoryFilter}
            onChange={(e) => setCategoryFilter(e.target.value)}
            className="rounded-md border border-input bg-background px-2.5 py-1 text-xs h-8 shadow-sm focus:outline-none"
          >
            <option value="all">{t("waf.allCategories")}</option>
            <option value="sqli">SQLi (SQL Injection)</option>
            <option value="xss">XSS (Cross-Site Scripting)</option>
            <option value="rce">RCE (Remote Code Exec)</option>
            <option value="scanner">Scanners & Bots</option>
            <option value="protocol">Protocol Violations</option>
          </select>

          <select
            data-testid="select-action-filter"
            value={actionFilter}
            onChange={(e) => setActionFilter(e.target.value)}
            className="rounded-md border border-input bg-background px-2.5 py-1 text-xs h-8 shadow-sm focus:outline-none"
          >
            <option value="all">{t("waf.allActions")}</option>
            <option value="blocked">{t("waf.actionBlocked")}</option>
            <option value="detected">{t("waf.actionDetected")}</option>
          </select>
        </div>
      </CardHeader>

      <CardContent>
        {isLoading ? (
          <p className="py-8 text-center text-xs text-muted-foreground">{t("common.loading")}</p>
        ) : filteredEvents.length === 0 ? (
          <p className="py-8 text-center text-xs text-muted-foreground">{t("common.noData")}</p>
        ) : (
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="text-xs">{t("waf.timestamp")}</TableHead>
                  <TableHead className="text-xs">{t("waf.clientIp")}</TableHead>
                  <TableHead className="text-xs">{t("waf.targetDomain")}</TableHead>
                  <TableHead className="text-xs">{t("waf.requestUri")}</TableHead>
                  <TableHead className="text-xs">{t("waf.ruleTriggered")}</TableHead>
                  <TableHead className="text-xs">{t("waf.category")}</TableHead>
                  <TableHead className="text-xs">{t("waf.action")}</TableHead>
                  <TableHead className="text-xs text-right">{t("common.actions")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredEvents.map((ev) => (
                  <TableRow key={ev.id} data-testid={`waf-event-row-${ev.id}`}>
                    <TableCell className="font-mono text-[11px] whitespace-nowrap text-muted-foreground">
                      {ev.timestamp}
                    </TableCell>
                    <TableCell className="font-mono text-xs whitespace-nowrap">
                      <span>{ev.client_ip}</span>
                      <span className="ml-1 text-[10px] text-muted-foreground">({ev.country})</span>
                    </TableCell>
                    <TableCell className="font-mono text-xs">{ev.domain}</TableCell>
                    <TableCell className="font-mono text-xs max-w-[200px] truncate">
                      <span className="font-bold text-primary mr-1">{ev.http_method}</span>
                      <span title={ev.uri}>{ev.uri}</span>
                    </TableCell>
                    <TableCell className="text-xs max-w-[220px]">
                      <div className="flex items-center gap-1">
                        <span className="font-mono font-bold text-primary text-[11px]">
                          {ev.rule_id}
                        </span>
                        <span className="truncate text-muted-foreground" title={ev.rule_message}>
                          {ev.rule_message}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className="font-mono text-[10px] uppercase">
                        {ev.category}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant={ev.action === "blocked" ? "destructive" : "secondary"}
                        className="text-[10px] font-mono uppercase"
                        data-testid={`badge-action-${ev.id}`}
                      >
                        {ev.action}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right whitespace-nowrap">
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => onInspectEvent(ev)}
                          data-testid={`btn-inspect-event-${ev.id}`}
                          className="h-7 w-7 p-0"
                          title={t("waf.inspectDetails")}
                        >
                          <Eye className="h-3.5 w-3.5 text-muted-foreground" />
                        </Button>

                        {/* DoD: UI থেকে rule বন্ধ করার বোতাম */}
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => onDisableRule(ev.rule_id, ev.domain, ev.rule_message)}
                          data-testid={`btn-disable-rule-${ev.id}`}
                          className="h-7 gap-1 text-[11px] hover:bg-amber-50 dark:hover:bg-amber-950/30 hover:text-amber-600"
                          title={t("waf.disableRuleOnThisDomain")}
                        >
                          <ShieldOff className="h-3 w-3 text-amber-500" />
                          <span>{t("waf.disableRuleButtonShort")}</span>
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
