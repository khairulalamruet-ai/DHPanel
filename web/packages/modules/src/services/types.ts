export type ServiceStatus = "running" | "stopped" | "restarting" | "degraded";

export interface ServiceItem {
  id: string;
  name: "nginx" | "php-fpm";
  display_name: string;
  description: string;
  status: ServiceStatus;
  uptime_seconds: number;
  memory_usage_mb: number;
  cpu_percent: number;
  workers_count: number;
  last_restarted_at: string;
  restarts_count_this_minute: number;
  restarts_limit_per_minute: number;
  retry_after_seconds?: number;
}

export interface RestartResponse {
  success: boolean;
  message: string;
  service: ServiceItem;
}

export interface ServiceRateLimitError {
  error: string;
  message: string;
  retry_after_seconds: number;
}
