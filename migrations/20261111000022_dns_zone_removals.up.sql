-- T05.09: zones that were deleted here and must be removed from the DNS server.
--
-- Deleting a domain deletes its zone row (and so its records) at once; the DNS
-- server is told afterwards by the sync job. This table is that to-do list. It
-- is filled by a trigger, so every way a zone can disappear (a domain removed,
-- a tenant purged, a cascade) is covered. The job skips a name that has a zone
-- again by the time it runs (a domain deleted and added back), and removes the
-- entry once the server no longer has the zone.
CREATE TABLE dns_zone_removals (
    name       text        PRIMARY KEY
        CONSTRAINT dns_zone_removals_name_len CHECK (length(name) <= 253),
    queued_at  timestamptz NOT NULL DEFAULT now()
);

CREATE FUNCTION dns_zones_queue_removal() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO dns_zone_removals (name) VALUES (OLD.name) ON CONFLICT (name) DO NOTHING;
    RETURN OLD;
END;
$$;

CREATE TRIGGER dns_zones_queue_removal
    AFTER DELETE ON dns_zones
    FOR EACH ROW EXECUTE FUNCTION dns_zones_queue_removal();
