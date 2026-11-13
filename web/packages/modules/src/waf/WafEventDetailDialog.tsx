import React from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  Badge,
} from "@dhpanel/ui";
import { ShieldAlert, ShieldOff, Globe, Terminal, Clock, MapPin } from "lucide-react";
import { useTranslation } from "@dhpanel/i18n";
import { WafEvent } from "./types";

interface WafEventDetailDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  event: WafEvent | null;
  onDisableRule: (ruleId: number, domain: string, ruleMessage: string) => void;
}

export function WafEventDetailDialog({
  open,
  onOpenChange,
  event,
  onDisableRule,
}: WafEventDetailDialogProps) {
  const { t } = useTranslation();

  if (!event) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <div className="flex items-center justify-between">
            <DialogTitle className="flex items-center gap-2 text-base">
              <ShieldAlert className="h-5 w-5 text-destructive" />
              <span data-testid="waf-detail-title">{t("waf.eventDetailsTitle")}</span>
            </DialogTitle>
            <Badge
              variant={event.action === "blocked" ? "destructive" : "outline"}
              className="font-mono text-xs uppercase"
              data-testid="badge-detail-action"
            >
              {event.action}
            </Badge>
          </div>
        </DialogHeader>

        <div className="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
          {/* Rule and Severity Summary */}
          <div className="rounded-lg border bg-muted/20 p-3 space-y-2">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">{t("waf.ruleId")}:</span>
                <span className="font-mono text-xs font-bold text-primary" data-testid="detail-rule-id">
                  {event.rule_id}
                </span>
                <Badge variant="outline" className="text-[10px] uppercase font-mono">
                  {event.category}
                </Badge>
              </div>
              <Badge
                variant={event.severity === "CRITICAL" || event.severity === "HIGH" ? "destructive" : "secondary"}
                className="text-[10px] font-mono"
              >
                {event.severity}
              </Badge>
            </div>
            <p className="font-mono text-xs font-medium text-foreground">{event.rule_message}</p>
          </div>

          {/* Request Metadata Grid */}
          <div className="grid grid-cols-2 gap-2 text-xs">
            <div className="rounded border p-2 bg-card">
              <span className="text-muted-foreground">{t("waf.targetDomain")}:</span>
              <p className="font-mono font-medium truncate mt-0.5">{event.domain}</p>
            </div>
            <div className="rounded border p-2 bg-card">
              <span className="text-muted-foreground">{t("waf.clientIp")}:</span>
              <p className="flex items-center gap-1 font-mono font-medium mt-0.5">
                <MapPin className="h-3 w-3 text-muted-foreground" />
                <span>{event.client_ip}</span>
                <span className="text-muted-foreground">({event.country})</span>
              </p>
            </div>
            <div className="rounded border p-2 bg-card">
              <span className="text-muted-foreground">{t("waf.httpMethodUri")}:</span>
              <p className="font-mono font-medium truncate mt-0.5">
                <span className="text-primary font-bold">{event.http_method}</span> {event.uri}
              </p>
            </div>
            <div className="rounded border p-2 bg-card">
              <span className="text-muted-foreground">{t("waf.timestamp")}:</span>
              <p className="flex items-center gap-1 font-mono font-medium mt-0.5">
                <Clock className="h-3 w-3 text-muted-foreground" />
                <span>{event.timestamp}</span>
              </p>
            </div>
          </div>

          {/* Matched Payload / Exploit String */}
          {event.matched_data && (
            <div>
              <span className="text-xs font-medium text-foreground">{t("waf.matchedExploitData")}:</span>
              <div className="mt-1 rounded-md border bg-muted/40 p-2.5 font-mono text-xs text-destructive break-all">
                {event.matched_data}
              </div>
            </div>
          )}

          {/* User Agent */}
          {event.user_agent && (
            <div>
              <span className="text-xs font-medium text-foreground">{t("waf.userAgent")}:</span>
              <div className="mt-1 rounded-md border bg-muted/20 p-2 font-mono text-[11px] text-muted-foreground truncate">
                {event.user_agent}
              </div>
            </div>
          )}

          {/* OWASP CRS Tags */}
          {event.tags && event.tags.length > 0 && (
            <div>
              <span className="text-xs font-medium text-foreground">{t("waf.crsTags")}:</span>
              <div className="mt-1 flex flex-wrap gap-1">
                {event.tags.map((tag) => (
                  <Badge key={tag} variant="outline" className="font-mono text-[10px] text-muted-foreground">
                    {tag}
                  </Badge>
                ))}
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="flex items-center justify-between border-t pt-3">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => onOpenChange(false)}
            data-testid="btn-close-waf-detail"
          >
            {t("common.close")}
          </Button>

          <Button
            type="button"
            variant="default"
            size="sm"
            onClick={() => {
              onOpenChange(false);
              onDisableRule(event.rule_id, event.domain, event.rule_message);
            }}
            data-testid="btn-detail-disable-rule"
            className="gap-1.5 bg-amber-600 hover:bg-amber-700 text-white text-xs"
          >
            <ShieldOff className="h-3.5 w-3.5" />
            {t("waf.disableRuleOnThisDomain")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
