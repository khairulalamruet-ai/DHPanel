import React, { useState, useEffect, useCallback } from "react";
import { useTranslation } from "@builtpanel/i18n";
import { apiFetch } from "@builtpanel/api";
import { useToast, Card, CardHeader, CardTitle, CardDescription, CardContent, Button, Input, Badge } from "@builtpanel/ui";
import {
  Bell,
  Mail,
  Webhook,
  ShieldAlert,
  Archive,
  Lock,
  Server,
  Clock,
  Send,
  Save,
  CheckCircle2,
  AlertCircle,
  PauseCircle,
  PlayCircle,
} from "lucide-react";
import { NotificationChannelConfig } from "./types";

export function NotificationPreferencesPage() {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [config, setConfig] = useState<NotificationChannelConfig>({
    email_enabled: true,
    email_recipients: ["admin@enterprise.io"],
    webhook_enabled: false,
    webhook_url: "",
    in_app_enabled: true,
    notify_on_ssl_expiry: true,
    notify_on_backup_failure: true,
    notify_on_node_offline: true,
    notify_on_security_alert: true,
    snooze_until: null,
  });

  const [recipientInput, setRecipientInput] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [isTesting, setIsTesting] = useState(false);

  const loadSettings = useCallback(async () => {
    try {
      setIsLoading(true);
      const res = await apiFetch<NotificationChannelConfig>("/api/v1/notifications/settings");
      setConfig(res);
      setRecipientInput(res.email_recipients.join(", "));
    } catch {
      // Use defaults
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  const handleSave = async (e?: React.FormEvent) => {
    e?.preventDefault();
    setIsSaving(true);
    try {
      const recipients = recipientInput
        .split(",")
        .map((s) => s.trim().toLowerCase())
        .filter(Boolean);

      const payload: NotificationChannelConfig = {
        ...config,
        email_recipients: recipients,
      };

      const updated = await apiFetch<NotificationChannelConfig>("/api/v1/notifications/settings", {
        method: "PUT",
        body: JSON.stringify(payload),
      });

      setConfig(updated);
      toast({
        title: t("notifications.savedTitle", "Preferences Saved"),
        description: t("notifications.savedDesc", "Notification channels and alert triggers updated."),
        variant: "success",
      });
    } catch (err: any) {
      toast({
        title: t("common.error", "Error"),
        description: err.message || t("notifications.saveError", "Failed to save notification settings."),
        variant: "destructive",
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleTestNotification = async () => {
    setIsTesting(true);
    try {
      await apiFetch("/api/v1/notifications/test", { method: "POST" });
      toast({
        title: t("notifications.testSentTitle", "Test Notification Dispatched"),
        description: t("notifications.testSentDesc", "A sample notification was sent to your active channels."),
        variant: "success",
      });
    } catch (err: any) {
      toast({
        title: t("common.error", "Error"),
        description: err.message || t("notifications.testFailed", "Failed to send test notification."),
        variant: "destructive",
      });
    } finally {
      setIsTesting(false);
    }
  };

  const handleSnooze = async (durationHours: number) => {
    try {
      const res = await apiFetch<{ snooze_until: string }>("/api/v1/notifications/snooze", {
        method: "POST",
        body: JSON.stringify({ duration_hours: durationHours }),
      });
      setConfig((prev) => ({ ...prev, snooze_until: res.snooze_until }));
      toast({
        title: t("notifications.snoozedTitle", "Notifications Snoozed"),
        description: t("notifications.snoozedDesc", "Alerts paused until {{time}}.", {
          time: new Date(res.snooze_until).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
        }),
        variant: "success",
      });
    } catch {
      // Revert if error
    }
  };

  const handleClearSnooze = async () => {
    try {
      await apiFetch("/api/v1/notifications/snooze", {
        method: "POST",
        body: JSON.stringify({ duration_hours: 0 }),
      });
      setConfig((prev) => ({ ...prev, snooze_until: null }));
      toast({
        title: t("notifications.snoozeClearedTitle", "Snooze Cleared"),
        description: t("notifications.snoozeClearedDesc", "Live notification alerts resumed."),
        variant: "success",
      });
    } catch {
      // Revert
    }
  };

  const isSnoozed = Boolean(config.snooze_until && new Date(config.snooze_until) > new Date());

  return (
    <div className="space-y-6" data-testid="notification-preferences-page">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight" data-testid="notification-settings-title">
            {t("notifications.settingsPageTitle", "Notification Channels & Alert Rules")}
          </h1>
          <p className="text-muted-foreground text-sm">
            {t(
              "notifications.settingsPageSubtitle",
              "Configure dispatch channels, critical incident triggers, and alert pause policies."
            )}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleTestNotification}
            disabled={isTesting}
            className="gap-2 shrink-0 text-xs h-9"
            data-testid="btn-test-notification"
          >
            <Send className={`h-3.5 w-3.5 ${isTesting ? "animate-spin" : ""}`} />
            {t("notifications.sendTestAlert", "Send Test Alert")}
          </Button>

          <Button
            onClick={() => handleSave()}
            disabled={isSaving}
            className="gap-2 shrink-0 text-xs h-9"
            data-testid="btn-save-notification-settings"
          >
            <Save className="h-3.5 w-3.5" />
            {isSaving ? t("common.saving", "Saving...") : t("common.saveChanges", "Save Changes")}
          </Button>
        </div>
      </div>

      {/* Snooze / Alert Pause Banner */}
      <Card
        className={`border shadow-xs ${
          isSnoozed ? "border-amber-500/40 bg-amber-500/10" : "bg-card"
        }`}
        data-testid="snooze-control-card"
      >
        <CardContent className="p-4 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className={`p-2.5 rounded-lg shrink-0 ${isSnoozed ? "bg-amber-500/20 text-amber-600 dark:text-amber-400" : "bg-primary/10 text-primary"}`}>
              {isSnoozed ? <PauseCircle className="h-5 w-5" /> : <Clock className="h-5 w-5" />}
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-semibold text-sm">
                  {t("notifications.snoozeHeading", "Alert Snooze & Maintenance Window")}
                </span>
                {isSnoozed && (
                  <Badge variant="outline" className="border-amber-500 text-amber-600 font-mono text-[10px]">
                    Snoozed until {new Date(config.snooze_until!).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                  </Badge>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                {t(
                  "notifications.snoozeSubheading",
                  "Temporarily pause all outbound notifications during scheduled system maintenance."
                )}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0 self-end sm:self-center">
            {isSnoozed ? (
              <Button
                variant="outline"
                size="sm"
                onClick={handleClearSnooze}
                className="text-xs h-8 gap-1.5"
                data-testid="btn-clear-snooze"
              >
                <PlayCircle className="h-3.5 w-3.5 text-emerald-500" />
                {t("notifications.resumeAlerts", "Resume Alerts")}
              </Button>
            ) : (
              <div className="flex items-center gap-1.5">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleSnooze(1)}
                  className="text-xs h-8"
                  data-testid="btn-snooze-1h"
                >
                  {t("notifications.snooze1h", "1 Hour")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleSnooze(8)}
                  className="text-xs h-8"
                  data-testid="btn-snooze-8h"
                >
                  {t("notifications.snooze8h", "8 Hours")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => handleSnooze(24)}
                  className="text-xs h-8"
                  data-testid="btn-snooze-24h"
                >
                  {t("notifications.snooze24h", "24 Hours")}
                </Button>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* 1. Dispatch Channels */}
        <Card className="border shadow-xs" data-testid="notification-channels-card">
          <CardHeader className="pb-3">
            <CardTitle className="text-base font-bold flex items-center gap-2">
              <Bell className="h-4 w-4 text-primary" />
              <span>{t("notifications.channelsTitle", "Dispatch Channels")}</span>
            </CardTitle>
            <CardDescription className="text-xs">
              {t(
                "notifications.channelsSubtitle",
                "Select where alerts and notifications should be transmitted."
              )}
            </CardDescription>
          </CardHeader>

          <CardContent className="space-y-4">
            {/* In-App Bell */}
            <div className="p-3.5 rounded-lg border bg-muted/15 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <Bell className="h-5 w-5 text-primary mt-0.5 shrink-0" />
                <div className="space-y-0.5">
                  <span className="text-xs font-bold block">
                    {t("notifications.inAppChannel", "In-App Notification Bell")}
                  </span>
                  <p className="text-[11px] text-muted-foreground">
                    {t("notifications.inAppDesc", "Display real-time alerts in the top panel navigation bar.")}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={config.in_app_enabled}
                onChange={(e) => setConfig({ ...config, in_app_enabled: e.target.checked })}
                className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                data-testid="checkbox-inapp-channel"
              />
            </div>

            {/* Email Channel */}
            <div className="p-3.5 rounded-lg border bg-muted/15 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-start gap-3">
                  <Mail className="h-5 w-5 text-primary mt-0.5 shrink-0" />
                  <div className="space-y-0.5">
                    <span className="text-xs font-bold block">
                      {t("notifications.emailChannel", "Email Notifications (SMTP)")}
                    </span>
                    <p className="text-[11px] text-muted-foreground">
                      {t("notifications.emailDesc", "Send formatted HTML notification alerts to administrator inboxes.")}
                    </p>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={config.email_enabled}
                  onChange={(e) => setConfig({ ...config, email_enabled: e.target.checked })}
                  className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                  data-testid="checkbox-email-channel"
                />
              </div>

              {config.email_enabled && (
                <div className="pt-2 border-t">
                  <label className="text-[11px] font-semibold block mb-1">
                    {t("notifications.emailRecipientsLabel", "Recipient Email Addresses (comma separated)")}
                  </label>
                  <Input
                    value={recipientInput}
                    onChange={(e) => setRecipientInput(e.target.value)}
                    placeholder="admin@enterprise.io, devops@enterprise.io"
                    className="text-xs font-mono"
                    data-testid="input-email-recipients"
                  />
                </div>
              )}
            </div>

            {/* Webhook Channel */}
            <div className="p-3.5 rounded-lg border bg-muted/15 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-start gap-3">
                  <Webhook className="h-5 w-5 text-primary mt-0.5 shrink-0" />
                  <div className="space-y-0.5">
                    <span className="text-xs font-bold block">
                      {t("notifications.webhookChannel", "Outbound Webhooks")}
                    </span>
                    <p className="text-[11px] text-muted-foreground">
                      {t("notifications.webhookDesc", "Post JSON alert payloads to Slack, Discord, or monitoring endpoints.")}
                    </p>
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={config.webhook_enabled}
                  onChange={(e) => setConfig({ ...config, webhook_enabled: e.target.checked })}
                  className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                  data-testid="checkbox-webhook-channel"
                />
              </div>

              {config.webhook_enabled && (
                <div className="pt-2 border-t">
                  <label className="text-[11px] font-semibold block mb-1">
                    {t("notifications.webhookUrlLabel", "Payload Destination URL")}
                  </label>
                  <Input
                    value={config.webhook_url || ""}
                    onChange={(e) => setConfig({ ...config, webhook_url: e.target.value })}
                    placeholder="https://hooks.slack.com/services/..."
                    className="text-xs font-mono"
                    data-testid="input-webhook-url"
                  />
                </div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* 2. Critical Alert Triggers (T18.03 DoD: SSL, Backup, Node Offline) */}
        <Card className="border shadow-xs" data-testid="notification-triggers-card">
          <CardHeader className="pb-3">
            <CardTitle className="text-base font-bold flex items-center gap-2">
              <ShieldAlert className="h-4 w-4 text-primary" />
              <span>{t("notifications.triggersTitle", "Critical Alert Triggers")}</span>
            </CardTitle>
            <CardDescription className="text-xs">
              {t(
                "notifications.triggersSubtitle",
                "Automated events that generate urgent warnings and administrator notifications."
              )}
            </CardDescription>
          </CardHeader>

          <CardContent className="space-y-3">
            {/* SSL Expiry */}
            <div className="p-3.5 rounded-lg border bg-muted/15 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <Lock className="h-4 w-4 text-primary mt-0.5 shrink-0" />
                <div className="space-y-0.5">
                  <span className="text-xs font-bold block">
                    {t("notifications.triggerSsl", "SSL Certificate Expiry Warning")}
                  </span>
                  <p className="text-[11px] text-muted-foreground">
                    {t("notifications.triggerSslDesc", "Alert 30 days and 7 days prior to Let's Encrypt / custom certificate expiration.")}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={config.notify_on_ssl_expiry}
                onChange={(e) => setConfig({ ...config, notify_on_ssl_expiry: e.target.checked })}
                className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                data-testid="checkbox-trigger-ssl"
              />
            </div>

            {/* Backup Failure */}
            <div className="p-3.5 rounded-lg border bg-muted/15 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <Archive className="h-4 w-4 text-emerald-500 mt-0.5 shrink-0" />
                <div className="space-y-0.5">
                  <span className="text-xs font-bold block">
                    {t("notifications.triggerBackup", "Backup Failure & Storage Errors")}
                  </span>
                  <p className="text-[11px] text-muted-foreground">
                    {t("notifications.triggerBackupDesc", "Immediate notification if scheduled snapshot or S3 sync encounters errors.")}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={config.notify_on_backup_failure}
                onChange={(e) => setConfig({ ...config, notify_on_backup_failure: e.target.checked })}
                className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                data-testid="checkbox-trigger-backup"
              />
            </div>

            {/* Node Offline */}
            <div className="p-3.5 rounded-lg border bg-muted/15 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <Server className="h-4 w-4 text-destructive mt-0.5 shrink-0" />
                <div className="space-y-0.5">
                  <span className="text-xs font-bold block">
                    {t("notifications.triggerNodeOffline", "Worker Node Disconnection / Offline")}
                  </span>
                  <p className="text-[11px] text-muted-foreground">
                    {t("notifications.triggerNodeOfflineDesc", "Trigger alert within 60 seconds if a cluster agent drops connection.")}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={config.notify_on_node_offline}
                onChange={(e) => setConfig({ ...config, notify_on_node_offline: e.target.checked })}
                className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                data-testid="checkbox-trigger-node-offline"
              />
            </div>

            {/* Security Alerts */}
            <div className="p-3.5 rounded-lg border bg-muted/15 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <ShieldAlert className="h-4 w-4 text-amber-500 mt-0.5 shrink-0" />
                <div className="space-y-0.5">
                  <span className="text-xs font-bold block">
                    {t("notifications.triggerSecurity", "Security & Brute-Force Alerts")}
                  </span>
                  <p className="text-[11px] text-muted-foreground">
                    {t("notifications.triggerSecurityDesc", "Coraza WAF anomalies, SSH jail triggers, and repeated login failures.")}
                  </p>
                </div>
              </div>
              <input
                type="checkbox"
                checked={config.notify_on_security_alert}
                onChange={(e) => setConfig({ ...config, notify_on_security_alert: e.target.checked })}
                className="rounded border-gray-300 text-primary focus:ring-primary h-4 w-4 mt-1"
                data-testid="checkbox-trigger-security"
              />
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
