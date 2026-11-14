-- T05.14b: which tenants need their web routes (re)sent to their node.
--
-- A tenant's routes (its domains, aliases, redirects, and whether it is
-- suspended) are computed from the tables and sent to the node as one operation.
-- Two numbers say whether that is up to date: `routes_version` goes up whenever
-- anything the routes are made of changes (a trigger on tenants, domains and
-- redirects), `routes_synced_version` is the version the last operation was
-- queued for. They differ while there is work to do. If the operation fails for
-- good, the synced version is reset so the work is queued again.
ALTER TABLE tenants
    ADD COLUMN routes_version        bigint NOT NULL DEFAULT 1,
    ADD COLUMN routes_synced_version bigint NOT NULL DEFAULT 0;

CREATE INDEX tenants_routes_pending_idx ON tenants (id) WHERE routes_version <> routes_synced_version;

-- A change of the tenant's own status (activated, suspended, deleted).
CREATE FUNCTION tenants_routes_status_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status THEN
        NEW.routes_version := OLD.routes_version + 1;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tenants_routes_status_changed
    BEFORE UPDATE OF status ON tenants
    FOR EACH ROW EXECUTE FUNCTION tenants_routes_status_changed();

-- A change of a domain (added, suspended, docroot, removed) or of its redirects.
CREATE FUNCTION domains_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tenants SET routes_version = routes_version + 1
     WHERE id = COALESCE(NEW.tenant_id, OLD.tenant_id);
    RETURN NULL;
END;
$$;

CREATE TRIGGER domains_routes_changed
    AFTER INSERT OR UPDATE OR DELETE ON domains
    FOR EACH ROW EXECUTE FUNCTION domains_routes_changed();

CREATE FUNCTION redirects_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tenants SET routes_version = routes_version + 1
     WHERE id = (SELECT tenant_id FROM domains WHERE id = COALESCE(NEW.domain_id, OLD.domain_id));
    RETURN NULL;
END;
$$;

CREATE TRIGGER redirects_routes_changed
    AFTER INSERT OR UPDATE OR DELETE ON redirects
    FOR EACH ROW EXECUTE FUNCTION redirects_routes_changed();

-- An operation that sets routes and failed for good: queue the work again.
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
    ELSIF NEW.kind = 'set_site_routes' AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;
