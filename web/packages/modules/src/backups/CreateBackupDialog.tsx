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
import { Archive, Database, Folder, Mail } from "lucide-react";
import { useTranslation } from "@builtpanel/i18n";
import { BackupType, CreateBackupPayload } from "./types";

interface CreateBackupDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}

export function CreateBackupDialog({
  open,
  onOpenChange,
  onCreated,
}: CreateBackupDialogProps) {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [type, setType] = useState<BackupType>("full");
  const [includeFiles, setIncludeFiles] = useState(true);
  const [includeDatabases, setIncludeDatabases] = useState(true);
  const [includeMail, setIncludeMail] = useState(false);
  const [note, setNote] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleTypeSelect = (newType: BackupType) => {
    setType(newType);
    if (newType === "full") {
      setIncludeFiles(true);
      setIncludeDatabases(true);
      setIncludeMail(true);
    } else if (newType === "files") {
      setIncludeFiles(true);
      setIncludeDatabases(false);
      setIncludeMail(false);
    } else if (newType === "databases") {
      setIncludeFiles(false);
      setIncludeDatabases(true);
      setIncludeMail(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    try {
      setIsSubmitting(true);
      await apiFetch("/api/v1/client/backups", {
        method: "POST",
        body: JSON.stringify({
          type,
          include_files: includeFiles,
          include_databases: includeDatabases,
          include_mail: includeMail,
          note: note.trim() || undefined,
        } as CreateBackupPayload),
      });

      toast({
        title: t("common.success"),
        description: t("backups.backupStarted"),
      });

      setNote("");
      onOpenChange(false);
      onCreated();
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
              <Archive className="w-5 h-5" />
            </div>
            <DialogTitle data-testid="create-backup-dialog-title">
              {t("backups.takeBackupNow")}
            </DialogTitle>
          </div>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4 py-2">
          {/* Type Selection */}
          <div className="space-y-2">
            <label className="text-sm font-medium">
              {t("backups.backupType")}
            </label>
            <div className="grid grid-cols-3 gap-2">
              <button
                type="button"
                onClick={() => handleTypeSelect("full")}
                className={`p-3 rounded-lg border text-left flex flex-col gap-1 transition-colors ${
                  type === "full" ? "border-primary bg-primary/5 text-primary" : "border-border hover:bg-muted/50"
                }`}
                data-testid="type-select-full"
              >
                <Archive className="w-4 h-4" />
                <span className="text-xs font-semibold">{t("backups.fullBackup")}</span>
              </button>

              <button
                type="button"
                onClick={() => handleTypeSelect("files")}
                className={`p-3 rounded-lg border text-left flex flex-col gap-1 transition-colors ${
                  type === "files" ? "border-primary bg-primary/5 text-primary" : "border-border hover:bg-muted/50"
                }`}
                data-testid="type-select-files"
              >
                <Folder className="w-4 h-4" />
                <span className="text-xs font-semibold">{t("backups.filesOnly")}</span>
              </button>

              <button
                type="button"
                onClick={() => handleTypeSelect("databases")}
                className={`p-3 rounded-lg border text-left flex flex-col gap-1 transition-colors ${
                  type === "databases" ? "border-primary bg-primary/5 text-primary" : "border-border hover:bg-muted/50"
                }`}
                data-testid="type-select-databases"
              >
                <Database className="w-4 h-4" />
                <span className="text-xs font-semibold">{t("backups.databasesOnly")}</span>
              </button>
            </div>
          </div>

          {/* Component Checkboxes */}
          <div className="space-y-2 pt-1">
            <label className="text-sm font-medium">
              {t("backups.componentsIncluded")}
            </label>
            <div className="space-y-2 p-3 bg-muted/30 border rounded-lg">
              <label className="flex items-center gap-2.5 text-xs font-medium cursor-pointer">
                <input
                  type="checkbox"
                  checked={includeFiles}
                  onChange={(e) => setIncludeFiles(e.target.checked)}
                  data-testid="checkbox-include-files"
                />
                <Folder className="w-3.5 h-3.5 text-amber-500" />
                <span>{t("backups.homeAndWebFiles")}</span>
              </label>

              <label className="flex items-center gap-2.5 text-xs font-medium cursor-pointer">
                <input
                  type="checkbox"
                  checked={includeDatabases}
                  onChange={(e) => setIncludeDatabases(e.target.checked)}
                  data-testid="checkbox-include-databases"
                />
                <Database className="w-3.5 h-3.5 text-blue-500" />
                <span>{t("backups.mariaDbDatabases")}</span>
              </label>

              <label className="flex items-center gap-2.5 text-xs font-medium cursor-pointer">
                <input
                  type="checkbox"
                  checked={includeMail}
                  onChange={(e) => setIncludeMail(e.target.checked)}
                  data-testid="checkbox-include-mail"
                />
                <Mail className="w-3.5 h-3.5 text-emerald-500" />
                <span>{t("backups.mailAccountsAndMessages")}</span>
              </label>
            </div>
          </div>

          {/* Note / Memo */}
          <div className="space-y-1.5">
            <label className="text-sm font-medium">
              {t("backups.noteOptional")}
            </label>
            <Input
              value={note}
              onChange={(e) => setNote(e.target.value)}
              placeholder={t("backups.notePlaceholder")}
              data-testid="input-backup-note"
            />
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
              disabled={(!includeFiles && !includeDatabases && !includeMail) || isSubmitting}
              data-testid="btn-submit-create-backup"
            >
              {isSubmitting ? t("common.submitting") : t("backups.startBackup")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
