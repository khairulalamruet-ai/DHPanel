ALTER TABLE domains
    DROP CONSTRAINT IF EXISTS domains_hsts_needs_https,
    DROP COLUMN IF EXISTS force_https,
    DROP COLUMN IF EXISTS hsts;
