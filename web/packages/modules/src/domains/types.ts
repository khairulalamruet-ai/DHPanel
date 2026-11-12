export type DomainType = "primary" | "addon" | "subdomain" | "alias";
export type SslStatus = "active" | "pending" | "expired" | "self_signed" | "none";
export type DnsStatus = "active" | "pending" | "proxied" | "error";

export interface DomainRedirect {
  enabled: boolean;
  target_url: string;
  status_code: 301 | 302;
}

export interface ClientDomainItem {
  id: string;
  name: string;
  type: DomainType;
  docroot: string;
  is_primary: boolean;
  ssl_status: SslStatus;
  ssl_expires_at?: string | null;
  ssl_issuer?: string | null;
  dns_status: DnsStatus;
  redirect?: DomainRedirect | null;
  php_version?: string;
  created_at: string;
}

export interface ClientDomainsResponse {
  items: ClientDomainItem[];
  total: number;
  limits: {
    max_domains: number; // -1 for unlimited
    used_domains: number;
  };
}

export interface CreateDomainPayload {
  name: string;
  type: DomainType;
  docroot?: string;
  redirect?: DomainRedirect;
  enable_ssl?: boolean;
}

export interface UpdateDocrootPayload {
  docroot: string;
}

export interface UpdateRedirectPayload {
  enabled: boolean;
  target_url: string;
  status_code: 301 | 302;
}
