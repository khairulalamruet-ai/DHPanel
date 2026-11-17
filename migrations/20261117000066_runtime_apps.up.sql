-- T12.07, T12.08: a tenant's own applications (Node.js, Python) in containers of their own,
-- behind one of the tenant's domains. The dispatcher moves a row from pending to ready (or
-- failed) when the node has answered.
CREATE TABLE runtime_apps (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- A short name, unique in the account; part of the container's name.
    name          text        NOT NULL CHECK (name ~ '^[a-z][a-z0-9-]{0,30}[a-z0-9]$'),
    kind          text        NOT NULL CHECK (kind IN ('node', 'python')),
    version       text        NOT NULL CHECK (version ~ '^[0-9.]{1,6}$'),
    -- The folder of the application below the account's web directory ('' is the directory).
    dir           text        NOT NULL DEFAULT ''
        CHECK (length(dir) <= 200 AND dir !~ '(^|/)\.\.?(/|$)' AND dir !~ '^/' AND dir !~ '[^A-Za-z0-9._/-]'),
    -- What starts the application (words; no shell). `$PORT` is the port it must listen on.
    start_command text        NOT NULL CHECK (length(start_command) BETWEEN 1 AND 500),
    -- Run the install step (npm install, pip install -r requirements.txt) before it starts.
    install       boolean     NOT NULL DEFAULT true,
    port          integer     NOT NULL DEFAULT 3000 CHECK (port BETWEEN 1024 AND 65535),
    memory_mb     integer     NOT NULL DEFAULT 256 CHECK (memory_mb BETWEEN 32 AND 4096),
    -- The environment, as a JSON object sealed with the master key (it holds secrets), and the
    -- names of the variables, to show.
    sealed_env    bytea,
    env_names     text[]      NOT NULL DEFAULT '{}',
    -- The domain that shows the application; one application per domain.
    domain_id     uuid        UNIQUE REFERENCES domains (id) ON DELETE SET NULL,
    status        text        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'ready', 'failed', 'removing')),
    op_id         uuid,
    -- Where the web server reaches it (`dh-<account>-app-<name>:<port>`), once it runs.
    host          text,
    error         text        CHECK (error IS NULL OR length(error) <= 500),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE INDEX runtime_apps_awaiting_idx ON runtime_apps (id) WHERE op_id IS NOT NULL;

-- The web server's site files change with an application: its domain, host or readiness.
CREATE FUNCTION runtime_apps_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tenants SET routes_version = routes_version + 1
     WHERE id = COALESCE(NEW.tenant_id, OLD.tenant_id);
    RETURN NULL;
END;
$$;

CREATE TRIGGER runtime_apps_routes_changed
    AFTER INSERT OR DELETE OR UPDATE OF domain_id, host, status, port ON runtime_apps
    FOR EACH ROW EXECUTE FUNCTION runtime_apps_routes_changed();
