import React from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  Button,
  Badge,
} from "@dhpanel/ui";
import { useTranslation, useFormatters } from "@dhpanel/i18n";
import { Server, Cpu, HardDrive, Shield, Activity, Fingerprint } from "lucide-react";
import type { NodeItem } from "./types";

interface NodeDetailDialogProps {
  node: NodeItem | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function NodeDetailDialog({ node, open, onOpenChange }: NodeDetailDialogProps) {
  const { t } = useTranslation();
  const { formatDate } = useFormatters();

  if (!node) return null;

  const ramUsedGb = (node.capabilities.ram_used_bytes / (1024 * 1024 * 1024)).toFixed(1);
  const ramTotalGb = (node.capabilities.ram_total_bytes / (1024 * 1024 * 1024)).toFixed(1);
  const diskUsedGb = (node.capabilities.disk_used_bytes / (1024 * 1024 * 1024)).toFixed(1);
  const diskTotalGb = (node.capabilities.disk_total_bytes / (1024 * 1024 * 1024)).toFixed(1);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <Server className="h-5 w-5 text-primary" />
            <DialogTitle>{node.name}</DialogTitle>
          </div>
          <DialogDescription className="font-mono text-xs">
            {node.hostname} • {node.ip_address}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          {/* Hardware & Resources Grid */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
            <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
              <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
                <Cpu className="h-4 w-4 text-primary" />
                {t("nodes.details.cpuModel")}
              </div>
              <p className="font-medium text-foreground">{node.capabilities.cpu_model}</p>
              <p className="text-muted-foreground">
                {node.capabilities.cpu_cores} Cores • {node.capabilities.cpu_usage_pct}% load
              </p>
            </div>

            <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
              <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
                <Activity className="h-4 w-4 text-primary" />
                {t("nodes.table.ram")}
              </div>
              <p className="font-medium text-foreground">
                {ramUsedGb} GB / {ramTotalGb} GB
              </p>
              <p className="text-muted-foreground">
                {Math.round((node.capabilities.ram_used_bytes / node.capabilities.ram_total_bytes) * 100)}% utilization
              </p>
            </div>

            <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
              <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
                <HardDrive className="h-4 w-4 text-primary" />
                {t("nodes.table.disk")}
              </div>
              <p className="font-medium text-foreground">
                {diskUsedGb} GB / {diskTotalGb} GB
              </p>
              <p className="text-muted-foreground">
                {Math.round((node.capabilities.disk_used_bytes / node.capabilities.disk_total_bytes) * 100)}% utilization
              </p>
            </div>

            <div className="p-3 rounded-lg border bg-muted/30 space-y-1">
              <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
                <Server className="h-4 w-4 text-primary" />
                {t("nodes.details.docker")}
              </div>
              <p className="font-medium text-foreground">{node.capabilities.docker_version}</p>
              <p className="text-muted-foreground font-mono text-[11px]">{node.capabilities.kernel}</p>
            </div>
          </div>

          {/* Security Features */}
          <div className="p-3.5 rounded-lg border bg-muted/30 space-y-2 text-xs">
            <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
              <Shield className="h-4 w-4 text-emerald-500" />
              {t("nodes.details.securityFeatures")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {node.capabilities.security_features.map((feature) => (
                <Badge key={feature} variant="outline" className="text-[11px] font-mono bg-background">
                  {feature}
                </Badge>
              ))}
            </div>
          </div>

          {/* mTLS Identity & Fingerprint */}
          <div className="p-3.5 rounded-lg border bg-muted/30 space-y-2 text-xs">
            <div className="flex items-center gap-1.5 text-muted-foreground font-semibold">
              <Fingerprint className="h-4 w-4 text-primary" />
              {t("nodes.details.fingerprint")}
            </div>
            <p className="font-mono text-[11px] break-all bg-muted/60 p-2 rounded border">
              {node.fingerprint}
            </p>
          </div>

          {/* Metadata Row */}
          <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground pt-1">
            <div>
              <span className="font-medium">{t("nodes.details.enrolledAt")}: </span>
              <span>{formatDate(new Date(node.enrolled_at))}</span>
            </div>
            <div>
              <span className="font-medium">{t("nodes.table.lastSeen")}: </span>
              <span>{formatDate(new Date(node.last_seen_at))}</span>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("nodes.details.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
