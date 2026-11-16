-- T12.10: a tenant's own cache servers (Redis, Memcached): one of each kind per account. The
-- dispatcher moves a row from pending to ready (or failed) when the node has answered.
CREATE TABLE cache_instances (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    kind            text        NOT NULL CHECK (kind IN ('redis', 'memcached')),
    memory_mb       integer     NOT NULL CHECK (memory_mb BETWEEN 1 AND 4096),
    status          text        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'ready', 'failed', 'removing')),
    -- The operation whose answer is awaited.
    op_id           uuid,
    -- Redis's password, sealed with the master key.
    sealed_password bytea,
    host            text,
    port            integer,
    error           text        CHECK (error IS NULL OR length(error) <= 500),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, kind)
);
