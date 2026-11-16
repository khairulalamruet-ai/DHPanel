DELETE FROM integrity_events WHERE kind = 'binding';
ALTER TABLE integrity_events DROP CONSTRAINT integrity_events_kind_check;
ALTER TABLE integrity_events ADD CONSTRAINT integrity_events_kind_check
    CHECK (kind IN ('mismatch', 'stale', 'unverifiable', 'recovered', 'note'));
ALTER TABLE licenses DROP COLUMN bound_ips, DROP COLUMN bound_hostnames, DROP COLUMN bound_domains;
