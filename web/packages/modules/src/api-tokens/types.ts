export type TokenScope =
  | "users:read"
  | "users:write"
  | "domains:read"
  | "domains:write"
  | "databases:manage"
  | "dns:manage"
  | "billing:whmcs";

export interface ApiTokenItem {
  id: string;
  name: string;
  token_preview: string; // e.g. dhp_live_8f...4a2b
  scopes: TokenScope[];
  created_at: string;
  expires_at: string | null;
  last_used_at: string | null;
}

export interface CreateTokenPayload {
  name: string;
  scopes: TokenScope[];
  expiry_days: number | null; // null for never
}

export interface CreateTokenResponse {
  token: ApiTokenItem;
  secret_key: string; // Plaintext token returned once
}

export interface ApiTestCallPayload {
  token_id: string;
  endpoint: "/users" | "/admin/nodes" | "/billing/whmcs/suspend";
  method: "GET" | "POST";
}

export interface ApiTestCallResponse {
  status: number;
  message: string;
  data?: any;
  required_scope?: string;
}
