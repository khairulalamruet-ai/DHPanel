-- Back to the function of migration 30.
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
    ELSIF NEW.kind IN ('set_site_routes', 'set_custom_certificates', 'attach_domain', 'keep_domains')
          AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS domains_php_changed ON domains;
DROP FUNCTION IF EXISTS domains_php_changed();
DROP TRIGGER IF EXISTS tenants_php_changed ON tenants;
DROP FUNCTION IF EXISTS tenants_php_changed();
ALTER TABLE domains DROP CONSTRAINT IF EXISTS domains_php_version_format;
ALTER TABLE tenants
    DROP COLUMN IF EXISTS php_applied_version,
    DROP COLUMN IF EXISTS php_state_version,
    DROP COLUMN IF EXISTS php_version;
