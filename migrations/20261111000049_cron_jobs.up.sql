-- T10.01: a hosting account's cron jobs. The control plane keeps the definitions and the
-- last result; the node's scheduler runs them (T10.02).
CREATE TABLE cron_jobs (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name            text        NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
    -- Five fields: minute hour day-of-month month day-of-week (UTC).
    schedule        text        NOT NULL CHECK (length(schedule) BETWEEN 9 AND 100),
    command         text        NOT NULL CHECK (length(command) BETWEEN 1 AND 10000),
    enabled         boolean     NOT NULL DEFAULT true,
    timeout_seconds integer     NOT NULL DEFAULT 300 CHECK (timeout_seconds BETWEEN 1 AND 86400),
    -- What the node last did with it.
    last_run_at     timestamptz,
    last_status     text        CHECK (last_status IN ('ok', 'failed', 'timeout', 'skipped')),
    last_exit_code  integer,
    last_output     text        CHECK (last_output IS NULL OR length(last_output) <= 20000),
    -- Counts every change, so that the node can tell whether it has the latest.
    version         bigint      NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
