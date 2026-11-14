-- T05.17b: uploaded certificates travel to the node with the tenant's routes. A
-- change of one (added, replaced, removed) makes the tenant's routes out of date,
-- so the dispatcher sends both again.
CREATE FUNCTION certificates_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    target uuid;
    was_custom boolean := false;
    is_custom boolean := false;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        was_custom := OLD.kind = 'custom';
    END IF;
    IF TG_OP <> 'DELETE' THEN
        is_custom := NEW.kind = 'custom';
    END IF;
    IF was_custom OR is_custom THEN
        target := COALESCE(NEW.domain_id, OLD.domain_id);
        UPDATE tenants SET routes_version = routes_version + 1
         WHERE id = (SELECT tenant_id FROM domains WHERE id = target);
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER certificates_routes_changed
    AFTER INSERT OR UPDATE OR DELETE ON certificates
    FOR EACH ROW EXECUTE FUNCTION certificates_routes_changed();

-- A certificates operation that failed for good: queue the work again, like routes.
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
    ELSIF NEW.kind IN ('set_site_routes', 'set_custom_certificates') AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;
