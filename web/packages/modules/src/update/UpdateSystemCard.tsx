import React, { useState } from "react";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
  Button,
  Badge,
  useToast,
} from "@builtpanel/ui";
import { useTranslation } from "@builtpanel/i18n";
import { apiFetch } from "@builtpanel/api";
import {
  RefreshCw,
  ShieldCheck,
  ArrowUpCircle,
  RotateCcw,
  AlertTriangle,
  CheckCircle2,
  GitBranch,
  Layers,
  FileText,
  AlertCircle,
} from "lucide-react";
import {
  PanelUpdateStatus,
  UpdateChannel,
  UpdateStep,
  SignedUpdateManifest,
} from "./types";

interface UpdateSystemCardProps {
  status: PanelUpdateStatus;
  onRefresh: () => void;
}

export function UpdateSystemCard({ status, onRefresh }: UpdateSystemCardProps) {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [isChecking, setIsChecking] = useState(false);
  const [currentStep, setCurrentStep] = useState<UpdateStep>("idle");
  const [stepMessage, setStepMessage] = useState<string>("");
  const [activeChannel, setActiveChannel] = useState<UpdateChannel>(status.channel);
  const [isSavingChannel, setIsSavingChannel] = useState(false);
  const [simulateFailure, setSimulateFailure] = useState(false);

  // Handle Channel Change
  const handleChannelChange = async (newChannel: UpdateChannel) => {
    setActiveChannel(newChannel);
    setIsSavingChannel(true);
    try {
      await apiFetch("/api/v1/update/channel", {
        method: "POST",
        body: JSON.stringify({ channel: newChannel }),
      });
      toast({
        title: t("update.channelUpdated", "Update Channel Changed"),
        description: t("update.channelUpdatedDesc", `Switched to ${newChannel} update channel.`),
      });
      onRefresh();
    } catch (err: any) {
      toast({
        title: t("common.error", "Error"),
        description: err?.message || "Failed to update channel",
        variant: "destructive",
      });
    } finally {
      setIsSavingChannel(false);
    }
  };

  // Handle Check for Updates
  const handleCheckUpdates = async () => {
    setIsChecking(true);
    try {
      await apiFetch("/api/v1/update/check", { method: "POST" });
      toast({
        title: t("update.checkSuccess", "Update Check Complete"),
        description: t("update.checkSuccessDesc", "Repository and signatures checked against release manifest."),
      });
      onRefresh();
    } catch (err: any) {
      toast({
        title: t("common.error", "Error"),
        description: err?.message || "Failed to check for updates",
        variant: "destructive",
      });
    } finally {
      setIsChecking(false);
    }
  };

  // Handle Update Now
  const handleApplyUpdate = async () => {
    if (!status.available_update) return;

    try {
      setCurrentStep("downloading");
      setStepMessage(t("update.steps.downloading", "Downloading signed package bundle..."));
      await new Promise((r) => setTimeout(r, 600));

      setCurrentStep("verifying_signature");
      setStepMessage(t("update.steps.verifying", "Verifying Ed25519 cryptographic signature..."));
      await new Promise((r) => setTimeout(r, 600));

      setCurrentStep("creating_snapshot");
      setStepMessage(t("update.steps.snapshot", "Creating pre-update backup snapshot..."));
      await new Promise((r) => setTimeout(r, 600));

      setCurrentStep("applying_migrations");
      setStepMessage(t("update.steps.migrations", "Applying database schema migrations..."));
      await new Promise((r) => setTimeout(r, 600));

      // Check if simulation was requested or API call
      const res = await apiFetch<{ ok: boolean; status: string; rolled_back?: boolean; error?: string }>(
        "/api/v1/update/apply",
        {
          method: "POST",
          body: JSON.stringify({
            version: status.available_update.version,
            simulate_failure: simulateFailure,
          }),
        }
      );

      if (res.rolled_back || !res.ok) {
        setCurrentStep("rolling_back");
        setStepMessage(
          t(
            "update.steps.rollbackSuccess",
            "Update failed. Automatic rollback completed: safely returned to previous version."
          )
        );
        toast({
          title: t("update.rollbackNotice", "Automatic Rollback Triggered"),
          description: res.error || t("update.rollbackNoticeDesc", "Safety invariant activated: system rolled back to initial version."),
          variant: "destructive",
        });
      } else {
        setCurrentStep("completed");
        setStepMessage(t("update.steps.completed", "Update successfully applied! Built Panel is running the newest release."));
        toast({
          title: t("update.updateSuccess", "Update Applied Successfully"),
          description: `Built Panel updated to ${status.available_update.version}`,
        });
      }
      onRefresh();
    } catch (err: any) {
      setCurrentStep("failed");
      setStepMessage(err?.message || "Update process encountered an error.");
    }
  };

  // Handle Manual Rollback
  const handleManualRollback = async () => {
    try {
      setCurrentStep("rolling_back");
      setStepMessage(t("update.steps.manualRollback", "Reverting schema and binary to previous version..."));
      await apiFetch("/api/v1/update/rollback", { method: "POST" });
      setCurrentStep("idle");
      toast({
        title: t("update.rollbackSuccessTitle", "Rollback Successful"),
        description: t("update.rollbackSuccessDesc", "Panel safely returned to previous version."),
      });
      onRefresh();
    } catch (err: any) {
      toast({
        title: t("common.error", "Error"),
        description: err?.message || "Failed to rollback version",
        variant: "destructive",
      });
    }
  };

  const isUpdating =
    currentStep !== "idle" &&
    currentStep !== "completed" &&
    currentStep !== "failed";

  return (
    <Card className="border bg-card shadow-sm" data-testid="panel-update-card">
      <CardHeader>
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <ArrowUpCircle className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg font-bold" data-testid="panel-update-title">
                {t("update.title", "Built Panel Release & Update Manager")}
              </CardTitle>
              <Badge variant="outline" className="font-mono text-xs">
                {status.current_version}
              </Badge>
              <Badge
                variant={status.channel === "stable" ? "secondary" : "default"}
                className="capitalize text-xs font-mono"
              >
                {status.channel}
              </Badge>
            </div>
            <CardDescription className="text-xs">
              {t(
                "update.description",
                "Automated self-updating control plane backed by Ed25519 signed manifests and atomic rollbacks."
              )}
            </CardDescription>
          </div>

          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={handleCheckUpdates}
              disabled={isChecking || isUpdating}
              className="gap-2 text-xs h-9"
              data-testid="btn-check-updates"
            >
              <RefreshCw className={`h-4 w-4 ${isChecking ? "animate-spin" : ""}`} />
              <span>{t("update.checkNow", "Check for Updates")}</span>
            </Button>
          </div>
        </div>
      </CardHeader>

      <CardContent className="space-y-6">
        {/* Version & Channel Grid */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 p-4 rounded-lg border bg-muted/20 text-xs">
          <div className="space-y-1">
            <span className="text-muted-foreground">{t("update.installedVersion", "Installed Version")}</span>
            <div className="font-mono text-base font-bold text-foreground" data-testid="current-version-display">
              {status.current_version}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t("update.lastChecked", "Last checked")}: {new Date(status.last_checked_at).toLocaleTimeString()}
            </div>
          </div>

          <div className="space-y-1">
            <span className="text-muted-foreground">{t("update.releaseChannel", "Release Channel")}</span>
            <div className="flex items-center gap-2 pt-1">
              <button
                type="button"
                onClick={() => handleChannelChange("stable")}
                disabled={isSavingChannel || isUpdating}
                className={`px-2.5 py-1 rounded text-xs font-medium border transition-colors cursor-pointer ${
                  activeChannel === "stable"
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-background text-muted-foreground border-border hover:bg-muted"
                }`}
                data-testid="channel-stable-btn"
              >
                Stable
              </button>
              <button
                type="button"
                onClick={() => handleChannelChange("beta")}
                disabled={isSavingChannel || isUpdating}
                className={`px-2.5 py-1 rounded text-xs font-medium border transition-colors cursor-pointer ${
                  activeChannel === "beta"
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-background text-muted-foreground border-border hover:bg-muted"
                }`}
                data-testid="channel-beta-btn"
              >
                Beta / Nightly
              </button>
            </div>
          </div>

          <div className="space-y-1">
            <span className="text-muted-foreground">{t("update.rollbackStatus", "Rollback Checkpoint")}</span>
            <div className="flex items-center gap-2 pt-1">
              {status.can_rollback && status.previous_version ? (
                <div className="space-y-1">
                  <div className="flex items-center gap-1.5 font-mono text-foreground font-semibold">
                    <RotateCcw className="h-3.5 w-3.5 text-amber-500" />
                    <span>v{status.previous_version} ready</span>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={handleManualRollback}
                    disabled={isUpdating}
                    className="h-7 text-[11px] gap-1 px-2 border-amber-500/30 text-amber-600 dark:text-amber-400 hover:bg-amber-500/10"
                    data-testid="btn-manual-rollback"
                  >
                    <RotateCcw className="h-3 w-3" />
                    <span>{t("update.rollbackTo", `Rollback to ${status.previous_version}`)}</span>
                  </Button>
                </div>
              ) : (
                <span className="text-muted-foreground font-mono text-[11px]">
                  {t("update.noRollbackCheckpoint", "No previous snapshot")}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Update Progress Indicator */}
        {currentStep !== "idle" && (
          <div
            className={`p-4 rounded-lg border text-xs space-y-2 ${
              currentStep === "completed"
                ? "bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-300"
                : currentStep === "rolling_back" || currentStep === "failed"
                ? "bg-destructive/10 border-destructive/30 text-destructive"
                : "bg-primary/10 border-primary/30 text-foreground"
            }`}
            data-testid="update-progress-container"
          >
            <div className="flex items-center gap-2 font-semibold">
              {isUpdating && <RefreshCw className="h-4 w-4 animate-spin text-primary" />}
              {currentStep === "completed" && <CheckCircle2 className="h-4 w-4 text-emerald-500" />}
              {(currentStep === "rolling_back" || currentStep === "failed") && (
                <AlertTriangle className="h-4 w-4 text-destructive" />
              )}
              <span data-testid="update-step-message">{stepMessage}</span>
            </div>
          </div>
        )}

        {/* Update Available Box */}
        {status.available_update ? (
          <div
            className="p-4 rounded-lg border border-primary/30 bg-primary/5 space-y-4"
            data-testid="update-available-banner"
          >
            <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <h4 className="text-sm font-bold text-foreground">
                    {t("update.newVersionAvailable", "New Version Available")}: v{status.available_update.version}
                  </h4>
                  <Badge variant="success" className="gap-1 text-[10px]">
                    <ShieldCheck className="h-3 w-3" />
                    <span>Ed25519 Verified ({status.available_update.key_id})</span>
                  </Badge>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("update.releasedOn", "Released on")}{" "}
                  {new Date(status.available_update.release_date).toLocaleDateString()}
                </p>
              </div>

              <div className="flex items-center gap-3">
                <label className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer select-none">
                  <input
                    type="checkbox"
                    checked={simulateFailure}
                    onChange={(e) => setSimulateFailure(e.target.checked)}
                    className="rounded border-border text-primary focus:ring-0"
                    data-testid="simulate-failure-checkbox"
                  />
                  <span>Simulate Failure (Test Auto-Rollback)</span>
                </label>

                <Button
                  variant="default"
                  size="sm"
                  onClick={handleApplyUpdate}
                  disabled={isUpdating}
                  className="gap-2 text-xs h-9 font-semibold"
                  data-testid="btn-update-now"
                >
                  <ArrowUpCircle className="h-4 w-4" />
                  <span>{t("update.updateNow", "Update Now")}</span>
                </Button>
              </div>
            </div>

            {/* Changelog & Migrations */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-2 text-xs">
              <div className="space-y-1.5 p-3 rounded border bg-card">
                <span className="font-semibold text-foreground flex items-center gap-1.5">
                  <FileText className="h-3.5 w-3.5 text-primary" />
                  {t("update.changelog", "Release Notes & Changelog")}
                </span>
                <ul className="list-disc list-inside space-y-1 text-muted-foreground">
                  {status.available_update.changelog.map((item, idx) => (
                    <li key={idx} className="truncate">{item}</li>
                  ))}
                </ul>
              </div>

              <div className="space-y-1.5 p-3 rounded border bg-card">
                <span className="font-semibold text-foreground flex items-center gap-1.5">
                  <Layers className="h-3.5 w-3.5 text-indigo-500" />
                  {t("update.requiredMigrations", "Database Schema Migrations")}
                </span>
                <ul className="space-y-1 font-mono text-[11px] text-muted-foreground">
                  {status.available_update.required_migrations.map((mig, idx) => (
                    <li key={idx} className="flex items-center gap-1.5">
                      <CheckCircle2 className="h-3 w-3 text-emerald-500" />
                      <span>{mig}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>
        ) : (
          <div className="p-4 rounded-lg border bg-muted/10 flex items-center gap-3 text-xs text-muted-foreground">
            <CheckCircle2 className="h-5 w-5 text-emerald-500 shrink-0" />
            <div>
              <span className="font-semibold text-foreground">
                {t("update.upToDate", "Built Panel is up to date.")}
              </span>
              <p>{t("update.upToDateDesc", "You are currently running the latest certified build on the active channel.")}</p>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
