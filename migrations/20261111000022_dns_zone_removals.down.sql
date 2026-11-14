DROP TRIGGER IF EXISTS dns_zones_queue_removal ON dns_zones;
DROP FUNCTION IF EXISTS dns_zones_queue_removal();
DROP TABLE IF EXISTS dns_zone_removals;
