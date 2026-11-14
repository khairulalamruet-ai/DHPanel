-- T05.08: DNS zones of hosted domains, their records, and the templates new
-- zones start from.
--
-- The panel keeps its own copy of every zone (what the person sees and edits);
-- the DNS server (PowerDNS) is told about changes through an adapter (T05.09).
-- `status` says how far that has got: `pending` (not pushed yet), `active`
-- (the server has it), `error` (the last push failed; `last_error` says why).
--
-- Record names are relative to the zone (`@` is the zone itself). The SOA and
-- the zone's own NS records belong to the server and are not stored here.
CREATE TABLE dns_zones (
    id          uuid        PRIMARY KEY DEFAULT uuidv7(),
    domain_id   uuid        NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    -- The zone's name: the domain's, lower-case ASCII.
    name        text        NOT NULL
        CONSTRAINT dns_zones_name_format CHECK (
            length(name) <= 253
            AND name ~ '^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$'
        ),
    provider    text        NOT NULL DEFAULT 'powerdns'
        CONSTRAINT dns_zones_provider CHECK (provider IN ('powerdns')),
    status      text        NOT NULL DEFAULT 'pending'
        CONSTRAINT dns_zones_status CHECK (status IN ('pending', 'active', 'error')),
    last_error  text
        CONSTRAINT dns_zones_last_error_len CHECK (length(last_error) <= 1000),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX dns_zones_domain_key ON dns_zones (domain_id);
CREATE UNIQUE INDEX dns_zones_name_key ON dns_zones (name);
CREATE INDEX dns_zones_status_idx ON dns_zones (status) WHERE status <> 'active';

CREATE TRIGGER dns_zones_set_updated_at
    BEFORE UPDATE ON dns_zones
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE dns_records (
    id          uuid        PRIMARY KEY DEFAULT uuidv7(),
    zone_id     uuid        NOT NULL REFERENCES dns_zones (id) ON DELETE CASCADE,
    name        text        NOT NULL
        CONSTRAINT dns_records_name_format CHECK (
            name = '@'
            OR (length(name) <= 200
                AND name ~ '^(\*\.)?[a-z0-9_]([a-z0-9_-]{0,61}[a-z0-9_])?(\.[a-z0-9_]([a-z0-9_-]{0,61}[a-z0-9_])?)*$')
        ),
    rtype       text        NOT NULL
        CONSTRAINT dns_records_type CHECK (rtype IN ('A', 'AAAA', 'CNAME', 'MX', 'TXT', 'SRV', 'CAA', 'NS')),
    content     text        NOT NULL
        CONSTRAINT dns_records_content_len CHECK (length(content) BETWEEN 1 AND 2100),
    ttl         integer     NOT NULL DEFAULT 3600
        CONSTRAINT dns_records_ttl CHECK (ttl BETWEEN 60 AND 604800),
    priority    integer
        CONSTRAINT dns_records_priority CHECK (priority BETWEEN 0 AND 65535),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT dns_records_priority_by_type CHECK ((rtype IN ('MX', 'SRV')) = (priority IS NOT NULL)),
    CONSTRAINT dns_records_no_cname_at_apex CHECK (NOT (rtype = 'CNAME' AND name = '@')),
    CONSTRAINT dns_records_no_apex_ns CHECK (NOT (rtype = 'NS' AND name = '@'))
);

CREATE INDEX dns_records_zone_idx ON dns_records (zone_id, name, rtype);
-- The very same record twice is refused (two A records at a name are fine).
CREATE UNIQUE INDEX dns_records_unique ON dns_records (zone_id, name, rtype, content, (COALESCE(priority, -1)));

CREATE TRIGGER dns_records_set_updated_at
    BEFORE UPDATE ON dns_records
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Zone templates: the records a new zone starts with. `records` is a JSON list
-- of {name, type, content, ttl?, priority?}; `content` may use {{domain}},
-- {{ipv4}} and {{ipv6}}, and a record whose variable is unknown for the
-- server (no IPv6 address, say) is left out. Checked by the application.
CREATE TABLE dns_zone_templates (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    name         text        NOT NULL
        CONSTRAINT dns_zone_templates_name_format CHECK (name ~ '^[a-z][a-z0-9_]{1,31}$'),
    description  text        NOT NULL DEFAULT ''
        CONSTRAINT dns_zone_templates_description_len CHECK (length(description) <= 255),
    records      jsonb       NOT NULL DEFAULT '[]'
        CONSTRAINT dns_zone_templates_records_array CHECK (jsonb_typeof(records) = 'array')
        CONSTRAINT dns_zone_templates_records_size CHECK (jsonb_array_length(records) <= 100),
    is_default   boolean     NOT NULL DEFAULT false,
    built_in     boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX dns_zone_templates_name_key ON dns_zone_templates (name);
CREATE UNIQUE INDEX dns_zone_templates_one_default ON dns_zone_templates (is_default) WHERE is_default;

CREATE TRIGGER dns_zone_templates_set_updated_at
    BEFORE UPDATE ON dns_zone_templates
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

INSERT INTO dns_zone_templates (name, description, records, is_default, built_in) VALUES (
    'default',
    'The site on this server, www as an alias, mail to the domain itself, and an SPF policy',
    '[
      {"name": "@",   "type": "A",     "content": "{{ipv4}}",           "ttl": 3600},
      {"name": "@",   "type": "AAAA",  "content": "{{ipv6}}",           "ttl": 3600},
      {"name": "www", "type": "CNAME", "content": "{{domain}}.",        "ttl": 3600},
      {"name": "@",   "type": "MX",    "content": "{{domain}}.",        "ttl": 3600, "priority": 10},
      {"name": "@",   "type": "TXT",   "content": "v=spf1 a mx ~all",   "ttl": 3600}
    ]'::jsonb,
    true,
    true
);
