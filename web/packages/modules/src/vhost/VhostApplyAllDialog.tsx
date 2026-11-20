import React, { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  Button,
} from "@builtpanel/ui";
import { apiFetch } from "@builtpanel/api";
import { Send, CheckCircle2, AlertTriangle, RefreshCw, Server } from "lucide-react";
import { useOperation } from "../operations/useOperation";
import type { VhostTemplate, VhostApplyAllResponse } from "./types";

interface VhostApplyAllDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  template: VhostTemplate | null;
  onSuccess?: () => void;
}

export function VhostApplyAllDialog({
  open,
  onOpenChange,
  template,
  onSuccess,
}: VhostApplyAllDialogProps) {
  const [activeOpId, setActiveOpId] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Real-time operation tracker
  const { operation, isPolling } = useOperation(activeOpId, {
    enabled: !!activeOpId,
    pollInterval: 1000,
    onSuccess: () => {
      onSuccess?.();
    },
  });

  const handleStartApply = async () => {
    if (!template) return;
    try {
      setIsSubmitting(true);
      setError(null);
      const res = await apiFetch<VhostApplyAllResponse>(
        `/api/v1/admin/vhost-templates/${template.id}/apply-all`,
        { method: "POST" }
      );
      setActiveOpId(res.operation_id);
    } catch (err: any) {
      setError(err?.message || "Failed to dispatch apply-all operation");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleClose = () => {
    setActiveOpId(null);
    setError(null);
    onOpenChange(false);
  };

  if (!template) return null;

  const isComplete = operation?.status === "succeeded";
  const isFailed = operation?.status === "failed";
  const inProgress = isSubmitting || isPolling || (operation && !isComplete && !isFailed);

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-md" data-testid="dialog-vhost-apply-all">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Send className="h-5 w-5 text-primary" />
            Apply Template to All Domains
          </DialogTitle>
          <DialogDescription>
            Deploy and reload virtual host configuration across all domains using this template.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 my-2 text-xs">
          {/* Target summary banner */}
          <div className="p-3 bg-muted/50 rounded-lg border space-y-1">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground font-semibold">Target Template:</span>
              <span className="font-mono font-bold text-foreground">{template.name}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground font-semibold">Web Server:</span>
              <span className="font-mono uppercase text-foreground">{template.server_type}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground font-semibold">Active Domains:</span>
              <span className="font-semibold text-primary">{template.domain_count} domains</span>
            </div>
          </div>

          {/* Operation Status Feedback */}
          {activeOpId && (
            <div
              className={`p-3.5 rounded-lg border text-xs space-y-2 ${
                isComplete
                  ? "bg-emerald-500/10 border-emerald-500/20 text-emerald-800 dark:text-emerald-300"
                  : isFailed
                  ? "bg-destructive/10 border-destructive/20 text-destructive"
                  : "bg-primary/5 border-primary/20 text-primary"
              }`}
              data-testid="operation-progress-card"
            >
              <div className="flex items-center gap-2 font-semibold">
                {isComplete ? (
                  <CheckCircle2 className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                ) : isFailed ? (
                  <AlertTriangle className="h-4 w-4 text-destructive" />
                ) : (
                  <RefreshCw className="h-4 w-4 animate-spin text-primary" />
                )}
                <span>
                  {isComplete
                    ? "Deployment Succeeded!"
                    : isFailed
                    ? "Deployment Failed"
                    : "Deploying VHost Template..."}
                </span>
              </div>

              <p className="text-[11px] leading-relaxed opacity-90">
                {isComplete
                  ? `Successfully re-rendered configuration and reloaded ${template.server_type} across all ${template.domain_count} domains.`
                  : isFailed
                  ? (operation?.error?.message || "Operation failed to reload web server.")
                  : "Dispatching configuration reload operation to nodes..."}
              </p>

              {activeOpId && (
                <p className="font-mono text-[10px] opacity-75">
                  Operation ID: {activeOpId}
                </p>
              )}
            </div>
          )}

          {error && (
            <div className="p-3 text-xs bg-destructive/10 border border-destructive/20 text-destructive rounded-lg flex items-center gap-2">
              <AlertTriangle className="h-4 w-4 shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {!activeOpId && (
            <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-800 dark:text-amber-300 rounded-lg flex items-start gap-2">
              <Server className="h-4 w-4 shrink-0 mt-0.5 text-amber-600 dark:text-amber-400" />
              <span>
                This operation will rebuild configuration files for all {template.domain_count} domains
                and reload the web server. Zero-downtime graceful reload will be performed.
              </span>
            </div>
          )}
        </div>

        <DialogFooter className="gap-2 sm:gap-0">
          <Button variant="outline" onClick={handleClose} disabled={Boolean(inProgress)}>
            {isComplete ? "Done" : "Cancel"}
          </Button>

          {!isComplete && (
            <Button
              onClick={handleStartApply}
              disabled={Boolean(inProgress)}
              className="gap-1.5"
              data-testid="btn-confirm-apply-all"
            >
              <Send className={`h-4 w-4 ${inProgress ? "animate-pulse" : ""}`} />
              {inProgress ? "Deploying..." : "Deploy to All Domains"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
