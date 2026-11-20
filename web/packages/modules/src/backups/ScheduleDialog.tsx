import React, { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  Input,
  useToast,
} from "@builtpanel/ui";
import { apiFetch } from "@builtpanel/api";
import { Clock, Calendar } from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";
import { BackupDestination } from "./types";

interface ScheduleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  destinations: BackupDestination[];
  onSaved: () => void;
}

export function ScheduleDialog({
  open,
  onOpenChange,
  destinations,
  onSaved,
}: ScheduleDialogProps) {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [name, setName] = useState("");
  const [cronExpression, setCronExpression] = useState("0 2 * * *");
  const [destinationId, setDestinationId] = useState(destinations[0]?.id || "");
  const [scope, setScope] = useState<"all_tenants" | "system">("all_tenants");
  const [retentionDaily, setRetentionDaily] = useState(7);
  const [retentionWeekly, setRetentionWeekly] = useState(4);
  const [retentionMonthly, setRetentionMonthly] = useState(3);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const applyPreset = (cron: string) => {
    setCronExpression(cron);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    try {
      setIsSubmitting(true);
      await apiFetch("/api/v1/admin/backups/schedules", {
        method: "POST",
        body: JSON.stringify({
          name: name.trim(),
          cron_expression: cronExpression,
          destination_id: destinationId || destinations[0]?.id,
          scope,
          retention_daily: Number(retentionDaily),
          retention_weekly: Number(retentionWeekly),
          retention_monthly: Number(retentionMonthly),
        }),
      });

      toast({
        title: t("common.success"),
        description: t("adminBackups.scheduleCreated"),
      });

      setName("");
      onOpenChange(false);
      onSaved();
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err.message || t("common.unknownError"),
        variant: "destructive",
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <div className="p-2 rounded-lg bg-primary/10 text-primary">
              <Calendar className="w-5 h-5" />
            </div>
            <DialogTitle data-testid="schedule-dialog-title">
              {t("adminBackups.newSchedule")}
            </DialogTitle>
          </div>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4 py-2">
          {/* Name */}
          <div className="space-y-1.5">
            <label className="text-sm font-medium">
              {t("adminBackups.scheduleName")}
            </label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Daily Nightly All Tenants"
              data-testid="input-schedule-name"
              autoFocus
            />
          </div>

          {/* Scope */}
          <div className="space-y-1.5">
            <label className="text-sm font-medium">
              {t("adminBackups.backupScope")}
            </label>
            <div className="flex gap-4">
              <label className="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  type="radio"
                  name="schedule-scope"
                  checked={scope === "all_tenants"}
                  onChange={() => setScope("all_tenants")}
                  data-testid="scope-all-tenants"
                />
                <span>{t("adminBackups.allTenants")}</span>
              </label>
              <label className="flex items-center gap-2 text-sm cursor-pointer">
                <input
                  type="radio"
                  name="schedule-scope"
                  checked={scope === "system"}
                  onChange={() => setScope("system")}
                  data-testid="scope-system"
                />
                <span>{t("adminBackups.systemControlPlane")}</span>
              </label>
            </div>
          </div>

          {/* Destination */}
          <div className="space-y-1.5">
            <label className="text-sm font-medium">
              {t("adminBackups.destination")}
            </label>
            <select
              value={destinationId}
              onChange={(e) => setDestinationId(e.target.value)}
              className="w-full h-9 rounded-md border border-input bg-background px-3 text-sm font-mono"
              data-testid="select-schedule-destination"
            >
              {destinations.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} ({d.type})
                </option>
              ))}
            </select>
          </div>

          {/* Cron Schedule */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium">{t("adminBackups.frequencyCron")}</label>
              <div className="flex gap-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 text-xs px-1.5"
                  onClick={() => applyPreset("0 2 * * *")}
                  data-testid="preset-daily"
                >
                  {t("adminBackups.daily2am")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-6 text-xs px-1.5"
                  onClick={() => applyPreset("0 3 * * 0")}
                  data-testid="preset-weekly"
                >
                  {t("adminBackups.weeklySun")}
                </Button>
              </div>
            </div>
            <Input
              value={cronExpression}
              onChange={(e) => setCronExpression(e.target.value)}
              placeholder="0 2 * * *"
              className="font-mono text-xs"
              data-testid="input-cron-expression"
            />
          </div>

          {/* Retention Policy */}
          <div className="space-y-2 pt-1 border-t">
            <label className="text-sm font-medium">
              {t("adminBackups.retentionPolicy")}
            </label>
            <div className="grid grid-cols-3 gap-2">
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">{t("adminBackups.keepDaily")}</label>
                <Input
                  type="number"
                  min="1"
                  value={retentionDaily}
                  onChange={(e) => setRetentionDaily(Number(e.target.value))}
                  className="font-mono text-xs"
                  data-testid="input-retention-daily"
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">{t("adminBackups.keepWeekly")}</label>
                <Input
                  type="number"
                  min="0"
                  value={retentionWeekly}
                  onChange={(e) => setRetentionWeekly(Number(e.target.value))}
                  className="font-mono text-xs"
                  data-testid="input-retention-weekly"
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-muted-foreground">{t("adminBackups.keepMonthly")}</label>
                <Input
                  type="number"
                  min="0"
                  value={retentionMonthly}
                  onChange={(e) => setRetentionMonthly(Number(e.target.value))}
                  className="font-mono text-xs"
                  data-testid="input-retention-monthly"
                />
              </div>
            </div>
          </div>

          <DialogFooter className="pt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isSubmitting}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="submit"
              disabled={!name.trim() || isSubmitting}
              data-testid="btn-submit-schedule"
            >
              {isSubmitting ? t("common.submitting") : t("common.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
