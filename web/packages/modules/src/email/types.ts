export interface Mailbox {
  id: string;
  email: string;
  username: string;
  domain: string;
  quota_mb: number;
  used_bytes: number;
  created_at: string;
  webmail_url: string;
}

export interface EmailForwarder {
  id: string;
  source: string;
  destination: string;
  domain: string;
  created_at: string;
}

export interface Autoresponder {
  id: string;
  email: string;
  domain: string;
  subject: string;
  body: string;
  is_active: boolean;
  start_date?: string;
  end_date?: string;
}

export interface EmailClientConfig {
  incoming_server: string;
  incoming_imap_port: number;
  incoming_pop3_port: number;
  incoming_security: string;
  outgoing_server: string;
  outgoing_smtp_port: number;
  outgoing_submission_port: number;
  outgoing_security: string;
  auth_required: boolean;
}

export interface MailQueueItem {
  id: string;
  sender: string;
  recipient: string;
  status: "queued" | "deferred" | "bounced";
  error_message?: string;
  queued_at: string;
  retries: number;
  size_bytes: number;
}

export interface DeliverabilityCheck {
  domain: string;
  spf_status: "pass" | "neutral" | "fail" | "missing";
  dkim_status: "pass" | "fail" | "missing";
  dmarc_status: "pass" | "fail" | "missing";
  mx_status: "pass" | "fail";
  rbl_blacklisted: boolean;
  rbl_listings: string[];
  recommendations: string[];
}
