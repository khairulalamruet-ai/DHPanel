import React from "react";
import { Card, CardContent } from "@dhpanel/ui";
import { useTranslation } from "@dhpanel/i18n";
import {
  UserX,
  Flame,
  Bug,
  KeyRound,
  Lock,
  Box,
} from "lucide-react";
import { SecurityOverviewStats } from "./types";

interface SecurityStatsGridProps {
  stats: SecurityOverviewStats;
  onFilterCategory?: (category: string) => void;
}

export function SecurityStatsGrid({ stats, onFilterCategory }: SecurityStatsGridProps) {
  const { t } = useTranslation();

  const cards = [
    {
      id: "auth",
      label: t("securityCenter.kpiLoginFailures", "Login Failures (24h)"),
      count: stats.login_failures_24h,
      icon: <UserX className="h-5 w-5 text-amber-500" />,
      highlight: stats.login_failures_24h > 10 ? "border-amber-300 bg-amber-50/20" : "",
      testId: "kpi-login-failures",
    },
    {
      id: "waf",
      label: t("securityCenter.kpiWafBlocks", "WAF Blocks (24h)"),
      count: stats.waf_blocks_24h,
      icon: <Flame className="h-5 w-5 text-red-500" />,
      highlight: stats.waf_blocks_24h > 0 ? "border-red-300 bg-red-50/20" : "",
      testId: "kpi-waf-blocks",
    },
    {
      id: "malware",
      label: t("securityCenter.kpiMalware", "Malware Detections"),
      count: stats.malware_detected,
      icon: <Bug className="h-5 w-5 text-rose-600" />,
      highlight: stats.malware_detected > 0 ? "border-rose-300 bg-rose-50/30 font-bold" : "",
      testId: "kpi-malware",
    },
    {
      id: "auth_2fa",
      label: t("securityCenter.kpiAdminsWithout2fa", "Admins without 2FA"),
      count: stats.admins_without_2fa,
      icon: <KeyRound className="h-5 w-5 text-orange-500" />,
      highlight: stats.admins_without_2fa > 0 ? "border-orange-300 bg-orange-50/20" : "",
      testId: "kpi-admins-without-2fa",
    },
    {
      id: "ssl",
      label: t("securityCenter.kpiExpiringSsl", "Expiring / Expired SSL"),
      count: stats.expiring_ssl_certs,
      icon: <Lock className="h-5 w-5 text-yellow-600" />,
      highlight: stats.expiring_ssl_certs > 0 ? "border-yellow-300 bg-yellow-50/20" : "",
      testId: "kpi-expiring-ssl",
    },
    {
      id: "container",
      label: t("securityCenter.kpiOutdatedImages", "Outdated Images / CVEs"),
      count: stats.outdated_images,
      icon: <Box className="h-5 w-5 text-purple-500" />,
      highlight: stats.outdated_images > 0 ? "border-purple-300 bg-purple-50/20" : "",
      testId: "kpi-outdated-images",
    },
  ];

  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3" data-testid="security-stats-grid">
      {cards.map((card) => (
        <Card
          key={card.id}
          data-testid={card.testId}
          onClick={() => onFilterCategory?.(card.id)}
          className={`cursor-pointer transition-all hover:shadow-md border ${card.highlight}`}
        >
          <CardContent className="p-3.5 flex flex-col justify-between h-full">
            <div className="flex items-center justify-between">
              <span className="text-xs text-muted-foreground font-medium line-clamp-1">{card.label}</span>
              {card.icon}
            </div>
            <div className="mt-2 text-2xl font-black">{card.count}</div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
