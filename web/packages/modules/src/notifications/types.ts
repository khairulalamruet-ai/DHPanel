export type NotificationSeverity = "info" | "warning" | "error" | "success";

export type NotificationCategory = "security" | "backup" | "ssl" | "system" | "billing";

export interface NotificationItem {
  id: string;
  title: string;
  message: string;
  severity: NotificationSeverity;
  category: NotificationCategory;
  read: boolean;
  created_at: string;
  action_url?: string;
  action_label?: string;
}

export interface NotificationChannelConfig {
  email_enabled: boolean;
  email_recipients: string[];
  webhook_enabled: boolean;
  webhook_url?: string;
  in_app_enabled: boolean;
  notify_on_ssl_expiry: boolean;
  notify_on_backup_failure: boolean;
  notify_on_node_offline: boolean;
  notify_on_security_alert: boolean;
  snooze_until?: string | null;
}
