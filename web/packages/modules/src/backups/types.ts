export type BackupStatus = "completed" | "in_progress" | "failed";
export type BackupType = "full" | "files" | "databases";

export interface BackupManifest {
  domains: string[];
  databases: string[];
  has_mail?: boolean;
  has_dns?: boolean;
}

export interface BackupItem {
  id: string;
  name: string;
  type: BackupType;
  size_bytes: number;
  size_formatted: string;
  status: BackupStatus;
  created_at: string;
  completed_at?: string;
  destination: string;
  download_url?: string;
  sha256: string;
  files_count?: number;
  databases_count?: number;
  manifest: BackupManifest;
}

export interface CreateBackupPayload {
  type: BackupType;
  include_databases: boolean;
  include_files: boolean;
  include_mail: boolean;
  note?: string;
}

export interface RestoreBackupPayload {
  backup_id: string;
  restore_mode: "all" | "files_only" | "databases_only" | "selective";
  selected_databases?: string[];
  selected_domains?: string[];
  take_snapshot_before_restore: boolean;
}
