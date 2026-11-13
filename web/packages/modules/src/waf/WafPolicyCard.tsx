import React from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Badge,
  Button,
} from "@dhpanel/ui";
import { Shield, ShieldAlert, ShieldCheck, ShieldX, Sliders } from "lucide-react";
import { useTranslation } from "@dhpanel/i18n";
import { WafPolicy, WafMode } from "./types";

interface WafPolicyCardProps {
  policy: WafPolicy | null;
  onUpdateMode: (mode: WafMode) => Promise<void>;
  isLoading?: boolean;
}

export function WafPolicyCard({
  policy,
  onUpdateMode,
  isLoading,
}: WafPolicyCardProps) {
  const { t } = useTranslation();

  if (isLoading || !policy) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-semibold">{t("waf.policyCardTitle")}</CardTitle>
          <CardDescription className="text-xs text-muted-foreground">{t("common.loading")}</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  const getModeBadge = (mode: WafMode) => {
    switch (mode) {
      case "block":
        return (
          <Badge className="gap-1 bg-emerald-600 hover:bg-emerald-600 font-mono text-xs">
            <ShieldCheck className="h-3 w-3" />
            <span>{t("waf.modeBlock")}</span>
          </Badge>
        );
      case "detect":
        return (
          <Badge variant="outline" className="gap-1 border-amber-500/40 text-amber-600 dark:text-amber-400 font-mono text-xs">
            <ShieldAlert className="h-3 w-3" />
            <span>{t("waf.modeDetect")}</span>
          </Badge>
        );
      case "off":
        return (
          <Badge variant="secondary" className="gap-1 font-mono text-xs">
            <ShieldX className="h-3 w-3" />
            <span>{t("waf.modeOff")}</span>
          </Badge>
        );
    }
  };

  return (
    <Card className="flex flex-col justify-between">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-base font-semibold">
            <Sliders className="h-4 w-4 text-primary" />
            <span data-testid="title-waf-policy">{t("waf.policyCardTitle")}</span>
          </CardTitle>
          <div data-testid="badge-active-waf-mode">{getModeBadge(policy.mode)}</div>
        </div>
        <CardDescription className="text-xs text-muted-foreground">
          {t("waf.policyCardDescription", { domain: policy.domain })}
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-3">
        <div className="flex items-center justify-between text-xs rounded border p-2 bg-muted/20">
          <span className="text-muted-foreground">{t("waf.paranoiaLevel")}:</span>
          <span className="font-mono font-medium">PL{policy.paranoia_level} (Standard OWASP CRS)</span>
        </div>

        <div>
          <label className="text-xs font-medium text-foreground block mb-1.5">
            {t("waf.selectProtectionMode")}:
          </label>
          <div className="grid grid-cols-3 gap-1.5">
            <Button
              type="button"
              variant={policy.mode === "block" ? "default" : "outline"}
              size="sm"
              className="text-xs h-8"
              data-testid="btn-waf-mode-block"
              onClick={() => onUpdateMode("block")}
            >
              {t("waf.modeBlock")}
            </Button>
            <Button
              type="button"
              variant={policy.mode === "detect" ? "default" : "outline"}
              size="sm"
              className="text-xs h-8"
              data-testid="btn-waf-mode-detect"
              onClick={() => onUpdateMode("detect")}
            >
              {t("waf.modeDetect")}
            </Button>
            <Button
              type="button"
              variant={policy.mode === "off" ? "default" : "outline"}
              size="sm"
              className="text-xs h-8"
              data-testid="btn-waf-mode-off"
              onClick={() => onUpdateMode("off")}
            >
              {t("waf.modeOff")}
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
