-- Feature sets and plans (T04.01).
--
-- A feature set is a named list of feature keys (the keys modules declare in
-- their manifests). A plan is what a hosting account is given: a feature set,
-- resource limits for its containers, numeric limits of modules, and how
-- strictly it is isolated. A plan references exactly one feature set.
--
-- Feature keys are checked against the installed modules when a feature set is
-- written through the API; the table only checks their shape, so a key of a
-- module that is switched off or removed stays in the list and enables nothing.
-- A feature key as modules declare it. A domain, so that every element of an array is checked.
CREATE DOMAIN feature_key AS text
    CONSTRAINT feature_key_format CHECK (VALUE ~ '^[a-z][a-z0-9_]{0,47}$');

CREATE TABLE feature_sets (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    name         text        NOT NULL
        CONSTRAINT feature_sets_name_format CHECK (name ~ '^[a-z][a-z0-9_]{1,31}$'),
    description  text        NOT NULL DEFAULT ''
        CONSTRAINT feature_sets_description_len CHECK (length(description) <= 255),
    -- Every feature of every module, including ones added later. `features` is then empty.
    all_features boolean     NOT NULL DEFAULT false,
    features     feature_key[] NOT NULL DEFAULT '{}'
        CONSTRAINT feature_sets_features_count CHECK (cardinality(features) <= 256),
    -- Created by the installation; cannot be deleted through the API.
    built_in     boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT feature_sets_all_or_list CHECK (NOT all_features OR cardinality(features) = 0)
);

CREATE UNIQUE INDEX feature_sets_name_key ON feature_sets (name);

CREATE TRIGGER feature_sets_set_updated_at
    BEFORE UPDATE ON feature_sets
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE plans (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    name             text        NOT NULL
        CONSTRAINT plans_name_format CHECK (name ~ '^[a-z][a-z0-9_]{1,31}$'),
    description      text        NOT NULL DEFAULT ''
        CONSTRAINT plans_description_len CHECK (length(description) <= 255),
    feature_set_id   uuid        NOT NULL REFERENCES feature_sets (id) ON DELETE RESTRICT,
    -- Container resources. NULL means unlimited (bounded by the node).
    cpu_millis       integer
        CONSTRAINT plans_cpu_range CHECK (cpu_millis BETWEEN 10 AND 256000),
    memory_bytes     bigint
        CONSTRAINT plans_memory_range CHECK (memory_bytes BETWEEN 6291456 AND 1099511627776),
    pids             integer
        CONSTRAINT plans_pids_range CHECK (pids BETWEEN 1 AND 32768),
    disk_bytes       bigint
        CONSTRAINT plans_disk_range CHECK (disk_bytes BETWEEN 1048576 AND 1125899906842624),
    inodes           bigint
        CONSTRAINT plans_inodes_range CHECK (inodes BETWEEN 1000 AND 1000000000),
    bandwidth_bytes  bigint
        CONSTRAINT plans_bandwidth_range CHECK (bandwidth_bytes >= 0),
    -- Numeric limits of modules, by limit key (`max_domains`, ...): zero or more, -1 = unlimited.
    limits           jsonb       NOT NULL DEFAULT '{}'
        CONSTRAINT plans_limits_object CHECK (jsonb_typeof(limits) = 'object')
        CONSTRAINT plans_limits_size CHECK (octet_length(limits::text) <= 8192),
    -- How strictly the account's containers are separated from each other.
    isolation        text        NOT NULL DEFAULT 'standard'
        CONSTRAINT plans_isolation CHECK (isolation IN ('standard', 'strict')),
    built_in         boolean     NOT NULL DEFAULT false,
    archived_at      timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX plans_name_key ON plans (name);
CREATE INDEX plans_feature_set_idx ON plans (feature_set_id);

CREATE TRIGGER plans_set_updated_at
    BEFORE UPDATE ON plans
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- The four presets. Feature keys here must match what the modules declare
-- (checked when those modules arrive).
INSERT INTO feature_sets (name, description, all_features, features, built_in) VALUES
    ('default', 'Everything the panel offers', true, '{}', true),
    ('basic', 'The essentials of a website',  false,
        ARRAY['domains', 'redirects', 'dns', 'ssl', 'files', 'ftp', 'databases', 'backups', 'cron', 'logs']::feature_key[], true),
    ('dns_only', 'DNS hosting only', false, ARRAY['dns']::feature_key[], true),
    ('db_only', 'Databases only', false, ARRAY['databases']::feature_key[], true);

INSERT INTO plans (name, description, feature_set_id, cpu_millis, memory_bytes, pids, disk_bytes, inodes, bandwidth_bytes, limits, built_in)
SELECT p.name, p.description, f.id, p.cpu, p.mem, p.pids, p.disk, p.inodes, p.bw, p.limits::jsonb, true
FROM (VALUES
    ('default',  'Generous resources, nothing switched off',  2000, 2147483648, 512,  21474836480, 500000, NULL::bigint,    '{"max_domains": -1, "max_databases": -1}'),
    ('basic',    'A small website',                           500,  536870912,  128,  5368709120,  100000, 107374182400,    '{"max_domains": 5, "max_databases": 3}'),
    ('dns_only', 'DNS hosting, no containers',                NULL::integer, NULL::bigint, NULL::integer, 1073741824, 10000, NULL::bigint, '{"max_domains": 50}'),
    ('db_only',  'Databases, no website',                     250,  268435456,  64,   2147483648,  50000,  NULL::bigint,    '{"max_databases": 10}')
) AS p (name, description, cpu, mem, pids, disk, inodes, bw, limits)
JOIN feature_sets f ON f.name = p.name;
