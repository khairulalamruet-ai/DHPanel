export type LogType = "access" | "error" | "php";

export interface LogEntry {
  id: string;
  lineNumber: number;
  timestamp: string;
  type: LogType;
  domain: string;
  level?: "info" | "warn" | "error" | "debug";
  status_code?: number;
  method?: string;
  path?: string;
  ip?: string;
  message: string;
  raw: string;
}

export interface DomainLogOverview {
  domain: string;
  access_log_size_bytes: number;
  access_log_size_formatted: string;
  error_log_size_bytes: number;
  error_log_size_formatted: string;
  php_error_log_size_bytes: number;
  php_error_log_size_formatted: string;
  last_updated_at: string;
}

export interface LogFilterOptions {
  domain: string;
  type: LogType;
  search: string;
  level?: string;
}

export type SystemLogService = "control-plane" | "agent" | "caddy" | "auth";
export type SystemLogLevel = "all" | "debug" | "info" | "warn" | "error" | "fatal";

export interface SystemLogEntry {
  id: string;
  lineNumber: number;
  timestamp: string;
  service: SystemLogService;
  node: string;
  level: "debug" | "info" | "warn" | "error" | "fatal";
  component?: string;
  message: string;
  raw: string;
}

export interface SystemLogServiceStats {
  service: SystemLogService;
  name: string;
  lines_count: number;
  size_formatted: string;
  error_count: number;
  last_entry_at: string;
}
