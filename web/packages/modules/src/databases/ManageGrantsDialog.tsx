import React, { useState, useEffect } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  Button,
  useToast,
} from "@builtpanel/ui";
import { apiFetch } from "@builtpanel/api";
import { Shield, Check, Lock } from "lucide-react";
import { DatabaseUserItem, DatabaseItem, UpdateGrantsPayload } from "./types";
import { useTranslation } from "@builtpanel/i18n";

interface ManageGrantsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  user: DatabaseUserItem | null;
  databases: DatabaseItem[];
  onGrantsUpdated: (updatedUser: DatabaseUserItem) => void;
}

const AVAILABLE_PRIVILEGES = [
  { name: "ALL PRIVILEGES", category: "all" },
  { name: "SELECT", category: "data" },
  { name: "INSERT", category: "data" },
  { name: "UPDATE", category: "data" },
  { name: "DELETE", category: "data" },
  { name: "CREATE", category: "structure" },
  { name: "ALTER", category: "structure" },
  { name: "INDEX", category: "structure" },
  { name: "DROP", category: "structure" },
  { name: "CREATE TEMPORARY TABLES", category: "admin" },
  { name: "LOCK TABLES", category: "admin" },
  { name: "EXECUTE", category: "admin" },
];

export function ManageGrantsDialog({
  open,
  onOpenChange,
  user,
  databases,
  onGrantsUpdated,
}: ManageGrantsDialogProps) {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [selectedDb, setSelectedDb] = useState<string>("");
  const [privileges, setPrivileges] = useState<string[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (open && user && databases.length > 0) {
      const initialDb = databases[0].name;
      setSelectedDb(initialDb);
      const existingGrant = user.grants.find((g) => g.db_name === initialDb);
      setPrivileges(existingGrant ? existingGrant.privileges : []);
    }
  }, [open, user, databases]);

  const handleDbChange = (dbName: string) => {
    setSelectedDb(dbName);
    if (!user) return;
    const existingGrant = user.grants.find((g) => g.db_name === dbName);
    setPrivileges(existingGrant ? existingGrant.privileges : []);
  };

  const handleTogglePrivilege = (priv: string) => {
    if (priv === "ALL PRIVILEGES") {
      if (privileges.includes("ALL PRIVILEGES")) {
        setPrivileges([]);
      } else {
        setPrivileges(AVAILABLE_PRIVILEGES.map((p) => p.name));
      }
      return;
    }

    setPrivileges((prev) => {
      let next: string[];
      if (prev.includes(priv)) {
        next = prev.filter((p) => p !== priv && p !== "ALL PRIVILEGES");
      } else {
        next = [...prev, priv];
        // If all single privileges selected, include ALL PRIVILEGES
        const singles = AVAILABLE_PRIVILEGES.filter((p) => p.name !== "ALL PRIVILEGES").map((p) => p.name);
        if (singles.every((s) => next.includes(s))) {
          next.push("ALL PRIVILEGES");
        }
      }
      return next;
    });
  };

  const handleApplyPreset = (preset: "all" | "readonly" | "readwrite" | "none") => {
    switch (preset) {
      case "all":
        setPrivileges(AVAILABLE_PRIVILEGES.map((p) => p.name));
        break;
      case "readonly":
        setPrivileges(["SELECT"]);
        break;
      case "readwrite":
        setPrivileges(["SELECT", "INSERT", "UPDATE", "DELETE"]);
        break;
      case "none":
        setPrivileges([]);
        break;
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!user || !selectedDb) return;

    try {
      setIsSubmitting(true);
      const payload: UpdateGrantsPayload = {
        db_name: selectedDb,
        privileges,
      };

      const updatedUser = await apiFetch<DatabaseUserItem>(
        `/api/v1/client/databases/users/${user.id}/grants`,
        {
          method: "PUT",
          body: JSON.stringify(payload),
        }
      );

      toast({
        title: t("common.success"),
        description: t("databases.toast.grantsUpdated", "Privileges updated for {{user}} on {{db}}", {
          user: user.username,
          db: selectedDb,
        }),
        variant: "default",
      });

      onGrantsUpdated(updatedUser);
      onOpenChange(false);
    } catch (err: any) {
      toast({
        title: t("common.error"),
        description: err?.message || t("databases.errors.grantsUpdateFailed", "Failed to update grants"),
        variant: "destructive",
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!user) return null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg" onClose={() => onOpenChange(false)}>
        <DialogHeader>
          <div className="flex items-center gap-3">
            <div className="rounded-full bg-primary/10 p-2.5 text-primary">
              <Shield className="h-5 w-5" />
            </div>
            <div>
              <DialogTitle>{t("databases.grantsDialog.title", "Manage Database Privileges")}</DialogTitle>
              <DialogDescription className="mt-0.5 text-xs font-mono">
                {user.username}@{user.host}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="mt-4 space-y-4">
          {/* Target Database Select */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-foreground">
              {t("databases.fields.targetDatabase", "Target Database")}
            </label>
            <select
              value={selectedDb}
              onChange={(e) => handleDbChange(e.target.value)}
              className="w-full h-9 rounded-md border border-input bg-background px-2.5 text-xs font-mono"
              data-testid="select-grant-database"
            >
              {databases.map((db) => (
                <option key={db.id} value={db.name}>
                  {db.name}
                </option>
              ))}
            </select>
          </div>

          {/* Quick Preset Buttons */}
          <div className="flex items-center justify-between border-y border-border py-2 text-xs">
            <span className="font-medium text-muted-foreground">
              {t("databases.grantsDialog.presets", "Presets")}:
            </span>
            <div className="flex items-center gap-1.5">
              <button
                type="button"
                onClick={() => handleApplyPreset("all")}
                className="px-2 py-1 rounded bg-secondary hover:bg-secondary/80 text-[11px]"
                data-testid="preset-all"
              >
                All Privileges
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset("readonly")}
                className="px-2 py-1 rounded bg-secondary hover:bg-secondary/80 text-[11px]"
                data-testid="preset-readonly"
              >
                Read Only
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset("readwrite")}
                className="px-2 py-1 rounded bg-secondary hover:bg-secondary/80 text-[11px]"
                data-testid="preset-readwrite"
              >
                Read / Write
              </button>
              <button
                type="button"
                onClick={() => handleApplyPreset("none")}
                className="px-2 py-1 rounded bg-secondary hover:bg-secondary/80 text-[11px]"
                data-testid="preset-none"
              >
                Revoke All
              </button>
            </div>
          </div>

          {/* Privileges Grid */}
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 max-h-56 overflow-y-auto p-1">
            {AVAILABLE_PRIVILEGES.map((priv) => {
              const isChecked = privileges.includes(priv.name);
              return (
                <label
                  key={priv.name}
                  className={`flex items-center gap-2 p-2 rounded border text-xs cursor-pointer transition-colors ${
                    isChecked
                      ? "border-primary bg-primary/10 text-primary font-medium"
                      : "border-border hover:bg-muted text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={isChecked}
                    onChange={() => handleTogglePrivilege(priv.name)}
                    className="rounded border-input text-primary focus:ring-primary h-3.5 w-3.5"
                    data-testid={`checkbox-priv-${priv.name.replace(/\s+/g, "_")}`}
                  />
                  <span className="truncate">{priv.name}</span>
                </label>
              );
            })}
          </div>

          <DialogFooter className="mt-6 border-t border-border pt-3">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => onOpenChange(false)}
              disabled={isSubmitting}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="submit"
              size="sm"
              disabled={isSubmitting}
              data-testid="btn-save-grants"
            >
              {isSubmitting ? t("common.saving") : t("databases.actions.savePrivileges", "Save Privileges")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
