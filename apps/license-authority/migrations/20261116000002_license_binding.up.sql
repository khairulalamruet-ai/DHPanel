-- A license is bound to where it may be used: its domains, host names and public addresses.
-- A license bound to nothing cannot be activated (the authority refuses it).
ALTER TABLE licenses
    ADD COLUMN bound_domains   text[] NOT NULL DEFAULT '{}' CHECK (cardinality(bound_domains) <= 50),
    ADD COLUMN bound_hostnames text[] NOT NULL DEFAULT '{}' CHECK (cardinality(bound_hostnames) <= 50),
    ADD COLUMN bound_ips       text[] NOT NULL DEFAULT '{}' CHECK (cardinality(bound_ips) <= 50);

-- A request that did not come from where the license is bound is an event of its own.
ALTER TABLE integrity_events DROP CONSTRAINT integrity_events_kind_check;
ALTER TABLE integrity_events ADD CONSTRAINT integrity_events_kind_check
    CHECK (kind IN ('mismatch', 'stale', 'unverifiable', 'recovered', 'note', 'binding'));
