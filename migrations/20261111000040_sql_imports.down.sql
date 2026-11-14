DROP TRIGGER operations_apply_to_tenant ON operations;
CREATE OR REPLACE FUNCTION operations_apply_to_tenant() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.tenant_id IS NULL THEN
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

CREATE TRIGGER operations_apply_to_tenant
    AFTER UPDATE OF status ON operations
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status AND NEW.status IN ('succeeded', 'failed'))
    EXECUTE FUNCTION operations_apply_to_tenant();

DROP TABLE IF EXISTS db_imports;
