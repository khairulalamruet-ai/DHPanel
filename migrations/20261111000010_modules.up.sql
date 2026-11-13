-- Modules known to this installation (T03.03).
--
-- The code decides which modules exist: at every start the control plane
-- writes the modules it was built with into this table. The table adds what
-- only an installation can know: whether an administrator switched a module
-- on or off.
--
-- A module that was here before but is not part of the running build is kept
-- and marked `missing` (its switch and history are not lost if it comes back).
CREATE TABLE modules (
    name           text        PRIMARY KEY
        CONSTRAINT modules_name_format CHECK (name ~ '^[a-z][a-z0-9-]{1,31}$'),
    version        text        NOT NULL
        CONSTRAINT modules_version_format CHECK (version ~ '^[0-9]+\.[0-9]+\.[0-9]+$'),
    category       text        NOT NULL
        CONSTRAINT modules_category CHECK (
            category IN ('core', 'web', 'db', 'mail', 'files', 'security', 'system', 'apps')
        ),
    -- A core module cannot be switched off.
    core           boolean     NOT NULL,
    enabled        boolean     NOT NULL DEFAULT true,
    status         text        NOT NULL DEFAULT 'present'
        CONSTRAINT modules_status CHECK (status IN ('present', 'missing')),
    -- The manifest as the build declared it (titles, dependencies, navigation, keys).
    manifest       jsonb       NOT NULL
        CONSTRAINT modules_manifest_object CHECK (jsonb_typeof(manifest) = 'object')
        CONSTRAINT modules_manifest_size CHECK (octet_length(manifest::text) <= 131072),
    first_seen_at  timestamptz NOT NULL DEFAULT now(),
    last_seen_at   timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT modules_core_is_enabled CHECK (NOT core OR enabled)
);

CREATE INDEX modules_status_idx ON modules (status);

CREATE TRIGGER modules_set_updated_at
    BEFORE UPDATE ON modules
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
