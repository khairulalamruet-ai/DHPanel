-- T12.01: which application runs where. A place is a domain (or none yet) and a path under it;
-- one application per place. The kind is a catalog slug (wordpress, joomla, nodejs, ...): the
-- catalog (T12.05) is data, so the list is open.
CREATE TABLE apps (
    id          uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The domain it is served on; null when the domain was removed (the files stay).
    domain_id   uuid        REFERENCES domains (id) ON DELETE SET NULL,
    path        text        NOT NULL DEFAULT '/'
        CHECK (length(path) <= 200 AND path ~ '^/([A-Za-z0-9._~-]+(/[A-Za-z0-9._~-]+)*)?$'),
    kind        text        NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_-]{1,31}$'),
    name        text        NOT NULL CHECK (length(name) BETWEEN 1 AND 100 AND name !~ '[[:cntrl:]]'),
    version     text        CHECK (version IS NULL OR length(version) <= 40),
    status      text        NOT NULL DEFAULT 'installing'
        CHECK (status IN ('installing', 'active', 'failed', 'removing')),
    database_id uuid        REFERENCES databases (id) ON DELETE SET NULL,
    settings    jsonb       NOT NULL DEFAULT '{}',
    error       text        CHECK (error IS NULL OR length(error) <= 500),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
-- One application per place (a failed install does not hold the place).
CREATE UNIQUE INDEX apps_place_idx ON apps (
    tenant_id, coalesce(domain_id, '00000000-0000-0000-0000-000000000000'::uuid), path
) WHERE status <> 'failed';
CREATE INDEX apps_tenant_idx ON apps (tenant_id, created_at);
