-- The trigger names a column that is about to go: put the plain one back first.
DROP TRIGGER IF EXISTS dns_zones_set_updated_at ON dns_zones;
CREATE TRIGGER dns_zones_set_updated_at
    BEFORE UPDATE ON dns_zones
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

ALTER TABLE dns_zones
    DROP COLUMN IF EXISTS ns_digest,
    DROP COLUMN IF EXISTS delegation,
    DROP COLUMN IF EXISTS delegation_checked_at;
