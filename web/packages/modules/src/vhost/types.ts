export type ServerType = "nginx" | "caddy";

export interface VhostTemplate {
  id: string;
  name: string;
  slug: string;
  server_type: ServerType;
  content: string;
  variables: string[];
  version: number;
  is_default: boolean;
  domain_count: number;
  created_at: string;
  updated_at: string;
}

export interface VhostTemplateVersion {
  id: string;
  template_id: string;
  version: number;
  content: string;
  changelog: string;
  created_by: string;
  created_at: string;
}

export interface VhostPreviewPayload {
  content: string;
  sample_variables?: Record<string, string>;
}

export interface VhostPreviewResponse {
  rendered: string;
  valid: boolean;
  errors?: string[];
}

export interface VhostApplyAllResponse {
  success: boolean;
  operation_id: string;
  target_count: number;
  message: string;
}
