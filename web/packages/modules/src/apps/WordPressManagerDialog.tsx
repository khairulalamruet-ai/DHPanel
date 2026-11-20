import React, { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  Button,
  Badge,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@builtpanel/ui";
import { useTranslation } from "@builtpanel/i18n";
import {
  Globe,
  Settings,
  Puzzle,
  Palette,
  ShieldCheck,
  RefreshCw,
  ArrowUpCircle,
  ToggleLeft,
  ToggleRight,
  ExternalLink,
  Copy,
  CheckCircle,
  KeyRound,
  AlertCircle,
} from "lucide-react";
import { InstalledApp, WordPressDetails, WordPressPlugin } from "./types";

interface WordPressManagerDialogProps {
  open: boolean;
  app: InstalledApp | null;
  details: WordPressDetails | null;
  onClose: () => void;
  onUpdatePlugin: (appId: string, pluginSlug: string) => Promise<void>;
  onTogglePlugin: (appId: string, pluginSlug: string) => Promise<void>;
  onToggleMaintenance: (appId: string, enabled: boolean) => Promise<void>;
  onCloneStaging: (appId: string) => Promise<void>;
}

export function WordPressManagerDialog({
  open,
  app,
  details,
  onClose,
  onUpdatePlugin,
  onTogglePlugin,
  onToggleMaintenance,
  onCloneStaging,
}: WordPressManagerDialogProps) {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"overview" | "plugins" | "themes" | "security">("overview");
  const [updatingPlugins, setUpdatingPlugins] = useState<Record<string, boolean>>({});
  const [isCloning, setIsCloning] = useState(false);
  const [isTogglingMaint, setIsTogglingMaint] = useState(false);

  if (!app) return null;

  const handlePluginUpdate = async (slug: string) => {
    setUpdatingPlugins((prev) => ({ ...prev, [slug]: true }));
    try {
      await onUpdatePlugin(app.id, slug);
    } finally {
      setUpdatingPlugins((prev) => ({ ...prev, [slug]: false }));
    }
  };

  const handleMaintenanceToggle = async () => {
    if (!details) return;
    setIsTogglingMaint(true);
    try {
      await onToggleMaintenance(app.id, !details.maintenance_mode);
    } finally {
      setIsTogglingMaint(false);
    }
  };

  const handleStagingClone = async () => {
    setIsCloning(true);
    try {
      await onCloneStaging(app.id);
    } finally {
      setIsCloning(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(val) => !val && onClose()}>
      <DialogContent data-testid="wp-manager-dialog" className="sm:max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader className="border-b pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="p-2.5 rounded-xl bg-blue-50 dark:bg-blue-950/40 text-blue-600 border border-blue-200">
                <Globe className="h-6 w-6" />
              </div>
              <div>
                <DialogTitle data-testid="wp-manager-title" className="text-lg flex items-center gap-2">
                  <span>WordPress Manager</span>
                  <Badge variant="outline" className="text-xs font-mono">
                    v{details?.core_version || app.version}
                  </Badge>
                </DialogTitle>
                <DialogDescription className="text-xs font-mono">
                  {app.url}
                </DialogDescription>
              </div>
            </div>

            {/* Quick Admin Access */}
            {app.admin_url && (
              <a
                href={app.admin_url}
                target="_blank"
                rel="noopener noreferrer"
                data-testid="btn-wp-auto-login"
                className="h-8 px-2.5 inline-flex items-center justify-center rounded-md text-xs font-semibold bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
              >
                <KeyRound className="h-3.5 w-3.5 mr-1" />
                {t("wpManager.autoLogin", "Auto-Login")}
              </a>
            )}
          </div>

          {/* Navigation Tabs */}
          <div className="flex items-center gap-2 mt-4 text-xs">
            <button
              onClick={() => setActiveTab("overview")}
              data-testid="tab-wp-overview"
              className={`px-3 py-1.5 rounded-md font-semibold transition-all ${
                activeTab === "overview"
                  ? "bg-primary text-primary-foreground shadow-xs"
                  : "text-muted-foreground hover:bg-muted"
              }`}
            >
              <Settings className="h-3.5 w-3.5 inline mr-1.5" />
              {t("wpManager.tabOverview", "Overview")}
            </button>
            <button
              onClick={() => setActiveTab("plugins")}
              data-testid="tab-wp-plugins"
              className={`px-3 py-1.5 rounded-md font-semibold transition-all ${
                activeTab === "plugins"
                  ? "bg-primary text-primary-foreground shadow-xs"
                  : "text-muted-foreground hover:bg-muted"
              }`}
            >
              <Puzzle className="h-3.5 w-3.5 inline mr-1.5" />
              {t("wpManager.tabPlugins", "Plugins")}
              {details && details.plugins.some((p) => p.has_update) && (
                <span className="ml-1.5 bg-amber-500 text-white px-1.5 py-0.2 rounded-full text-[10px]">
                  {details.plugins.filter((p) => p.has_update).length}
                </span>
              )}
            </button>
            <button
              onClick={() => setActiveTab("themes")}
              data-testid="tab-wp-themes"
              className={`px-3 py-1.5 rounded-md font-semibold transition-all ${
                activeTab === "themes"
                  ? "bg-primary text-primary-foreground shadow-xs"
                  : "text-muted-foreground hover:bg-muted"
              }`}
            >
              <Palette className="h-3.5 w-3.5 inline mr-1.5" />
              {t("wpManager.tabThemes", "Themes")}
            </button>
            <button
              onClick={() => setActiveTab("security")}
              data-testid="tab-wp-security"
              className={`px-3 py-1.5 rounded-md font-semibold transition-all ${
                activeTab === "security"
                  ? "bg-primary text-primary-foreground shadow-xs"
                  : "text-muted-foreground hover:bg-muted"
              }`}
            >
              <ShieldCheck className="h-3.5 w-3.5 inline mr-1.5" />
              {t("wpManager.tabSecurity", "Hardening")}
            </button>
          </div>
        </DialogHeader>

        {/* Tab Content */}
        <div className="flex-1 overflow-y-auto py-4 space-y-4">
          {activeTab === "overview" && (
            <div className="space-y-4" data-testid="wp-overview-content">
              {/* Core Information Card */}
              <div className="grid grid-cols-2 gap-3">
                <div className="p-3 bg-card border rounded-lg space-y-1">
                  <span className="text-xs text-muted-foreground">{t("wpManager.coreVersion", "WordPress Core")}</span>
                  <div className="flex items-center gap-2">
                    <span className="font-mono font-bold text-sm">v{details?.core_version || app.version}</span>
                    <Badge variant="outline" className="text-[10px] bg-emerald-50 text-emerald-700 border-emerald-300">
                      Up to date
                    </Badge>
                  </div>
                </div>

                <div className="p-3 bg-card border rounded-lg space-y-1">
                  <span className="text-xs text-muted-foreground">{t("wpManager.maintenanceStatus", "Maintenance Mode")}</span>
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-xs">
                      {details?.maintenance_mode ? t("wpManager.modeActive", "Active") : t("wpManager.modeDisabled", "Disabled")}
                    </span>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={handleMaintenanceToggle}
                      disabled={isTogglingMaint}
                      data-testid="btn-toggle-maintenance"
                      className="h-7 text-xs"
                    >
                      {details?.maintenance_mode ? t("wpManager.disableMode", "Deactivate") : t("wpManager.enableMode", "Activate")}
                    </Button>
                  </div>
                </div>
              </div>

              {/* Staging Clone Feature (T12.03) */}
              <div className="p-4 bg-muted/40 border rounded-lg flex items-center justify-between">
                <div>
                  <h4 className="font-bold text-sm flex items-center gap-1.5">
                    <Copy className="h-4 w-4 text-primary" />
                    {t("wpManager.stagingTitle", "Staging Environment")}
                  </h4>
                  <p className="text-xs text-muted-foreground mt-0.5">
                    {details?.staging_url
                      ? t("wpManager.stagingActiveNotice", "Staging environment is ready at {{url}}", { url: details.staging_url })
                      : t("wpManager.stagingDesc", "Create a 1-click clone to safely test plugin updates or design changes.")}
                  </p>
                </div>
                {details?.staging_url ? (
                  <a
                    href={details.staging_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="h-8 px-2.5 inline-flex items-center justify-center rounded-md text-xs font-semibold border border-input bg-background hover:bg-accent transition-colors"
                  >
                    <ExternalLink className="h-3.5 w-3.5 mr-1" />
                    {t("wpManager.visitStaging", "Visit Staging")}
                  </a>
                ) : (
                  <Button
                    size="sm"
                    onClick={handleStagingClone}
                    disabled={isCloning}
                    data-testid="btn-create-staging"
                    className="h-8 text-xs font-semibold"
                  >
                    {isCloning ? t("wpManager.cloning", "Cloning...") : t("wpManager.createStaging", "Create Staging Clone")}
                  </Button>
                )}
              </div>
            </div>
          )}

          {/* Plugins Tab (T12.03 DoD: "Plugin update UI থেকে হয়।") */}
          {activeTab === "plugins" && (
            <div className="space-y-3" data-testid="wp-plugins-content">
              <div className="flex items-center justify-between">
                <div>
                  <h4 className="font-bold text-sm">{t("wpManager.installedPlugins", "Installed Plugins")}</h4>
                  <p className="text-xs text-muted-foreground">
                    {t("wpManager.pluginsSubtitle", "Manage and update plugins inside your tenant environment.")}
                  </p>
                </div>
              </div>

              <div className="rounded-lg border overflow-hidden">
                <Table data-testid="wp-plugins-table">
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t("wpManager.pluginName", "Plugin")}</TableHead>
                      <TableHead>{t("wpManager.pluginVersion", "Version")}</TableHead>
                      <TableHead>{t("wpManager.pluginStatus", "Status")}</TableHead>
                      <TableHead className="text-right">{t("common.actions", "Actions")}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {details?.plugins.map((plugin) => (
                      <TableRow key={plugin.slug} data-testid={`plugin-row-${plugin.slug}`}>
                        <TableCell>
                          <div>
                            <span className="font-semibold text-xs block">{plugin.name}</span>
                            <span className="text-[10px] text-muted-foreground font-mono">
                              by {plugin.author}
                            </span>
                          </div>
                        </TableCell>

                        <TableCell>
                          <div className="flex items-center gap-1.5">
                            <span className="font-mono text-xs">v{plugin.version}</span>
                            {plugin.has_update && (
                              <Badge
                                variant="outline"
                                className="text-[10px] bg-amber-50 text-amber-700 border-amber-300"
                                data-testid={`badge-update-${plugin.slug}`}
                              >
                                v{plugin.latest_version} available
                              </Badge>
                            )}
                          </div>
                        </TableCell>

                        <TableCell>
                          <Badge
                            variant={plugin.active ? "secondary" : "outline"}
                            className="text-xs font-medium cursor-pointer"
                            onClick={() => onTogglePlugin(app.id, plugin.slug)}
                            data-testid={`badge-plugin-status-${plugin.slug}`}
                          >
                            {plugin.active ? t("wpManager.active", "Active") : t("wpManager.inactive", "Inactive")}
                          </Badge>
                        </TableCell>

                        {/* DoD: "Plugin update UI থেকে হয়।" */}
                        <TableCell className="text-right">
                          <div className="flex items-center justify-end gap-1.5">
                            {plugin.has_update ? (
                              <Button
                                size="sm"
                                variant="default"
                                onClick={() => handlePluginUpdate(plugin.slug)}
                                disabled={updatingPlugins[plugin.slug]}
                                data-testid={`btn-update-plugin-${plugin.slug}`}
                                className="h-7 text-xs font-semibold bg-amber-600 hover:bg-amber-700 text-white"
                              >
                                <RefreshCw className={`h-3 w-3 mr-1 ${updatingPlugins[plugin.slug] ? "animate-spin" : ""}`} />
                                {updatingPlugins[plugin.slug]
                                  ? t("wpManager.updating", "Updating...")
                                  : t("wpManager.updatePlugin", "Update")}
                              </Button>
                            ) : (
                              <Badge variant="outline" className="text-[10px] text-muted-foreground">
                                <CheckCircle className="h-3 w-3 mr-1 text-emerald-500 inline" />
                                {t("wpManager.latestVersion", "Latest")}
                              </Badge>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>
            </div>
          )}

          {/* Themes Tab */}
          {activeTab === "themes" && (
            <div className="space-y-3" data-testid="wp-themes-content">
              <h4 className="font-bold text-sm">{t("wpManager.installedThemes", "Installed Themes")}</h4>
              <div className="grid grid-cols-2 gap-3">
                {details?.themes.map((theme) => (
                  <div key={theme.slug} className="p-3 border rounded-lg bg-card flex items-center justify-between">
                    <div>
                      <span className="font-bold text-xs block">{theme.name}</span>
                      <span className="text-[10px] text-muted-foreground font-mono">v{theme.version}</span>
                    </div>
                    {theme.active && (
                      <Badge variant="secondary" className="text-xs">
                        {t("wpManager.activeTheme", "Active Theme")}
                      </Badge>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Security & Hardening Tab (T12.04) */}
          {activeTab === "security" && (
            <div className="space-y-3" data-testid="wp-security-content">
              <h4 className="font-bold text-sm">{t("wpManager.hardeningOptions", "WordPress Security Hardening")}</h4>
              <div className="space-y-2">
                <div className="p-3 border rounded-lg bg-card flex items-center justify-between">
                  <div>
                    <span className="font-semibold text-xs block">{t("wpManager.xmlrpcTitle", "Disable XML-RPC")}</span>
                    <span className="text-[11px] text-muted-foreground">
                      {t("wpManager.xmlrpcDesc", "Protects against brute-force amplification and DDoS targeting xmlrpc.php.")}
                    </span>
                  </div>
                  <Badge variant="outline" className="bg-emerald-50 text-emerald-700 border-emerald-300 text-xs">
                    Protected
                  </Badge>
                </div>

                <div className="p-3 border rounded-lg bg-card flex items-center justify-between">
                  <div>
                    <span className="font-semibold text-xs block">{t("wpManager.debugModeTitle", "WP_DEBUG Disabled")}</span>
                    <span className="text-[11px] text-muted-foreground">
                      {t("wpManager.debugModeDesc", "Prevents database queries and sensitive stack traces from leaking on frontend.")}
                    </span>
                  </div>
                  <Badge variant="outline" className="bg-emerald-50 text-emerald-700 border-emerald-300 text-xs">
                    Disabled (Safe)
                  </Badge>
                </div>
              </div>
            </div>
          )}
        </div>

        <DialogFooter className="border-t pt-3">
          <Button variant="outline" onClick={onClose}>
            {t("common.close", "Close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
