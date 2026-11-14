-- T06.14: a temporary address for a site, `<random>.<preview domain>`, to look at
-- it before its own name points here. The link works until `expires_at`; the
-- dispatcher removes expired rows (which makes the tenant's routes out of date, so
-- the web server stops answering for the name).
CREATE TABLE preview_links (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    domain_id  uuid        NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    host       text        NOT NULL UNIQUE
        CHECK (length(host) <= 253 AND host = lower(host)),
    expires_at timestamptz NOT NULL,
    created_by uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX preview_links_domain_idx ON preview_links (domain_id);
CREATE INDEX preview_links_expiry_idx ON preview_links (expires_at);

CREATE FUNCTION preview_links_routes_changed() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE tenants SET routes_version = routes_version + 1
     WHERE id = COALESCE(NEW.tenant_id, OLD.tenant_id);
    RETURN NULL;
END;
$$;

CREATE TRIGGER preview_links_routes_changed
    AFTER INSERT OR UPDATE OR DELETE ON preview_links
    FOR EACH ROW EXECUTE FUNCTION preview_links_routes_changed();
