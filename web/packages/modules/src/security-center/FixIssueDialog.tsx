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
} from "@builtpanel/ui";
import { useTranslation } from "@builtpanel/i18n";
import { Wrench, CheckCircle, AlertTriangle, ArrowRight } from "lucide-react";
import { SecurityIssue } from "./types";

interface FixIssueDialogProps {
  open: boolean;
  issue: SecurityIssue | null;
  onClose: () => void;
  onExecuteFix: (issueId: string) => Promise<void>;
  onNavigateTo: (path: string) => void;
}

export function FixIssueDialog({
  open,
  issue,
  onClose,
  onExecuteFix,
  onNavigateTo,
}: FixIssueDialogProps) {
  const { t, i18n } = useTranslation();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [success, setSuccess] = useState(false);

  if (!issue) return null;

  const isBn = i18n.language?.startsWith("bn");
  const title = (isBn && issue.title_bn) ? issue.title_bn : issue.title;
  const description = (isBn && issue.description_bn) ? issue.description_bn : issue.description;

  const handleFix = async () => {
    if (issue.fix_action.type === "link" && issue.fix_action.target_url) {
      onNavigateTo(issue.fix_action.target_url);
      onClose();
      return;
    }

    setIsSubmitting(true);
    try {
      await onExecuteFix(issue.id);
      setSuccess(true);
      setTimeout(() => {
        setSuccess(false);
        onClose();
      }, 1200);
    } catch {
      // handled by parent or toast
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(val) => !val && onClose()}>
      <DialogContent data-testid="fix-issue-dialog" className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <div className="p-2 rounded-full bg-primary/10 text-primary">
              <Wrench className="h-5 w-5" />
            </div>
            <div>
              <DialogTitle data-testid="fix-dialog-title">
                {t("securityCenter.fixIssueTitle", "Remediate Security Issue")}
              </DialogTitle>
              <DialogDescription className="text-xs">
                {issue.resource}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="p-3 bg-muted/50 rounded-lg space-y-1.5 border">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-sm">{title}</span>
              <Badge variant={issue.severity === "critical" ? "destructive" : "secondary"}>
                {issue.severity.toUpperCase()}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">{description}</p>
          </div>

          <div className="p-3 bg-blue-50/50 dark:bg-blue-950/20 border border-blue-200/50 rounded-lg text-xs space-y-1">
            <div className="font-semibold text-blue-900 dark:text-blue-300 flex items-center gap-1.5">
              <AlertTriangle className="h-3.5 w-3.5 text-blue-500" />
              {t("securityCenter.remediationAction", "Recommended Action")}
            </div>
            <p className="text-muted-foreground">
              {t(issue.fix_action.label_key, issue.fix_action.default_label)}
            </p>
          </div>

          {success && (
            <div className="p-3 bg-emerald-50 text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-300 rounded-lg text-xs flex items-center gap-2">
              <CheckCircle className="h-4 w-4 text-emerald-600" />
              {t("securityCenter.fixSuccess", "Issue resolved and security rule applied successfully!")}
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel", "Cancel")}
          </Button>
          <Button
            onClick={handleFix}
            disabled={isSubmitting || success}
            data-testid="btn-confirm-fix"
            className="flex items-center gap-1.5"
          >
            {issue.fix_action.type === "link" ? (
              <>
                <span>{t(issue.fix_action.label_key, issue.fix_action.default_label)}</span>
                <ArrowRight className="h-4 w-4" />
              </>
            ) : (
              <>
                <Wrench className="h-4 w-4" />
                <span>{t(issue.fix_action.label_key, issue.fix_action.default_label)}</span>
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
