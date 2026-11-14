import React, { useState, useEffect } from "react";
import {
  Button,
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  Badge,
  Table,
  TableHeader,
  TableBody,
  TableHead,
  TableRow,
  TableCell,
  ConfirmDialog,
  useToast,
} from "@dhpanel/ui";
import { useTranslation, useFormatters } from "@dhpanel/i18n";
import { apiFetch, getCsrfToken } from "@dhpanel/api";
import { useSession } from "@dhpanel/auth";
import {
  Server,
  Plus,
  RotateCcw,
  Info,
  CheckCircle2,
  XCircle,
  Clock,
  AlertTriangle,
  KeyRound,
  Trash2,
} from "lucide-react";
import type { NodeItem, EnrollmentView, NodeStatus } from "./types";
import { AddNodeDialog } from "./AddNodeDialog";
import { NodeDetailDialog } from "./NodeDetailDialog";
import { AgentUpdateDialog } from "./AgentUpdateDialog";

export function NodesPage() {
  const { t } = useTranslation();
  const { formatDate } = useFormatters();
  const { toast } = useToast();
  const { hasPermission, isLoading: isSessionLoading } = useSession();

  const [nodes, setNodes] = useState<NodeItem[]>([]);
  const [enrollments, setEnrollments] = useState<EnrollmentView[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Dialog states
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [selectedNode, setSelectedNode] = useState<NodeItem | null>(null);
  const [revokingNode, setRevokingNode] = useState<NodeItem | null>(null);
  const [cancellingEnrollmentId, setCancellingEnrollmentId] = useState<string | null>(null);
  const [updatingAgentNode, setUpdatingAgentNode] = useState<NodeItem | null>(null);

  // List nodes - TODO(api:T02.08): replace mocked nodes with backend endpoint when implemented
  const fetchNodes = async () => {
    try {
      setIsLoading(true);
      setError(null);
      const res = await apiFetch<{ items: NodeItem[]; total: number }>("/api/v1/admin/nodes");
      setNodes(res.items || []);
    } catch {
      setError(t("common.error"));
    } finally {
      setIsLoading(false);
    }
  };

  const fetchEnrollments = async () => {
    try {
      const res = await apiFetch<{ items: EnrollmentView[] }>("/api/v1/admin/nodes/enrollments");
      setEnrollments(res.items || []);
    } catch {
      // Non-critical; enrollment listing may be empty or unconfigured
    }
  };

  const loadData = async () => {
    await Promise.all([fetchNodes(), fetchEnrollments()]);
  };

  useEffect(() => {
    loadData();
  }, []);

  const handleRevokeConfirm = async () => {
    if (!revokingNode) return;
    try {
      await apiFetch<{ success: boolean; message: string }>(
        `/api/v1/admin/nodes/${revokingNode.id}/revoke`,
        {
          method: "POST",
          headers: {
            "X-CSRF-Token": getCsrfToken() || "",
          },
        }
      );
      toast({
        title: t("nodes.revoke.success"),
        variant: "default",
      });
      fetchNodes();
    } catch {
      toast({
        title: t("common.error"),
        variant: "destructive",
      });
    } finally {
      setRevokingNode(null);
    }
  };

  const handleCancelEnrollment = async (id: string) => {
    try {
      setCancellingEnrollmentId(id);
      await apiFetch(`/api/v1/admin/nodes/enrollments/${id}`, {
        method: "DELETE",
        headers: {
          "X-CSRF-Token": getCsrfToken() || "",
        },
      });
      toast({
        title: t("nodes.enrollments.cancelSuccess"),
        variant: "default",
      });
      fetchEnrollments();
    } catch {
      toast({
        title: t("common.error"),
        variant: "destructive",
      });
    } finally {
      setCancellingEnrollmentId(null);
    }
  };

  const renderStatusBadge = (status: NodeStatus) => {
    switch (status) {
      case "online":
        return (
          <Badge variant="success" className="gap-1 text-[11px] py-0 px-2 font-medium">
            <CheckCircle2 className="h-3 w-3" />
            {t("nodes.status.online")}
          </Badge>
        );
      case "offline":
        return (
          <Badge variant="secondary" className="gap-1 text-[11px] py-0 px-2 font-medium text-muted-foreground">
            <Clock className="h-3 w-3" />
            {t("nodes.status.offline")}
          </Badge>
        );
      case "draining":
        return (
          <Badge variant="outline" className="gap-1 text-[11px] py-0 px-2 text-amber-500 border-amber-500 font-medium">
            <AlertTriangle className="h-3 w-3" />
            {t("nodes.status.draining")}
          </Badge>
        );
      case "revoked":
        return (
          <Badge variant="destructive" className="gap-1 text-[11px] py-0 px-2 font-medium">
            <XCircle className="h-3 w-3" />
            {t("nodes.status.revoked")}
          </Badge>
        );
    }
  };

  const renderEnrollmentStatusBadge = (status: EnrollmentView["status"]) => {
    switch (status) {
      case "pending":
        return (
          <Badge variant="outline" className="text-[10px] text-amber-500 border-amber-500">
            {t("nodes.enrollments.statusPending")}
          </Badge>
        );
      case "consumed":
        return (
          <Badge variant="success" className="text-[10px]">
            {t("nodes.enrollments.statusConsumed")}
          </Badge>
        );
      case "expired":
        return (
          <Badge variant="secondary" className="text-[10px] text-muted-foreground">
            {t("nodes.enrollments.statusExpired")}
          </Badge>
        );
      case "revoked":
        return (
          <Badge variant="destructive" className="text-[10px]">
            {t("nodes.enrollments.statusRevoked")}
          </Badge>
        );
    }
  };

  if (isSessionLoading) {
    return (
      <div className="py-12 text-center text-sm text-muted-foreground">
        {t("common.loading")}
      </div>
    );
  }

  // Permission Guard
  if (!hasPermission("admin.nodes.read")) {
    return (
      <div className="p-8 text-center border rounded-lg bg-card">
        <p className="text-sm text-muted-foreground">{t("common.noPermission")}</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{t("nodes.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("nodes.desc")}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={loadData} className="gap-1.5 text-xs">
            <RotateCcw className="h-3.5 w-3.5" />
            {t("common.reload")}
          </Button>
          {hasPermission("admin.nodes.write") && (
            <Button size="sm" onClick={() => setIsAddOpen(true)} className="gap-1.5 text-xs">
              <Plus className="h-4 w-4" />
              {t("nodes.addNode")}
            </Button>
          )}
        </div>
      </div>

      {/* Nodes Table Card */}
      <Card>
        <CardContent className="p-0">
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("nodes.table.node")}</TableHead>
                  <TableHead className="w-[120px]">{t("nodes.table.status")}</TableHead>
                  <TableHead className="w-[140px]">{t("nodes.table.cpu")}</TableHead>
                  <TableHead className="w-[150px]">{t("nodes.table.ram")}</TableHead>
                  <TableHead className="w-[150px]">{t("nodes.table.disk")}</TableHead>
                  <TableHead>{t("nodes.table.version")}</TableHead>
                  <TableHead>{t("nodes.table.lastSeen")}</TableHead>
                  <TableHead className="text-right">{t("nodes.table.actions")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center py-10 text-muted-foreground text-sm">
                      {t("common.loading")}
                    </TableCell>
                  </TableRow>
                ) : error ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center py-10 text-destructive text-sm">
                      {error}
                    </TableCell>
                  </TableRow>
                ) : nodes.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={8} className="text-center py-10 text-muted-foreground text-sm">
                      {t("nodes.empty")}
                    </TableCell>
                  </TableRow>
                ) : (
                  nodes.map((node) => {
                    const ramUsedGb = (node.capabilities.ram_used_bytes / (1024 * 1024 * 1024)).toFixed(1);
                    const ramTotalGb = (node.capabilities.ram_total_bytes / (1024 * 1024 * 1024)).toFixed(1);
                    const diskUsedGb = (node.capabilities.disk_used_bytes / (1024 * 1024 * 1024)).toFixed(1);
                    const diskTotalGb = (node.capabilities.disk_total_bytes / (1024 * 1024 * 1024)).toFixed(1);

                    return (
                      <TableRow key={node.id}>
                        <TableCell>
                          <div className="space-y-0.5">
                            <span className="font-semibold text-sm text-foreground flex items-center gap-1.5">
                              <Server className="h-3.5 w-3.5 text-primary" />
                              {node.name}
                            </span>
                            <div className="font-mono text-xs text-muted-foreground">
                              {node.hostname} • {node.ip_address}
                            </div>
                          </div>
                        </TableCell>
                        <TableCell>{renderStatusBadge(node.status)}</TableCell>
                        <TableCell>
                          <div className="text-xs">
                            <span className="font-medium">{node.capabilities.cpu_usage_pct}%</span>
                            <span className="text-muted-foreground"> ({node.capabilities.cpu_cores} cores)</span>
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="text-xs font-mono">
                            {ramUsedGb} / {ramTotalGb} GB
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="text-xs font-mono">
                            {diskUsedGb} / {diskTotalGb} GB
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="text-xs font-mono">
                            v{node.version} <span className="text-muted-foreground">(Docker {node.capabilities.docker_version})</span>
                          </div>
                        </TableCell>
                        <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                          {formatDate(new Date(node.last_seen_at))}
                        </TableCell>
                        <TableCell className="text-right">
                          <div className="flex items-center justify-end gap-1.5">
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => setSelectedNode(node)}
                              className="h-8 px-2 text-xs gap-1"
                            >
                              <Info className="h-3.5 w-3.5" />
                              {t("nodes.table.viewDetails")}
                            </Button>
                            {hasPermission("admin.nodes.write") && (
                              <Button
                                variant="outline"
                                size="sm"
                                data-testid={`btn-update-agent-${node.id}`}
                                onClick={() => setUpdatingAgentNode(node)}
                                className="h-8 px-2 text-xs text-primary hover:bg-primary/10 border-primary/30"
                              >
                                {t("nodes.update.actionBtn")}
                              </Button>
                            )}
                            {hasPermission("admin.nodes.write") && node.status !== "revoked" && (
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => setRevokingNode(node)}
                                className="h-8 px-2 text-xs text-destructive hover:bg-destructive/10 border-destructive/30"
                              >
                                {t("nodes.table.revoke")}
                              </Button>
                            )}
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  })
                )}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      {/* Active Enrollments Section */}
      {enrollments.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center gap-2">
              <KeyRound className="h-4 w-4 text-primary" />
              <CardTitle className="text-base">{t("nodes.enrollments.title")}</CardTitle>
            </div>
            <CardDescription className="text-xs">{t("nodes.enrollments.desc")}</CardDescription>
          </CardHeader>
          <CardContent className="p-0">
            <div className="border-t">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("nodes.enrollments.name")}</TableHead>
                    <TableHead className="w-[120px]">{t("nodes.enrollments.status")}</TableHead>
                    <TableHead>{t("nodes.enrollments.created")}</TableHead>
                    <TableHead>{t("nodes.enrollments.expires")}</TableHead>
                    <TableHead className="text-right w-[100px]">{t("nodes.table.actions")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {enrollments.map((enrollment) => (
                    <TableRow key={enrollment.id}>
                      <TableCell className="font-mono text-xs font-medium">
                        {enrollment.name}
                      </TableCell>
                      <TableCell>{renderEnrollmentStatusBadge(enrollment.status)}</TableCell>
                      <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                        {formatDate(new Date(enrollment.created_at))}
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground whitespace-nowrap">
                        {formatDate(new Date(enrollment.expires_at))}
                      </TableCell>
                      <TableCell className="text-right">
                        {hasPermission("admin.nodes.write") && enrollment.status === "pending" && (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => handleCancelEnrollment(enrollment.id)}
                            disabled={cancellingEnrollmentId === enrollment.id}
                            className="h-7 px-2 text-xs text-destructive hover:bg-destructive/10 gap-1"
                          >
                            <Trash2 className="h-3 w-3" />
                            {t("nodes.enrollments.cancel")}
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Add Node Wizard Dialog */}
      <AddNodeDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        onEnrolled={loadData}
      />

      {/* Node Details Dialog */}
      <NodeDetailDialog
        node={selectedNode}
        open={!!selectedNode}
        onOpenChange={(open) => !open && setSelectedNode(null)}
      />

      {/* Agent Self-Update Dialog (T16.08) */}
      <AgentUpdateDialog
        node={updatingAgentNode}
        open={!!updatingAgentNode}
        onOpenChange={(open) => !open && setUpdatingAgentNode(null)}
        onUpdateComplete={loadData}
      />

      {/* Revoke Node Confirmation Dialog (AGENTS.md typed-confirmation) */}
      {revokingNode && (
        <ConfirmDialog
          open={!!revokingNode}
          onOpenChange={(open) => !open && setRevokingNode(null)}
          title={t("nodes.revoke.title")}
          description={t("nodes.revoke.desc")}
          confirmText={revokingNode.name}
          confirmPrompt={t("nodes.revoke.confirmPrompt")}
          destructive={true}
          onConfirm={handleRevokeConfirm}
        />
      )}
    </div>
  );
}
