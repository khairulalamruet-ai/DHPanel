import React from "react";
import { Card, CardHeader, CardTitle, CardContent, Button, Badge } from "@builtpanel/ui";
import { useTranslation } from "@builtpanel/i18n";
import { ShieldAlert, ShieldCheck, Shield, RefreshCw } from "lucide-react";
import { SecurityOverviewStats } from "./types";

interface SecurityScoreCardProps {
  stats: SecurityOverviewStats;
  onRescan: () => void;
  isScanning: boolean;
}

export function SecurityScoreCard({ stats, onRescan, isScanning }: SecurityScoreCardProps) {
  const { t, i18n } = useTranslation();
  const isBn = i18n.language?.startsWith("bn");

  const getScoreColor = (score: number) => {
    if (score >= 90) return "text-emerald-500 border-emerald-500";
    if (score >= 75) return "text-amber-500 border-amber-500";
    return "text-red-500 border-red-500";
  };

  const getScoreBadge = (score: number) => {
    if (score >= 90) {
      return (
        <Badge variant="outline" className="bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-400 border-emerald-300">
          <ShieldCheck className="h-3.5 w-3.5 mr-1 inline" />
          {t("securityCenter.scoreGood", "Strong Protection")}
        </Badge>
      );
    }
    if (score >= 75) {
      return (
        <Badge variant="outline" className="bg-amber-50 text-amber-700 dark:bg-amber-950/40 dark:text-amber-400 border-amber-300">
          <Shield className="h-3.5 w-3.5 mr-1 inline" />
          {t("securityCenter.scoreWarning", "Attention Needed")}
        </Badge>
      );
    }
    return (
      <Badge variant="outline" className="bg-red-50 text-red-700 dark:bg-red-950/40 dark:text-red-400 border-red-300">
        <ShieldAlert className="h-3.5 w-3.5 mr-1 inline" />
        {t("securityCenter.scoreCritical", "Action Required")}
      </Badge>
    );
  };

  return (
    <Card data-testid="security-score-card" className="border shadow-sm">
      <CardHeader className="flex flex-row items-center justify-between pb-2">
        <div>
          <CardTitle className="text-xl font-bold flex items-center gap-2">
            <ShieldAlert className="h-6 w-6 text-primary" />
            {t("securityCenter.scoreTitle", "System Security Health Score")}
          </CardTitle>
          <p className="text-sm text-muted-foreground mt-0.5">
            {t("securityCenter.scoreSubtitle", "Real-time posture assessment across auth, WAF, SSL, malware, and container CVEs.")}
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={onRescan}
          disabled={isScanning}
          data-testid="btn-rescan-security"
          className="flex items-center gap-1.5"
        >
          <RefreshCw className={`h-4 w-4 ${isScanning ? "animate-spin" : ""}`} />
          {isScanning ? t("securityCenter.scanning", "Scanning...") : t("securityCenter.rescan", "Run Security Audit")}
        </Button>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4 items-center mt-2">
          {/* Circular/Numerical Score Display */}
          <div className="flex items-center space-x-4 border-r pr-4">
            <div
              data-testid="security-score-value"
              className={`flex flex-col items-center justify-center w-20 h-20 rounded-full border-4 font-black text-2xl ${getScoreColor(
                stats.score
              )}`}
            >
              <span>{stats.score}</span>
              <span className="text-[10px] text-muted-foreground font-semibold uppercase tracking-wider">/ 100</span>
            </div>
            <div>
              <div className="font-semibold text-lg">{getScoreBadge(stats.score)}</div>
              <p className="text-xs text-muted-foreground mt-1">
                {isBn ? `গ্রেড ${stats.grade} নিরাপত্তা স্তর` : `Grade ${stats.grade} Security Level`}
              </p>
            </div>
          </div>

          {/* Quick Metrics */}
          <div className="col-span-3 grid grid-cols-2 sm:grid-cols-4 gap-3 text-center">
            <div className="p-2.5 rounded-lg bg-red-50/60 dark:bg-red-950/20 border border-red-200/50">
              <span className="text-2xl font-bold text-red-600 dark:text-red-400" data-testid="stat-critical-count">
                {stats.critical_count}
              </span>
              <p className="text-xs text-muted-foreground font-medium mt-0.5">
                {t("securityCenter.criticalIssues", "Critical Issues")}
              </p>
            </div>
            <div className="p-2.5 rounded-lg bg-amber-50/60 dark:bg-amber-950/20 border border-amber-200/50">
              <span className="text-2xl font-bold text-amber-600 dark:text-amber-400" data-testid="stat-warning-count">
                {stats.warning_count}
              </span>
              <p className="text-xs text-muted-foreground font-medium mt-0.5">
                {t("securityCenter.warningIssues", "Warnings")}
              </p>
            </div>
            <div className="p-2.5 rounded-lg bg-blue-50/60 dark:bg-blue-950/20 border border-blue-200/50">
              <span className="text-2xl font-bold text-blue-600 dark:text-blue-400" data-testid="stat-info-count">
                {stats.info_count}
              </span>
              <p className="text-xs text-muted-foreground font-medium mt-0.5">
                {t("securityCenter.infoIssues", "Advisories")}
              </p>
            </div>
            <div className="p-2.5 rounded-lg bg-emerald-50/60 dark:bg-emerald-950/20 border border-emerald-200/50">
              <span className="text-2xl font-bold text-emerald-600 dark:text-emerald-400" data-testid="stat-resolved-count">
                {stats.resolved_count}
              </span>
              <p className="text-xs text-muted-foreground font-medium mt-0.5">
                {t("securityCenter.resolvedIssues", "Resolved")}
              </p>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
