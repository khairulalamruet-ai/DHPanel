-- The catalogue of permission keys (T03.06).
--
-- A permission key is declared by exactly one module, in its manifest. At every
-- start the control plane writes the keys of the running build into this table,
-- so that the role editor can offer the real list, grouped by module, without
-- anyone listing keys by hand. `role_permissions` stays a plain list of grants:
-- a role may still hold a key whose module is gone (it then grants nothing that
-- exists), and `super_admin` holds `*`, which covers every key, old or new.
CREATE TABLE permissions (
    key         text        PRIMARY KEY
        CONSTRAINT permissions_key_format CHECK (key ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$'),
    -- The module that declares it.
    module      text        NOT NULL
        CONSTRAINT permissions_module_format CHECK (module ~ '^[a-z][a-z0-9-]{1,31}$'),
    -- `missing` once the running build no longer declares it.
    status      text        NOT NULL DEFAULT 'present'
        CONSTRAINT permissions_status CHECK (status IN ('present', 'missing')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX permissions_module_idx ON permissions (module, key);

CREATE TRIGGER permissions_set_updated_at
    BEFORE UPDATE ON permissions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
