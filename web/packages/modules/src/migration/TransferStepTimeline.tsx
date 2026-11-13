import React from "react";
import { CheckCircle2, Circle, Clock, Loader2, XCircle } from "lucide-react";
import { TransferStep } from "./types";

interface TransferStepTimelineProps {
  steps: TransferStep[];
}

export function TransferStepTimeline({ steps }: TransferStepTimelineProps) {
  const renderStepIcon = (status: TransferStep["status"]) => {
    switch (status) {
      case "completed":
        return <CheckCircle2 className="h-5 w-5 text-emerald-500 shrink-0" />;
      case "running":
        return <Loader2 className="h-5 w-5 text-primary animate-spin shrink-0" />;
      case "failed":
        return <XCircle className="h-5 w-5 text-destructive shrink-0" />;
      case "pending":
      default:
        return <Circle className="h-5 w-5 text-muted-foreground/40 shrink-0" />;
    }
  };

  return (
    <div className="space-y-3" data-testid="transfer-step-timeline">
      {steps.map((step, idx) => {
        const isLast = idx === steps.length - 1;
        return (
          <div key={step.id || idx} className="relative flex items-start gap-3" data-testid={`step-item-${step.id}`}>
            {/* Timeline connector line */}
            {!isLast && (
              <div
                className={`absolute left-2.5 top-6 bottom-0 w-0.5 -ml-px ${
                  step.status === "completed" ? "bg-emerald-500/50" : "bg-muted"
                }`}
              />
            )}

            <div className="z-10 bg-background">{renderStepIcon(step.status)}</div>

            <div className="flex-1 pb-4">
              <div className="flex items-center justify-between gap-2">
                <span
                  className={`text-xs font-semibold ${
                    step.status === "running"
                      ? "text-primary"
                      : step.status === "completed"
                      ? "text-foreground"
                      : step.status === "failed"
                      ? "text-destructive"
                      : "text-muted-foreground"
                  }`}
                >
                  {step.name}
                </span>

                {step.completed_at && (
                  <span className="text-[10px] text-muted-foreground font-mono flex items-center gap-1">
                    <Clock className="h-3 w-3" />
                    {new Date(step.completed_at).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                      second: "2-digit",
                    })}
                  </span>
                )}
              </div>

              {step.details && (
                <p className="text-[11px] text-muted-foreground mt-0.5">{step.details}</p>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
