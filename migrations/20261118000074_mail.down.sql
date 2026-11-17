DROP TRIGGER IF EXISTS tenants_mail_going ON tenants;
DROP TRIGGER IF EXISTS domains_mail_going ON domains;
DROP FUNCTION IF EXISTS mail_domain_going();
DROP TABLE IF EXISTS mail_queue_reports;
DROP TABLE IF EXISTS mail_forwarders;
DROP TABLE IF EXISTS mailbox_passwords;
DROP TABLE IF EXISTS mailboxes;
DROP TABLE IF EXISTS mail_domains;
DROP TABLE IF EXISTS mail_servers;
