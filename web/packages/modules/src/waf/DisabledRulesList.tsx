import React from "react";
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
} from "@builtpanel/ui";
import { ShieldCheck, ShieldOff, RotateCcw } from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";
import { DisabledRuleInfo } from "./types";

interface DisabledRulesListProps {
  rules: DisabledRuleInfo[];
  domain: string;
  onEnableRule: (ruleId: number, domain: string) => Promise<void>;
  isLoading?: boolean;
}

export function DisabledRulesList({
  rules,
  domain,
  onEnableRule,
  isLoading,
}: DisabledRulesListProps) {
  const { t } = useTranslation();

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle className="flex items-center gap-2 text-base font-semibold">
              <ShieldOff className="h-4 w-4 text-amber-500" />
              <span data-testid="title-disabled-rules">{t("waf.disabledRulesTitle")}</span>
            </CardTitle>
            <CardDescription className="text-xs text-muted-foreground mt-0.5">
              {t("waf.disabledRulesDescription", { domain })}
            </CardDescription>
          </div>
          <Badge variant="outline" className="font-mono text-xs" data-testid="badge-disabled-count">
            {rules.length} {t("waf.rulesDisabled")}
          </Badge>
        </div>
      </CardHeader>

      <CardContent>
        {isLoading ? (
          <p className="py-8 text-center text-xs text-muted-foreground">{t("common.loading")}</p>
        ) : rules.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-8 text-center" data-testid="empty-disabled-rules">
            <ShieldCheck className="h-8 w-8 text-emerald-500 mb-2 opacity-80" />
            <p className="text-sm font-medium text-foreground">{t("waf.noDisabledRulesTitle")}</p>
            <p className="text-xs text-muted-foreground mt-1 max-w-sm">
              {t("waf.noDisabledRulesDescription")}
            </p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="text-xs">{t("waf.ruleId")}</TableHead>
                  <TableHead className="text-xs">{t("waf.targetDomain")}</TableHead>
                  <TableHead className="text-xs">{t("waf.ruleDescription")}</TableHead>
                  <TableHead className="text-xs">{t("waf.disableReason")}</TableHead>
                  <TableHead className="text-xs">{t("waf.disabledAt")}</TableHead>
                  <TableHead className="text-xs text-right">{t("common.actions")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rules.map((r) => (
                  <TableRow key={`${r.domain}-${r.rule_id}`} data-testid={`disabled-rule-row-${r.rule_id}`}>
                    <TableCell className="font-mono text-xs font-semibold text-primary">
                      {r.rule_id}
                    </TableCell>
                    <TableCell className="font-mono text-xs">{r.domain}</TableCell>
                    <TableCell className="text-xs font-medium max-w-xs truncate">{r.description}</TableCell>
                    <TableCell className="text-xs text-muted-foreground max-w-xs truncate">
                      {r.reason || "—"}
                    </TableCell>
                    <TableCell className="text-xs font-mono text-muted-foreground">{r.disabled_at}</TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => onEnableRule(r.rule_id, r.domain)}
                        data-testid={`btn-enable-rule-${r.rule_id}`}
                        className="gap-1 text-xs h-7 hover:bg-emerald-50 dark:hover:bg-emerald-950/30 hover:text-emerald-600"
                      >
                        <RotateCcw className="h-3 w-3" />
                        {t("waf.reEnableRule")}
                      </Button>
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
