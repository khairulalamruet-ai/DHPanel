export type DnsRecordType =
  | "A"
  | "AAAA"
  | "CNAME"
  | "MX"
  | "TXT"
  | "SRV"
  | "CAA"
  | "NS";

export interface DnsRecord {
  id: string;
  zone_id: string;
  name: string; // e.g. "@", "www", "mail", "sub"
  type: DnsRecordType;
  content: string; // IPv4, IPv6, FQDN target, TXT string, etc.
  ttl: number; // e.g. 300, 3600, 86400
  priority?: number | null; // For MX, SRV
  weight?: number | null; // For SRV
  port?: number | null; // For SRV
  flags?: number | null; // For CAA
  tag?: string | null; // For CAA ("issue", "issuewild", "iodef")
  created_at: string;
  updated_at: string;
}

export interface DnsZone {
  id: string;
  domain_id: string;
  domain_name: string;
  tenant_id: string;
  nameservers: string[];
  soa_serial: number;
  records_count: number;
  is_custom_ns: boolean;
  dnssec_enabled: boolean;
  updated_at: string;
}

export interface DnsZoneDetailResponse {
  zone: DnsZone;
  records: DnsRecord[];
}

export interface CreateDnsRecordPayload {
  name: string;
  type: DnsRecordType;
  content: string;
  ttl: number;
  priority?: number | null;
  weight?: number | null;
  port?: number | null;
  flags?: number | null;
  tag?: string | null;
}

export interface UpdateDnsRecordPayload {
  name?: string;
  type?: DnsRecordType;
  content?: string;
  ttl?: number;
  priority?: number | null;
  weight?: number | null;
  port?: number | null;
  flags?: number | null;
  tag?: string | null;
}

export interface BindImportPayload {
  bind_text: string;
  overwrite: boolean;
}

export interface BindImportResponse {
  imported_count: number;
  skipped_count: number;
  warnings: string[];
  records: DnsRecord[];
}

export interface BindExportResponse {
  zone_name: string;
  soa_serial: number;
  bind_text: string;
}
