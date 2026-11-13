-- Settings (T03.08).
--
-- One row per stored setting. Each module describes its settings with a JSON
-- Schema; the service validates every value against it before writing, so the
-- table only ever holds what the schema allows. A setting without a row has
-- the default declared in the schema.
--
-- `scope` says what the value is for: the whole panel (`global`), one node, or
-- one tenant. `scope_id` is NULL exactly for `global`. There are no foreign
-- keys on `scope_id` (tenants arrive with phase 04, and a deleted node's
-- settings are removed by the node's own cleanup).
CREATE TABLE settings (
    scope       text        NOT NULL
        CONSTRAINT settings_scope CHECK (scope IN ('global', 'node', 'tenant')),
    scope_id    uuid,
    module      text        NOT NULL
        CONSTRAINT settings_module_format CHECK (module ~ '^[a-z][a-z0-9-]{1,31}$'),
    key         text        NOT NULL
        CONSTRAINT settings_key_format CHECK (key ~ '^[a-z][a-z0-9_]{0,63}$'),
    value       jsonb       NOT NULL
        CONSTRAINT settings_value_size CHECK (octet_length(value::text) <= 16384),
    updated_by  uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT settings_scope_id CHECK ((scope = 'global') = (scope_id IS NULL))
);

-- One value per setting and scope. NULLs are equal here, so global settings are unique too.
CREATE UNIQUE INDEX settings_unique ON settings (scope, scope_id, module, key) NULLS NOT DISTINCT;
CREATE INDEX settings_module_idx ON settings (module, scope);
