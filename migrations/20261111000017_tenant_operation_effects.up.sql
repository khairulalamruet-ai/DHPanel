-- T04.12: what a finished operation does to its tenant.
--
-- The control plane asks, the node does, and the operations ledger records how it
-- ended. The tenant's status follows the ledger in the same transaction that
-- records the end, whichever path got there (the node answered, the operation
-- expired, the node stayed silent), so the two can never disagree.
--
--   set_up_tenant   succeeded -> tenant `active`;  failed -> `failed`
--   suspend_tenant  succeeded -> `suspended`   (only from `active`)
--   resume_tenant   succeeded -> `active`      (only from `suspended`)
--   delete_tenant   succeeded with purge_data -> the tenant row is removed
--                   (only from `deleting`); the ledger keeps the history.
CREATE FUNCTION operations_apply_to_tenant() RETURNS trigger
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

CREATE TRIGGER operations_apply_to_tenant
    AFTER UPDATE OF status ON operations
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status AND NEW.status IN ('succeeded', 'failed'))
    EXECUTE FUNCTION operations_apply_to_tenant();
