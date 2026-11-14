-- T09.02: backups, where they go, and when they are made.
--
-- A destination is where backups are stored (a folder on the node, an S3-compatible
-- bucket, an SFTP folder). What is not secret (host, bucket, path, user) is in `settings`;
-- the secret part (an access key, a password, a private key) is sealed with the panel's
-- master key into `sealed_credentials` and never leaves the panel except for the one
-- operation that needs it.
CREATE TABLE backup_destinations (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name               text        NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 80),
    kind               text        NOT NULL CHECK (kind IN ('local', 's3', 'sftp')),
    settings           jsonb       NOT NULL DEFAULT '{}',
    sealed_credentials bytea       CHECK (sealed_credentials IS NULL OR octet_length(sealed_credentials) <= 8192),
    enabled            boolean     NOT NULL DEFAULT true,
    -- The destination new backups go to, unless a schedule says another.
    is_default         boolean     NOT NULL DEFAULT false,
    last_tested_at     timestamptz,
    last_test_ok       boolean,
    last_test_error    text        CHECK (last_test_error IS NULL OR length(last_test_error) <= 500),
    created_by         uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX backup_destinations_one_default_idx ON backup_destinations (is_default) WHERE is_default;

CREATE TABLE backups (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- A hosting account's backup, or none for the panel's own (system) backup.
    tenant_id       uuid        REFERENCES tenants (id) ON DELETE SET NULL,
    -- Kept after the account is gone, for the listing and for restoring elsewhere.
    account_name    text        CHECK (account_name IS NULL OR length(account_name) <= 64),
    kind            text        NOT NULL CHECK (kind IN ('tenant', 'system')),
    trigger         text        NOT NULL CHECK (trigger IN ('manual', 'schedule', 'before_restore')),
    destination_id  uuid        NOT NULL REFERENCES backup_destinations (id),
    status          text        NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'running', 'ready', 'failed', 'deleting', 'deleted')),
    size_bytes      bigint      CHECK (size_bytes IS NULL OR size_bytes >= 0),
    -- What it holds: [{"name": "files.tar.zst.enc", "kind": "files", "stored_bytes": 1}, ...]
    parts           jsonb       NOT NULL DEFAULT '[]',
    -- SHA-256 of the (encrypted) manifest, so that replacing the manifest is found too.
    manifest_sha256 text        CHECK (manifest_sha256 IS NULL OR manifest_sha256 ~ '^[0-9a-f]{64}$'),
    -- The key the parts are encrypted with, sealed with the master key.
    sealed_key      bytea       CHECK (sealed_key IS NULL OR octet_length(sealed_key) <= 1024),
    error           text        CHECK (error IS NULL OR length(error) <= 500),
    created_by      uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    finished_at     timestamptz,
    -- When retention removes it (none: kept until someone removes it).
    expires_at      timestamptz
);
CREATE INDEX backups_tenant_idx ON backups (tenant_id, created_at DESC);
CREATE INDEX backups_status_idx ON backups (status) WHERE status IN ('queued', 'running', 'deleting');

CREATE TABLE backup_schedules (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- One schedule per hosting account, or none for the panel's own backup.
    tenant_id       uuid        UNIQUE REFERENCES tenants (id) ON DELETE CASCADE,
    kind            text        NOT NULL CHECK (kind IN ('tenant', 'system')),
    -- Five-field cron: minute hour day-of-month month day-of-week (UTC).
    cron            text        NOT NULL CHECK (length(cron) BETWEEN 9 AND 100),
    keep_daily      integer     NOT NULL DEFAULT 7 CHECK (keep_daily BETWEEN 0 AND 365),
    keep_weekly     integer     NOT NULL DEFAULT 4 CHECK (keep_weekly BETWEEN 0 AND 520),
    destination_id  uuid        REFERENCES backup_destinations (id),
    enabled         boolean     NOT NULL DEFAULT true,
    next_run_at     timestamptz,
    last_run_at     timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CHECK ((kind = 'tenant') = (tenant_id IS NOT NULL))
);
CREATE UNIQUE INDEX backup_schedules_one_system_idx ON backup_schedules (kind) WHERE kind = 'system';
CREATE INDEX backup_schedules_due_idx ON backup_schedules (next_run_at) WHERE enabled;
