-- What the node's answers do to the rows: ... backups, and the cron runs it reports (T10.02).
CREATE OR REPLACE FUNCTION operations_apply_to_tenant() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    -- The runs the node's cron scheduler reports (not about one tenant): the newest run of
    -- each job becomes the job's last result, unless a newer one is already there.
    IF NEW.kind = 'report_cron_runs' THEN
        IF NEW.status = 'succeeded' THEN
            UPDATE cron_jobs j
               SET last_run_at = to_timestamp(r.started_at),
                   last_status = r.status,
                   last_exit_code = r.exit_code,
                   last_output = left(r.output, 20000)
              FROM (SELECT DISTINCT ON (job_id) job_id, started_at, status, exit_code, output
                      FROM jsonb_to_recordset(COALESCE(NEW.output -> 'runs', '[]'::jsonb))
                           AS x(job_id uuid, started_at bigint, status text, exit_code int, output text)
                     WHERE status IN ('ok', 'failed', 'timeout')
                     ORDER BY job_id, started_at DESC) r
             WHERE j.id = r.job_id
               AND (j.last_run_at IS NULL OR to_timestamp(r.started_at) > j.last_run_at);
        END IF;
        RETURN NEW;
    END IF;
    IF NEW.tenant_id IS NULL THEN
        RETURN NEW;
    END IF;
    -- Starting is news for imports, exports and file transfers only.
    IF NEW.status = 'dispatched' AND NEW.kind NOT IN ('import_sql', 'export_sql', 'fetch_file', 'send_file', 'create_backup', 'restore_backup', 'export_backup', 'import_export') THEN
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
    ELSIF NEW.kind = 'upsert_ftp_account' AND NEW.status IN ('succeeded', 'failed') THEN
        UPDATE ftp_accounts
           SET status = CASE WHEN NEW.status = 'succeeded' THEN 'ready' ELSE 'failed' END,
               updated_at = now()
         WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name' AND status = 'pending';
        -- The password was only kept until the node had it (or the work gave up).
        DELETE FROM ftp_account_passwords
         WHERE account_id IN (SELECT id FROM ftp_accounts
                               WHERE tenant_id = NEW.tenant_id AND name = NEW.payload ->> 'name');
    ELSIF NEW.kind = 'drop_ftp_account' AND NEW.status = 'succeeded' THEN
        DELETE FROM ftp_accounts WHERE name = NEW.payload ->> 'name' AND status = 'deleting';
    ELSIF NEW.kind = 'set_php_versions' AND NEW.status = 'succeeded' THEN
        -- `php:<tenant>:<state>:<slot>`
        UPDATE tenants
           SET php_applied_version = GREATEST(php_applied_version,
                                              split_part(NEW.idempotency_key, ':', 3)::bigint),
               routes_synced_version = 0
         WHERE id = NEW.tenant_id AND NEW.idempotency_key ~ '^php:[0-9a-f]+:[0-9]+:[0-9]+$';
    ELSIF NEW.kind = 'create_backup' THEN
        -- `backup:<backup id>`. What the node made is in its answer.
        UPDATE backups
           SET status = CASE NEW.status
                            WHEN 'dispatched' THEN 'running'
                            WHEN 'succeeded' THEN 'ready'
                            ELSE 'failed' END,
               size_bytes = CASE WHEN NEW.status = 'succeeded' THEN (NEW.output ->> 'size_bytes')::bigint END,
               parts = CASE WHEN NEW.status = 'succeeded' THEN COALESCE(NEW.output -> 'parts', '[]'::jsonb) ELSE parts END,
               manifest_sha256 = CASE WHEN NEW.status = 'succeeded' THEN NEW.output ->> 'manifest_sha256' END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the backup failed'), 500) END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('queued', 'running')
           AND NEW.status IN ('dispatched', 'succeeded', 'failed');
    ELSIF NEW.kind = 'restore_backup' THEN
        -- `restore:<restore id>`. What the node did is in its answer.
        UPDATE backup_restores
           SET status = CASE NEW.status
                            WHEN 'dispatched' THEN 'running'
                            WHEN 'succeeded' THEN 'done'
                            ELSE 'failed' END,
               result = CASE WHEN NEW.status = 'succeeded' THEN NEW.output END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the restore failed'), 500) END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('queued', 'running')
           AND NEW.status IN ('dispatched', 'succeeded', 'failed');
    ELSIF NEW.kind = 'export_backup' THEN
        -- `export:<export id>`. The file's name and size are in the answer.
        UPDATE backup_exports
           SET status = CASE NEW.status
                            WHEN 'dispatched' THEN 'running'
                            WHEN 'succeeded' THEN 'ready'
                            ELSE 'failed' END,
               object = CASE WHEN NEW.status = 'succeeded' THEN NEW.output ->> 'object' END,
               size_bytes = CASE WHEN NEW.status = 'succeeded' THEN (NEW.output ->> 'size_bytes')::bigint END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the export failed'), 500) END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('queued', 'running')
           AND NEW.status IN ('dispatched', 'succeeded', 'failed');
    ELSIF NEW.kind = 'import_export' THEN
        -- `import:<backup id>`: the unpacked export is the backup, ready to be restored.
        UPDATE backups
           SET status = CASE NEW.status
                            WHEN 'dispatched' THEN 'importing'
                            WHEN 'succeeded' THEN 'ready'
                            ELSE 'failed' END,
               size_bytes = CASE WHEN NEW.status = 'succeeded' THEN (NEW.output ->> 'size_bytes')::bigint END,
               parts = CASE WHEN NEW.status = 'succeeded' THEN COALESCE(NEW.output -> 'parts', '[]'::jsonb) ELSE parts END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the import failed'), 500) END,
               finished_at = CASE WHEN NEW.status IN ('succeeded', 'failed') THEN now() END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status IN ('importing', 'import_queued')
           AND NEW.status IN ('dispatched', 'succeeded', 'failed');
    ELSIF NEW.kind = 'delete_backup' AND NEW.status IN ('succeeded', 'failed') THEN
        -- `backup-delete:<backup id>`. A failed removal leaves the backup ready, so that the next
        -- retention run tries again.
        UPDATE backups
           SET status = CASE WHEN NEW.status = 'succeeded' THEN 'deleted' ELSE 'ready' END,
               error = CASE WHEN NEW.status = 'failed'
                            THEN left(COALESCE(NEW.error ->> 'message', 'the removal failed'), 500) END,
               updated_at = now()
         WHERE id::text = split_part(NEW.idempotency_key, ':', 2)
           AND status = 'deleting';
    ELSIF NEW.kind IN ('set_site_routes', 'set_custom_certificates', 'attach_domain',
                       'keep_domains', 'set_php_versions', 'set_php_ini')
          AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;

