export type UpdateChannel = "stable" | "beta";

export type UpdateStep =
  | "idle"
  | "checking"
  | "downloading"
  | "verifying_signature"
  | "creating_snapshot"
  | "applying_migrations"
  | "restarting"
  | "rolling_back"
  | "completed"
  | "failed";

export interface SignedUpdateManifest {
  version: string;
  release_date: string;
  channel: UpdateChannel;
  ed25519_signature: string;
  key_id: string;
  checksum_sha256: string;
  changelog: string[];
  required_migrations: string[];
  can_rollback: boolean;
}

export interface UpdateHistoryItem {
  id: string;
  from_version: string;
  to_version: string;
  channel: UpdateChannel;
  status: "success" | "rolled_back" | "failed";
  timestamp: string;
  details: string;
}

export interface PanelUpdateStatus {
  current_version: string;
  channel: UpdateChannel;
  last_checked_at: string;
  available_update: SignedUpdateManifest | null;
  previous_version: string | null;
  can_rollback: boolean;
  history: UpdateHistoryItem[];
}
