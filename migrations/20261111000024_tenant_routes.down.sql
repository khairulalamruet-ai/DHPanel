-- Back to the function of migration 17.
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
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS redirects_routes_changed ON redirects;
DROP FUNCTION IF EXISTS redirects_routes_changed();
DROP TRIGGER IF EXISTS domains_routes_changed ON domains;
DROP FUNCTION IF EXISTS domains_routes_changed();
DROP TRIGGER IF EXISTS tenants_routes_status_changed ON tenants;
DROP FUNCTION IF EXISTS tenants_routes_status_changed();
DROP INDEX IF EXISTS tenants_routes_pending_idx;
ALTER TABLE tenants
    DROP COLUMN IF EXISTS routes_version,
    DROP COLUMN IF EXISTS routes_synced_version;
