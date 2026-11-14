-- T06.08: the PHP version of an account, and of a domain that wants another one.
--
-- `php_state_version` goes up whenever what the PHP containers should be changes
-- (the account's version, or a domain taking or leaving a version of its own);
-- `php_applied_version` is the state the node reported done. While they differ
-- the dispatcher sends the account's PHP containers first and only then the web
-- server files that point to them.
ALTER TABLE tenants
    ADD COLUMN php_version         text   NOT NULL DEFAULT '8.4'
        CONSTRAINT tenants_php_version_format CHECK (php_version ~ '^[0-9]{1,2}[.][0-9]{1,2}$'),
    ADD COLUMN php_state_version   bigint NOT NULL DEFAULT 1,
    ADD COLUMN php_applied_version bigint NOT NULL DEFAULT 1;

-- (domains.php_version exists since migration 20, empty so far.)
ALTER TABLE domains
    ADD CONSTRAINT domains_php_version_format
        CHECK (php_version IS NULL OR php_version ~ '^[0-9]{1,2}[.][0-9]{1,2}$');

-- The account's own version changed: the containers and the files both follow.
CREATE FUNCTION tenants_php_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.php_version IS DISTINCT FROM OLD.php_version THEN
        NEW.php_state_version := OLD.php_state_version + 1;
        NEW.routes_version := OLD.routes_version + 1;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tenants_php_changed
    BEFORE UPDATE OF php_version ON tenants
    FOR EACH ROW EXECUTE FUNCTION tenants_php_changed();

-- A domain took, changed or gave up a version of its own. (The files follow from
-- the trigger that makes the routes out of date for any change of a domain.)
CREATE FUNCTION domains_php_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF (TG_OP = 'INSERT' AND NEW.php_version IS NOT NULL)
       OR (TG_OP = 'UPDATE' AND NEW.php_version IS DISTINCT FROM OLD.php_version)
       OR (TG_OP = 'DELETE' AND OLD.php_version IS NOT NULL) THEN
        UPDATE tenants SET php_state_version = php_state_version + 1
         WHERE id = COALESCE(NEW.tenant_id, OLD.tenant_id);
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER domains_php_changed
    AFTER INSERT OR UPDATE OR DELETE ON domains
    FOR EACH ROW EXECUTE FUNCTION domains_php_changed();

-- The node's answer: done (the key carries the state it was asked for), or failed
-- for good (try again with the next pass).
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
    ELSIF NEW.kind = 'set_php_versions' AND NEW.status = 'succeeded' THEN
        -- `php:<tenant>:<state>:<slot>`
        UPDATE tenants
           SET php_applied_version = GREATEST(php_applied_version,
                                              split_part(NEW.idempotency_key, ':', 3)::bigint),
               routes_synced_version = 0
         WHERE id = NEW.tenant_id AND NEW.idempotency_key ~ '^php:[0-9a-f]+:[0-9]+:[0-9]+$';
    ELSIF NEW.kind IN ('set_site_routes', 'set_custom_certificates', 'attach_domain',
                       'keep_domains', 'set_php_versions')
          AND NEW.status = 'failed' THEN
        UPDATE tenants SET routes_synced_version = 0 WHERE id = NEW.tenant_id;
    END IF;
    RETURN NEW;
END;
$$;
