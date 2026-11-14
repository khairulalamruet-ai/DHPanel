-- T05.12: name servers follow the panel's settings, and a domain's delegation can be checked.
--
-- `ns_digest` is the list of name servers a zone was last pushed with. When the
-- administrator changes the name servers, every zone whose digest differs is
-- pushed again, so the zones the DNS server already has get the new apex NS
-- records too. `delegation` is the answer of the last check of what the public
-- DNS says about the domain's name servers (the checking is rate-limited by
-- `delegation_checked_at`).
ALTER TABLE dns_zones
    ADD COLUMN ns_digest text
        CONSTRAINT dns_zones_ns_digest_len CHECK (length(ns_digest) <= 2200),
    ADD COLUMN delegation jsonb
        CONSTRAINT dns_zones_delegation_object CHECK (jsonb_typeof(delegation) = 'object'),
    ADD COLUMN delegation_checked_at timestamptz;

-- A check of the delegation is not a change of the zone: it must not move the
-- zone's version (a push in progress would then think the zone changed).
DROP TRIGGER dns_zones_set_updated_at ON dns_zones;
CREATE TRIGGER dns_zones_set_updated_at
    BEFORE UPDATE OF domain_id, name, provider, status, last_error, ns_digest ON dns_zones
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
