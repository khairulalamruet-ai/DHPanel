import React, { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  Button,
  Input,
  Badge,
} from "@builtpanel/ui";
import { ShieldOff, AlertTriangle } from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";

interface DisableRuleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  ruleId: number | null;
  ruleMessage?: string;
  domain: string;
  onConfirm: (ruleId: number, domain: string, reason: string) => Promise<void>;
}

export function DisableRuleDialog({
  open,
  onOpenChange,
  ruleId,
  ruleMessage,
  domain,
  onConfirm,
}: DisableRuleDialogProps) {
  const { t } = useTranslation();
  const [reason, setReason] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  React.useEffect(() => {
    if (open) {
      setReason("");
    }
  }, [open]);

  if (ruleId === null) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      setIsSubmitting(true);
      await onConfirm(ruleId, domain, reason);
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
            <DialogTitle className="flex items-center gap-2 text-amber-600 dark:text-amber-400">
              <ShieldOff className="h-5 w-5" />
              <span data-testid="disable-rule-dialog-title">{t("waf.disableRuleTitle")}</span>
            </DialogTitle>
          </DialogHeader>

          <div className="rounded-md border border-amber-500/20 bg-amber-500/10 p-3 text-xs text-amber-800 dark:text-amber-300">
            <div className="flex items-start gap-2">
              <AlertTriangle className="h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400 mt-0.5" />
              <div>
                <p className="font-semibold">{t("waf.disableRuleWarningHeading")}</p>
                <p className="mt-1 opacity-90">{t("waf.disableRuleWarningText")}</p>
              </div>
            </div>
          </div>

          <div className="space-y-2 rounded border bg-muted/20 p-3 text-xs">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("waf.ruleId")}:</span>
              <Badge variant="outline" className="font-mono text-xs" data-testid="badge-modal-rule-id">
                {ruleId}
              </Badge>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">{t("waf.targetDomain")}:</span>
              <span className="font-mono font-medium" data-testid="text-modal-domain">{domain}</span>
            </div>
            {ruleMessage && (
              <div className="pt-1">
                <span className="text-muted-foreground">{t("waf.ruleDescription")}:</span>
                <p className="mt-0.5 font-mono text-[11px] text-foreground">{ruleMessage}</p>
              </div>
            )}
          </div>

          <div>
            <label className="text-xs font-medium text-foreground">
              {t("waf.disableReasonLabel")}
            </label>
            <Input
              data-testid="input-disable-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder={t("waf.disableReasonPlaceholder")}
              className="mt-1 text-xs"
            />
          </div>

          <DialogFooter className="flex justify-end gap-2">
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
              variant="default"
              disabled={isSubmitting}
              data-testid="btn-confirm-disable-rule"
              className="gap-1.5 bg-amber-600 hover:bg-amber-700 text-white"
            >
              <ShieldOff className="h-3.5 w-3.5" />
              {t("waf.disableRuleButton")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
