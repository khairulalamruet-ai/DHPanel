-- T12.11: a domain's page cache. Most domains have none, so it is kept apart from
-- `domains`. `generation` goes up with every purge: it is part of each cache key,
-- so a purge is one number changing and the configuration being sent again.
CREATE TABLE domain_page_cache (
    domain_id   uuid PRIMARY KEY REFERENCES domains (id) ON DELETE CASCADE,
    enabled     boolean     NOT NULL DEFAULT true,
    ttl_minutes smallint    NOT NULL DEFAULT 10 CHECK (ttl_minutes BETWEEN 1 AND 1440),
    generation  integer     NOT NULL DEFAULT 1,
    purged_at   timestamptz,
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TRIGGER domain_page_cache_updated_at
    BEFORE UPDATE ON domain_page_cache
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE FUNCTION domain_page_cache_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tenants SET routes_version = routes_version + 1
     WHERE id = (SELECT tenant_id FROM domains WHERE id = COALESCE(NEW.domain_id, OLD.domain_id));
    RETURN NULL;
END;
$$;

CREATE TRIGGER domain_page_cache_routes_changed
    AFTER INSERT OR UPDATE OR DELETE ON domain_page_cache
    FOR EACH ROW EXECUTE FUNCTION domain_page_cache_routes_changed();
