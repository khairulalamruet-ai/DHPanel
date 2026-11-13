-- Tenants (T04.05): one hosting account's footprint on a node.
--
-- A tenant belongs to one client account, lives on one node, is on one plan, and
-- has its own system user (name and numeric id on that node). Everything that
-- belongs to a tenant later (domains, databases, files, backups) refers to this
-- row, and deleting an account, node or plan that tenants still use is refused.
CREATE TABLE tenants (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    owner_account_id  uuid        NOT NULL REFERENCES accounts (id) ON DELETE RESTRICT,
    node_id           uuid        NOT NULL REFERENCES nodes (id) ON DELETE RESTRICT,
    plan_id           uuid        NOT NULL REFERENCES plans (id) ON DELETE RESTRICT,
    -- The system user name on the node: a-z and 0-9, starting with a letter.
    username          text        NOT NULL
        CONSTRAINT tenants_username_format CHECK (username ~ '^[a-z][a-z0-9]{2,31}$'),
    -- Numeric user and group id inside the agent's user namespace. Handed out
    -- per node from 10000 up to 59999 (crates/domain, `UidRange`).
    uid               integer     NOT NULL
        CONSTRAINT tenants_uid_range CHECK (uid BETWEEN 10000 AND 59999),
    status            text        NOT NULL DEFAULT 'provisioning'
        CONSTRAINT tenants_status CHECK (status IN ('provisioning', 'active', 'suspended', 'deleting', 'failed')),
    -- Set when deletion is requested; the data is purged after `purge_after`.
    deleted_at        timestamptz,
    purge_after       timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tenants_deleting_has_dates CHECK (
        (status = 'deleting') = (deleted_at IS NOT NULL AND purge_after IS NOT NULL)
    )
);

-- A system name and a numeric id are unique on their node, and a name is also
-- unique panel-wide so a tenant can move between nodes without renaming.
CREATE UNIQUE INDEX tenants_username_key ON tenants (username);
CREATE UNIQUE INDEX tenants_node_uid_key ON tenants (node_id, uid);
CREATE INDEX tenants_owner_idx ON tenants (owner_account_id);
CREATE INDEX tenants_plan_idx ON tenants (plan_id);
CREATE INDEX tenants_node_idx ON tenants (node_id, status);
CREATE INDEX tenants_purge_idx ON tenants (purge_after) WHERE status = 'deleting';

CREATE TRIGGER tenants_set_updated_at
    BEFORE UPDATE ON tenants
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Only accounts of kind `client` own tenants.
CREATE FUNCTION tenants_owner_is_client() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM accounts WHERE id = NEW.owner_account_id AND kind = 'client') THEN
        RAISE EXCEPTION 'a tenant must belong to a client account' USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER tenants_owner_check
    BEFORE INSERT OR UPDATE OF owner_account_id ON tenants
    FOR EACH ROW EXECUTE FUNCTION tenants_owner_is_client();
