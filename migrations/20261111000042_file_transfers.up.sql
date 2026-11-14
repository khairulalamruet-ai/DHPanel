-- T08.07: big files between the browser and a tenant's files.
--
-- An upload is sent to the control plane in chunks (the spool folder), then the node
-- fetches it with a token only that transfer knows and puts it in the tenant's files.
-- A download is the other way: the node sends the file to the spool folder, the client
-- gets a link that works for five minutes. Only hashes of tokens are kept, plus the
-- node's token sealed until the node has had it.
CREATE TABLE file_transfers (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    kind            text        NOT NULL CHECK (kind IN ('upload', 'download')),
    path            text        NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),
    overwrite       boolean     NOT NULL DEFAULT false,
    size_bytes      bigint      CHECK (size_bytes IS NULL OR size_bytes > 0),
    received_bytes  bigint      NOT NULL DEFAULT 0 CHECK (received_bytes >= 0),
    next_chunk      integer     NOT NULL DEFAULT 0 CHECK (next_chunk >= 0),
    sha256          text        CHECK (sha256 IS NULL OR sha256 ~ '^[0-9a-f]{64}$'),
    status          text        NOT NULL
                    CHECK (status IN ('uploading', 'queued', 'running', 'ready', 'done',
                                      'failed', 'cancelled', 'expired')),
    error           text        CHECK (error IS NULL OR length(error) <= 500),
    token_hash      bytea       CHECK (token_hash IS NULL OR octet_length(token_hash) = 32),
    sealed_token    bytea       CHECK (sealed_token IS NULL OR octet_length(sealed_token) <= 1024),
    link_hash       bytea       CHECK (link_hash IS NULL OR octet_length(link_hash) = 32),
    link_expires_at timestamptz,
    created_by      uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    finished_at     timestamptz,
    expires_at      timestamptz
);
CREATE INDEX file_transfers_tenant_idx ON file_transfers (tenant_id, created_at DESC);

-- What the node's answers do to the rows: made, removed, failed, imported, exported, transferred.
CREATE OR REPLACE FUNCTION operations_apply_to_tenant() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
        RETURN NEW;
    END IF;
    -- Starting is news for imports, exports and file transfers only.
    IF NEW.status = 'dispatched' AND NEW.kind NOT IN ('import_sql', 'export_sql', 'fetch_file', 'send_file') THEN
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
    ELSIF NEW.kind = 'export_sql' THEN
        -- `export:<export id>`. The dump itself arrives through the upload endpoints,
        -- which make the row `ready`; here the node starting and failing is noted. A
        -- node that says it is done without having sent the dump did not do its work.
        UPDATE db_exports
           SET status = CASE
                            WHEN NEW.status = 'dispatched' THEN 'running'
                            ELSE 'failed' END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the export failed'), 500)
                            WHEN NEW.status = 'succeeded' THEN 'the node finished without sending the dump' END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               sealed_token = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN NULL ELSE sealed_token END,
               upload_token_hash = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN NULL ELSE upload_token_hash END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('queued', 'running');
    ELSIF NEW.kind IN ('fetch_file', 'send_file') THEN
        -- `transfer:<transfer id>`. A fetch (an upload coming in) ends `done`; a send (a
        -- download going out) is made `ready` by the node's upload endpoints, so here only
        -- its start and its failure are noted, and a node that says it is done without
        -- having sent the file did not do its work.
        UPDATE file_transfers
           SET status = CASE
                            WHEN NEW.status = 'dispatched' THEN 'running'
                            WHEN NEW.status = 'succeeded' AND NEW.kind = 'fetch_file' THEN 'done'
                            ELSE 'failed' END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the transfer failed'), 500)
                            WHEN NEW.status = 'succeeded' AND NEW.kind = 'send_file'
                            THEN 'the node finished without sending the file' END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               sealed_token = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN NULL ELSE sealed_token END,
               token_hash = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN NULL ELSE token_hash END,
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

