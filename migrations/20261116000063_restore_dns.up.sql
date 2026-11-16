-- A restore may also put the zone records of the account's domains back (opt-in). The node
-- reads them from the backup and answers with them; the panel applies them and notes here
-- when, and what changed (including the records that were replaced, so they can be set again).
ALTER TABLE backup_restores
    ADD COLUMN dns             boolean     NOT NULL DEFAULT false,
    ADD COLUMN dns_applied_at  timestamptz,
    ADD COLUMN dns_summary     jsonb;
CREATE INDEX backup_restores_dns_idx ON backup_restores (status) WHERE dns AND dns_applied_at IS NULL;
