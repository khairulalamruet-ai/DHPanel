export interface DatabaseItem {
  id: string;
  name: string;
  raw_name: string;
  prefix: string;
  charset: string;
  collation: string;
  size_bytes: number;
  size_formatted: string;
  tables_count: number;
  assigned_users: string[];
  created_at: string;
}

export interface DatabaseUserItem {
  id: string;
  username: string;
  raw_username: string;
  prefix: string;
  host: string;
  grants: {
    db_name: string;
    privileges: string[];
  }[];
  created_at: string;
}

export interface DatabaseLimits {
  max_databases: number;
  used_databases: number;
  max_db_users: number;
  used_db_users: number;
  max_db_size_mb?: number;
  used_db_size_mb?: number;
}

export interface ClientDatabasesResponse {
  databases: DatabaseItem[];
  users: DatabaseUserItem[];
  limits: DatabaseLimits;
  remote_db_hosts: string[];
  phpmyadmin_url: string;
}

export interface CreateDatabasePayload {
  name: string;
  charset?: string;
  collation?: string;
}

export interface CreateDatabaseUserPayload {
  username: string;
  password: string;
  host?: string;
  assignments?: {
    db_name: string;
    privileges: string[];
  }[];
}

export interface UpdateGrantsPayload {
  db_name: string;
  privileges: string[];
}

export interface UpdateRemoteHostsPayload {
  remote_db_hosts: string[];
}

export interface PhpMyAdminSsoResponse {
  redirect_url: string;
  token: string;
  expires_in_seconds: number;
}

export interface ExportDatabaseResponse {
  download_url: string;
  filename: string;
  size_bytes: number;
  expires_at: string;
}

export interface ImportDatabaseResponse {
  success: boolean;
  message: string;
  operation_id?: string;
}
