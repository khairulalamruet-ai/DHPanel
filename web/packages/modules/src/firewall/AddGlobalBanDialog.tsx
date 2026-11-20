import React, { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  Input,
} from "@builtpanel/ui";
import { Ban, Plus } from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";

interface AddGlobalBanDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (ip: string, reason: string, durationHours: number) => Promise<void>;
}

export function AddGlobalBanDialog({ open, onOpenChange, onConfirm }: AddGlobalBanDialogProps) {
  const { t } = useTranslation();
  const [ip, setIp] = useState("");
  const [reason, setReason] = useState("");
  const [durationHours, setDurationHours] = useState(1);
  const [isSubmitting, setIsSubmitting] = useState(false);

  React.useEffect(() => {
    if (open) {
      setIp("");
      setReason("");
      setDurationHours(1);
    }
  }, [open]);

  const isValid = ip.trim().length > 6;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isValid) return;
    try {
      setIsSubmitting(true);
      await onConfirm(ip.trim(), reason.trim(), durationHours);
      onOpenChange(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form onSubmit={handleSubmit} className="space-y-4">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-base font-semibold text-destructive">
              <Ban className="h-5 w-5" />
              <span data-testid="add-global-ban-title">{t("firewall.addGlobalBanTitle")}</span>
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-3">
            <div>
              <label className="text-xs font-medium text-foreground">
                {t("firewall.ipAddressLabel")}
              </label>
              <Input
                data-testid="input-global-ban-ip"
                value={ip}
                onChange={(e) => setIp(e.target.value)}
                placeholder="e.g. 198.51.100.99"
                className="mt-1 font-mono text-xs"
                autoComplete="off"
              />
            </div>

            <div>
              <label className="text-xs font-medium text-foreground">
                {t("firewall.banDurationLabel")}
              </label>
              <select
                data-testid="select-ban-duration"
                value={durationHours}
                onChange={(e) => setDurationHours(Number(e.target.value))}
                className="mt-1 w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-xs font-mono h-9 shadow-sm focus:outline-none"
              >
                <option value={1}>1 Hour (Standard Brute-force window)</option>
                <option value={24}>24 Hours</option>
                <option value={168}>7 Days</option>
                <option value={0}>Permanent Ban</option>
              </select>
            </div>

            <div>
              <label className="text-xs font-medium text-foreground">
                {t("firewall.banReasonLabel")}
              </label>
              <Input
                data-testid="input-global-ban-reason"
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                placeholder="e.g. Repeated SSH / SFTP authentication failures"
                className="mt-1 text-xs"
              />
            </div>
          </div>

          <DialogFooter className="flex justify-end gap-2 pt-2">
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
              variant="destructive"
              disabled={!isValid || isSubmitting}
              data-testid="btn-confirm-add-global-ban"
              className="gap-1.5"
            >
              <Ban className="h-3.5 w-3.5" />
              {t("firewall.btnSubmitBan")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
