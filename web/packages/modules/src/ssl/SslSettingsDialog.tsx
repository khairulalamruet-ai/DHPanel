import React, { useState, useEffect } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
} from "@builtpanel/ui";
import { apiFetch } from "@builtpanel/api";
import { useTranslation } from "@builtpanel/i18n";
import { Shield, ShieldAlert, Lock, RefreshCw, AlertCircle } from "lucide-react";
import type { SslCertificateItem, SslSettingsPayload } from "./types";

interface SslSettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  certificate: SslCertificateItem | null;
  onSuccess: (updatedCert: SslCertificateItem) => void;
}

export function SslSettingsDialog({
  open,
  onOpenChange,
  certificate,
  onSuccess,
}: SslSettingsDialogProps) {
  const { t } = useTranslation();

  const [forceHttps, setForceHttps] = useState(false);
  const [hstsEnabled, setHstsEnabled] = useState(false);
  const [hstsSubdomains, setHstsSubdomains] = useState(false);
  const [hstsPreload, setHstsPreload] = useState(false);
  const [autoRenew, setAutoRenew] = useState(true);

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (certificate) {
      setForceHttps(certificate.force_https);
      setHstsEnabled(certificate.hsts_enabled);
      setHstsSubdomains(certificate.hsts_subdomains);
      setHstsPreload(certificate.hsts_preload);
      setAutoRenew(certificate.auto_renew);
      setError(null);
    }
  }, [certificate]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!certificate) return;
    setError(null);

    try {
      setIsSubmitting(true);
      const payload: SslSettingsPayload = {
        force_https: forceHttps,
        hsts_enabled: hstsEnabled,
        hsts_subdomains: hstsSubdomains,
        hsts_preload: hstsPreload,
        auto_renew: autoRenew,
      };

      const updated = await apiFetch<SslCertificateItem>(
        `/api/v1/client/ssl/${certificate.domain_id}/settings`,
        {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        }
      );

      onSuccess(updated);
      onOpenChange(false);
    } catch (err: any) {
      setError(err?.message || t("ssl.errors.updateSettingsFailed", "Failed to update security settings"));
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!certificate) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg" data-testid="dialog-ssl-settings">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Shield className="h-5 w-5 text-primary" />
            {t("ssl.settingsDialog.title", "HTTPS & Security Headers")}
          </DialogTitle>
        </DialogHeader>

        <p className="text-xs text-muted-foreground">
          {t(
            "ssl.settingsDialog.description",
            "Configure traffic encryption rules and browser transport security for {{domain}}.",
            { domain: certificate.domain_name }
          )}
        </p>

        {error && (
          <div className="p-3 text-xs bg-destructive/10 border border-destructive/20 text-destructive rounded-lg flex items-center gap-2">
            <AlertCircle className="h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4 py-2 text-sm">
          {/* Force HTTPS Toggle */}
          <div className="p-3 rounded-lg border bg-muted/20 space-y-2">
            <div className="flex items-center justify-between">
              <label htmlFor="toggle-force-https" className="cursor-pointer font-semibold text-xs flex items-center gap-2">
                <Lock className="h-4 w-4 text-emerald-500" />
                {t("ssl.settings.forceHttps", "Force HTTPS Redirection")}
              </label>
              <input
                type="checkbox"
                id="toggle-force-https"
                checked={forceHttps}
                onChange={(e) => setForceHttps(e.target.checked)}
                className="rounded border-input text-primary focus:ring-primary h-4 w-4"
                data-testid="checkbox-force-https"
              />
            </div>
            <p className="text-[11px] text-muted-foreground">
              {t(
                "ssl.settings.forceHttpsDesc",
                "Automatically redirect all insecure HTTP requests (port 80) to encrypted HTTPS (port 443) with a permanent 301 redirect."
              )}
            </p>
          </div>

          {/* HSTS Options */}
          <div className="p-3 rounded-lg border bg-muted/20 space-y-3">
            <div className="flex items-center justify-between">
              <label htmlFor="toggle-hsts" className="cursor-pointer font-semibold text-xs flex items-center gap-2">
                <ShieldAlert className="h-4 w-4 text-primary" />
                {t("ssl.settings.hsts", "HTTP Strict Transport Security (HSTS)")}
              </label>
              <input
                type="checkbox"
                id="toggle-hsts"
                checked={hstsEnabled}
                onChange={(e) => setHstsEnabled(e.target.checked)}
                className="rounded border-input text-primary focus:ring-primary h-4 w-4"
                data-testid="checkbox-hsts"
              />
            </div>
            <p className="text-[11px] text-muted-foreground">
              {t(
                "ssl.settings.hstsDesc",
                "Instructs browsers to strictly refuse any plain HTTP connection for 1 year (Strict-Transport-Security: max-age=31536000)."
              )}
            </p>

            {hstsEnabled && (
              <div className="pl-6 space-y-2 pt-2 border-t text-xs">
                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="toggle-hsts-subdomains"
                    checked={hstsSubdomains}
                    onChange={(e) => setHstsSubdomains(e.target.checked)}
                    className="rounded border-input text-primary focus:ring-primary h-3.5 w-3.5"
                    data-testid="checkbox-hsts-subdomains"
                  />
                  <label htmlFor="toggle-hsts-subdomains" className="cursor-pointer text-xs">
                    {t("ssl.settings.includeSubdomains", "Include all subdomains (includeSubDomains)")}
                  </label>
                </div>

                <div className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="toggle-hsts-preload"
                    checked={hstsPreload}
                    onChange={(e) => setHstsPreload(e.target.checked)}
                    className="rounded border-input text-primary focus:ring-primary h-3.5 w-3.5"
                    data-testid="checkbox-hsts-preload"
                  />
                  <label htmlFor="toggle-hsts-preload" className="cursor-pointer text-xs">
                    {t("ssl.settings.preload", "Enable browser HSTS preload list eligibility (preload)")}
                  </label>
                </div>
              </div>
            )}
          </div>

          {/* Auto-renew */}
          <div className="p-3 rounded-lg border bg-muted/20 space-y-2">
            <div className="flex items-center justify-between">
              <label htmlFor="toggle-auto-renew" className="cursor-pointer font-semibold text-xs flex items-center gap-2">
                <RefreshCw className="h-4 w-4 text-blue-500" />
                {t("ssl.settings.autoRenew", "Automatic Certificate Renewal")}
              </label>
              <input
                type="checkbox"
                id="toggle-auto-renew"
                checked={autoRenew}
                onChange={(e) => setAutoRenew(e.target.checked)}
                className="rounded border-input text-primary focus:ring-primary h-4 w-4"
                data-testid="checkbox-auto-renew"
              />
            </div>
            <p className="text-[11px] text-muted-foreground">
              {t(
                "ssl.settings.autoRenewDesc",
                "Automatically trigger background ACME renewal 30 days prior to certificate expiration."
              )}
            </p>
          </div>

          <DialogFooter className="pt-2">
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={isSubmitting}>
              {t("common.cancel", "Cancel")}
            </Button>
            <Button type="submit" disabled={isSubmitting} data-testid="btn-save-ssl-settings">
              {isSubmitting ? t("common.processing", "Saving...") : t("common.saveChanges", "Save Settings")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
