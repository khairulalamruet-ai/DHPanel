-- T07.07: SQL imports.
--
-- A dump is uploaded in chunks to the control plane's spool folder, then the node
-- downloads it with a one-time token (only its hash is kept here, plus the token
-- sealed until the node has had it) and runs it. The row follows the operation
-- `import:<id>` (see the trigger function below).
CREATE TABLE db_imports (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    database_id    uuid        NOT NULL REFERENCES databases (id) ON DELETE CASCADE,
    gzip           boolean     NOT NULL,
    size_bytes     bigint      NOT NULL CHECK (size_bytes > 0),
    sha256         text        NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    status         text        NOT NULL DEFAULT 'uploading'
                   CHECK (status IN ('uploading', 'queued', 'running', 'done', 'failed', 'cancelled')),
    received_bytes bigint      NOT NULL DEFAULT 0 CHECK (received_bytes >= 0),
    next_chunk     integer     NOT NULL DEFAULT 0 CHECK (next_chunk >= 0),
    -- What the node last said: downloading, checking, importing.
    stage          text        CHECK (stage IS NULL OR length(stage) <= 20),
    bytes_done     bigint      NOT NULL DEFAULT 0,
    statements     bigint      NOT NULL DEFAULT 0,
    error          text        CHECK (error IS NULL OR length(error) <= 500),
    token_hash     bytea       CHECK (token_hash IS NULL OR octet_length(token_hash) = 32),
    sealed_token   bytea       CHECK (sealed_token IS NULL OR octet_length(sealed_token) <= 1024),
    created_by     uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    finished_at    timestamptz
);
CREATE INDEX db_imports_tenant_idx ON db_imports (tenant_id, created_at DESC);
-- One import at a time per database.
CREATE UNIQUE INDEX db_imports_one_active_idx ON db_imports (database_id)
    WHERE status IN ('uploading', 'queued', 'running');

-- What the node's answers do to the rows: made, removed, failed, imported.
CREATE OR REPLACE FUNCTION operations_apply_to_tenant() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        RETURN NEW;
    END IF;
    -- Starting is news for imports only.
    IF NEW.status = 'dispatched' AND NEW.kind <> 'import_sql' THEN
        RETURN NEW;
    END IF;
    IF NEW.kind = 'set_up_tenant' THEN
        UPDATE tenants
           SET status = CASE WHEN NEW.status = 'succeeded' THEN 'active' ELSE 'failed' END
         WHERE id = NEW.tenant_id AND status = 'provisioning';
    ELSIF NEW.kind = 'suspend_tenant' AND NEW.status = 'succeeded' THEN
        UPDATE tenants SET status = 'suspended' WHERE id = NEW.tenant_id AND status = 'active';
    ELSIF NEW.kind = 'resume_tenant' AND NEW.status = 'succeeded' THEN
        UPDATE tenants SET status = 'active' WHERE id = NEW.tenant_id AND status = 'suspended';
    ELSIF NEW.kind = 'delete_tenant' AND NEW.status = 'succeeded'
          AND COALESCE((NEW.payload ->> 'purge_data')::boolean, false) THEN
        DELETE FROM tenants WHERE id = NEW.tenant_id AND status = 'deleting';
    ELSIF NEW.kind = 'create_database' THEN
        UPDATE databases
           SET status = CASE WHEN NEW.status = 'succeeded' THEN 'ready' ELSE 'failed' END
         WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name' AND status = 'pending'
           AND NEW.status IN ('succeeded', 'failed');
    ELSIF NEW.kind = 'drop_database' AND NEW.status = 'succeeded' THEN
        DELETE FROM databases
         WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name' AND status = 'deleting';
    ELSIF NEW.kind = 'upsert_db_user' AND NEW.status IN ('succeeded', 'failed') THEN
        UPDATE db_users
           SET status = CASE WHEN NEW.status = 'succeeded' THEN 'ready' ELSE 'failed' END
         WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name' AND status = 'pending';
        -- The password was only kept until the node had it (or the work gave up).
        DELETE FROM db_user_passwords
         WHERE user_id IN (SELECT id FROM db_users
                            WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name');
    ELSIF NEW.kind = 'drop_db_user' AND NEW.status = 'succeeded' THEN
        DELETE FROM db_users
         WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name' AND status = 'deleting';
    ELSIF NEW.kind = 'import_sql' THEN
        -- `import:<import id>`; what the dump did stays in the node's answer.
        UPDATE db_imports
           SET status = CASE NEW.status
                            WHEN 'dispatched' THEN 'running'
                            WHEN 'succeeded' THEN 'done'
                            ELSE 'failed' END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the import failed'), 500) END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               sealed_token = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN NULL ELSE sealed_token END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('queued', 'running');
    ELSIF NEW.kind = 'set_php_versions' AND NEW.status = 'succeeded' THEN
        -- `php:<tenant>:<state>:<slot>`
        UPDATE tenants
           SET php_applied_version = GREATEST(php_applied_version,
                                              split_part(NEW.idempotency_key, ':', 3)::bigint),
               routes_synced_version = 0
         WHERE id = NEW.tenant_id AND NEW.idempotency_key ~ '^php:[0-9a-f]+:[0-9]+:[0-9]+$';
    ELSIF NEW.kind IN ('set_site_routes', 'set_custom_certificates', 'attach_domain',
                       'keep_domains', 'set_php_versions', 'set_php_ini')
          AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;

-- Starting an import is news too (the row turns `running`).
DROP TRIGGER operations_apply_to_tenant ON operations;
CREATE TRIGGER operations_apply_to_tenant
    AFTER UPDATE OF status ON operations
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status
          AND NEW.status IN ('succeeded', 'failed', 'dispatched'))
    EXECUTE FUNCTION operations_apply_to_tenant();
