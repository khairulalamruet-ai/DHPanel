export type SslStatus = "active" | "pending" | "expired" | "self_signed" | "none";
export type SslType = "lets_encrypt" | "zerossl" | "custom" | "self_signed" | "none";

export interface SslCertificateItem {
  id: string;
  domain_id: string;
  domain_name: string;
  status: SslStatus;
  type: SslType;
  issuer: string | null;
  subject: string;
  sans: string[];
  expires_at: string | null;
  days_remaining: number;
  auto_renew: boolean;
  force_https: boolean;
  hsts_enabled: boolean;
  hsts_subdomains: boolean;
  hsts_preload: boolean;
  created_at: string;
  updated_at: string;
  cert_fingerprint?: string | null;
}

export interface CustomCertPayload {
  cert_pem: string;
  key_pem: string;
  chain_pem?: string | null;
}

export interface SslSettingsPayload {
  auto_renew?: boolean;
  force_https?: boolean;
  hsts_enabled?: boolean;
  hsts_subdomains?: boolean;
  hsts_preload?: boolean;
}

export interface ReissueResponse {
  success: boolean;
  operation_id: string;
  status: "pending" | "dispatched" | "succeeded";
  message: string;
  certificate?: SslCertificateItem;
}
