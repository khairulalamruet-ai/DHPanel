DROP INDEX IF EXISTS backup_restores_dns_idx;
ALTER TABLE backup_restores DROP COLUMN IF EXISTS dns_summary, DROP COLUMN IF EXISTS dns_applied_at, DROP COLUMN IF EXISTS dns;
