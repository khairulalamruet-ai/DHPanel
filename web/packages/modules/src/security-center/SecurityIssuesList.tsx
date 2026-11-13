import React, { useState } from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Input,
  Button,
  Badge,
} from "@dhpanel/ui";
import { useTranslation } from "@dhpanel/i18n";
import {
  Search,
  CheckCircle,
  AlertOctagon,
  AlertTriangle,
  Info,
  Wrench,
  ExternalLink,
  Shield,
  Filter,
} from "lucide-react";
import { SecurityIssue, SecurityCategory, SecuritySeverity } from "./types";

interface SecurityIssuesListProps {
  issues: SecurityIssue[];
  onOpenFix: (issue: SecurityIssue) => void;
  onNavigateTo: (path: string) => void;
}

export function SecurityIssuesList({
  issues,
  onOpenFix,
  onNavigateTo,
}: SecurityIssuesListProps) {
  const { t, i18n } = useTranslation();
  const [search, setSearch] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string>("all");
  const [statusFilter, setStatusFilter] = useState<"all" | "open" | "resolved">("open");

  const isBn = i18n.language?.startsWith("bn");

  const filteredIssues = issues.filter((issue) => {
    // Status filter
    if (statusFilter === "open" && issue.status !== "open") return false;
    if (statusFilter === "resolved" && issue.status !== "resolved") return false;

    // Category filter
    if (selectedCategory !== "all" && issue.category !== selectedCategory) return false;

    // Search query
    if (search.trim()) {
      const q = search.toLowerCase();
      const matchTitle = issue.title.toLowerCase().includes(q) || (issue.title_bn && issue.title_bn.toLowerCase().includes(q));
      const matchDesc = issue.description.toLowerCase().includes(q) || (issue.description_bn && issue.description_bn.toLowerCase().includes(q));
      const matchResource = issue.resource.toLowerCase().includes(q);
      return matchTitle || matchDesc || matchResource;
    }

    return true;
  });

  const getSeverityBadge = (severity: SecuritySeverity) => {
    switch (severity) {
      case "critical":
        return (
          <Badge variant="destructive" className="flex items-center gap-1 font-semibold text-xs">
            <AlertOctagon className="h-3 w-3" />
            {t("securityCenter.severityCritical", "Critical")}
          </Badge>
        );
      case "warning":
        return (
          <Badge variant="outline" className="flex items-center gap-1 font-semibold text-xs border-amber-400 text-amber-700 bg-amber-50 dark:bg-amber-950/30 dark:text-amber-300">
            <AlertTriangle className="h-3 w-3" />
            {t("securityCenter.severityWarning", "Warning")}
          </Badge>
        );
      case "info":
      default:
        return (
          <Badge variant="secondary" className="flex items-center gap-1 font-semibold text-xs text-blue-700 bg-blue-50 dark:bg-blue-950/30 dark:text-blue-300">
            <Info className="h-3 w-3" />
            {t("securityCenter.severityInfo", "Advisory")}
          </Badge>
        );
    }
  };

  const categories: { id: string; labelKey: string; defaultLabel: string }[] = [
    { id: "all", labelKey: "common.all", defaultLabel: "All Categories" },
    { id: "auth", labelKey: "securityCenter.catAuth", defaultLabel: "Auth & 2FA" },
    { id: "waf", labelKey: "securityCenter.catWaf", defaultLabel: "WAF Protection" },
    { id: "malware", labelKey: "securityCenter.catMalware", defaultLabel: "Malware & Files" },
    { id: "ssl", labelKey: "securityCenter.catSsl", defaultLabel: "SSL Certificates" },
    { id: "container", labelKey: "securityCenter.catContainer", defaultLabel: "Containers & CVEs" },
    { id: "firewall", labelKey: "securityCenter.catFirewall", defaultLabel: "Firewall & Ports" },
  ];

  return (
    <Card data-testid="security-issues-card" className="border shadow-sm">
      <CardHeader className="pb-3 border-b">
        <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
          <div>
            <CardTitle className="text-lg font-bold flex items-center gap-2">
              <Shield className="h-5 w-5 text-primary" />
              {t("securityCenter.issuesTitle", "Detected Security Issues & Hardening Recommendations")}
            </CardTitle>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t("securityCenter.issuesSubtitle", "Each issue provides an immediate one-click fix or link to configuration.")}
            </p>
          </div>

          {/* Status Tabs */}
          <div className="flex items-center bg-muted/60 p-1 rounded-lg border text-xs">
            <button
              onClick={() => setStatusFilter("open")}
              data-testid="filter-status-open"
              className={`px-3 py-1 rounded-md font-medium transition-all ${
                statusFilter === "open"
                  ? "bg-background text-foreground shadow-xs font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("securityCenter.filterOpen", "Open Issues")} (
              {issues.filter((i) => i.status === "open").length})
            </button>
            <button
              onClick={() => setStatusFilter("resolved")}
              data-testid="filter-status-resolved"
              className={`px-3 py-1 rounded-md font-medium transition-all ${
                statusFilter === "resolved"
                  ? "bg-background text-foreground shadow-xs font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("securityCenter.filterResolved", "Resolved")} (
              {issues.filter((i) => i.status === "resolved").length})
            </button>
            <button
              onClick={() => setStatusFilter("all")}
              data-testid="filter-status-all"
              className={`px-3 py-1 rounded-md font-medium transition-all ${
                statusFilter === "all"
                  ? "bg-background text-foreground shadow-xs font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("common.all", "All")} ({issues.length})
            </button>
          </div>
        </div>

        {/* Filter and Search Bar */}
        <div className="flex flex-col sm:flex-row items-center gap-2 mt-4 pt-2">
          <div className="relative flex-1 w-full">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              type="text"
              placeholder={t("securityCenter.searchPlaceholder", "Search issues by keyword, resource, or IP...")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-8 text-xs"
              data-testid="input-search-issues"
            />
          </div>

          <div className="flex items-center gap-1.5 w-full sm:w-auto overflow-x-auto pb-1 sm:pb-0">
            <Filter className="h-4 w-4 text-muted-foreground hidden sm:block" />
            {categories.map((cat) => (
              <Button
                key={cat.id}
                variant={selectedCategory === cat.id ? "secondary" : "ghost"}
                size="sm"
                onClick={() => setSelectedCategory(cat.id)}
                className={`text-xs whitespace-nowrap h-8 px-2.5 ${
                  selectedCategory === cat.id ? "font-bold" : "text-muted-foreground"
                }`}
                data-testid={`btn-filter-cat-${cat.id}`}
              >
                {t(cat.labelKey, cat.defaultLabel)}
              </Button>
            ))}
          </div>
        </div>
      </CardHeader>

      <CardContent className="p-0">
        {filteredIssues.length === 0 ? (
          <div className="p-8 text-center" data-testid="empty-issues-placeholder">
            <CheckCircle className="h-10 w-10 text-emerald-500 mx-auto mb-2 opacity-80" />
            <p className="font-semibold text-sm">
              {t("securityCenter.noIssuesFound", "No security issues found matching your filter")}
            </p>
            <p className="text-xs text-muted-foreground mt-1">
              {t("securityCenter.systemHardened", "All evaluated security checks are within operational standards.")}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {filteredIssues.map((issue) => {
              const title = isBn && issue.title_bn ? issue.title_bn : issue.title;
              const desc = isBn && issue.description_bn ? issue.description_bn : issue.description;
              const isResolved = issue.status === "resolved";

              return (
                <div
                  key={issue.id}
                  data-testid={`security-issue-row-${issue.id}`}
                  className={`p-4 flex flex-col md:flex-row md:items-center justify-between gap-3 transition-colors ${
                    isResolved ? "opacity-60 bg-muted/20" : "hover:bg-muted/30"
                  }`}
                >
                  <div className="space-y-1.5 flex-1 pr-2">
                    <div className="flex flex-wrap items-center gap-2">
                      {getSeverityBadge(issue.severity)}
                      <span className="text-xs font-mono bg-muted px-2 py-0.5 rounded text-muted-foreground">
                        {issue.category.toUpperCase()}
                      </span>
                      <h4
                        className={`text-sm font-bold ${
                          isResolved ? "line-through text-muted-foreground" : ""
                        }`}
                        data-testid={`issue-title-${issue.id}`}
                      >
                        {title}
                      </h4>
                    </div>

                    <p className="text-xs text-muted-foreground line-clamp-2">{desc}</p>

                    <div className="flex flex-wrap items-center gap-3 text-[11px] text-muted-foreground pt-0.5">
                      <span className="font-mono bg-background border px-1.5 py-0.5 rounded">
                        {issue.resource}
                      </span>
                      <span>•</span>
                      <span>{new Date(issue.timestamp).toLocaleString()}</span>
                      {isResolved && (
                        <>
                          <span>•</span>
                          <span className="text-emerald-600 dark:text-emerald-400 font-semibold flex items-center gap-1">
                            <CheckCircle className="h-3 w-3" />
                            {t("securityCenter.resolvedBadge", "Resolved")}
                          </span>
                        </>
                      )}
                    </div>
                  </div>

                  {/* DoD: "প্রতিটা সমস্যার পাশে 'ঠিক করো' link আছে।" */}
                  <div className="flex items-center gap-2 shrink-0 self-end md:self-center">
                    {issue.status === "open" ? (
                      <Button
                        variant={issue.severity === "critical" ? "destructive" : "default"}
                        size="sm"
                        onClick={() => {
                          if (issue.fix_action.type === "link" && issue.fix_action.target_url) {
                            onNavigateTo(issue.fix_action.target_url);
                          } else {
                            onOpenFix(issue);
                          }
                        }}
                        data-testid={`btn-fix-issue-${issue.id}`}
                        className="flex items-center gap-1.5 h-8 text-xs font-semibold"
                      >
                        {issue.fix_action.type === "link" ? (
                          <>
                            <ExternalLink className="h-3.5 w-3.5" />
                            <span>
                              {t("securityCenter.fixLinkPrefix", "Fix:")}{" "}
                              {t(issue.fix_action.label_key, issue.fix_action.default_label)}
                            </span>
                          </>
                        ) : (
                          <>
                            <Wrench className="h-3.5 w-3.5" />
                            <span>
                              {t("securityCenter.fixLinkPrefix", "Fix:")}{" "}
                              {t(issue.fix_action.label_key, issue.fix_action.default_label)}
                            </span>
                          </>
                        )}
                      </Button>
                    ) : (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled
                        className="h-8 text-xs text-muted-foreground flex items-center gap-1.5"
                      >
                        <CheckCircle className="h-3.5 w-3.5 text-emerald-500" />
                        {t("securityCenter.alreadyFixed", "Remediated")}
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
