-- T05.16: the certificate each domain is served with.
--
-- `acme` certificates are obtained by the node's web server (Caddy); the node
-- reports what it holds, the control plane keeps the latest per domain. `custom`
-- certificates are uploaded by an administrator or the owner (T05.17); the node
-- reports never overwrite those. One row per domain: it describes the
-- certificate in use, not a history.
--
-- Nothing secret is stored here: only who issued the certificate and when it is
-- valid. Private keys never leave the node (ACME) or are kept encrypted
-- elsewhere (custom).
CREATE TABLE certificates (
    id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
    domain_id          uuid        NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    kind               text        NOT NULL DEFAULT 'acme'
        CONSTRAINT certificates_kind CHECK (kind IN ('acme', 'custom')),
    issuer             text        NOT NULL
        CONSTRAINT certificates_issuer_len CHECK (length(issuer) BETWEEN 1 AND 200),
    not_before         timestamptz NOT NULL,
    not_after          timestamptz NOT NULL,
    -- The node that reported it, and when it last did.
    source_node_id     uuid        REFERENCES nodes (id) ON DELETE SET NULL,
    last_seen_at       timestamptz NOT NULL DEFAULT now(),
    -- Set when the "expires soon" notice for this certificate was sent; a renewed
    -- certificate (another `not_after`) starts again.
    expiry_notified_at timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT certificates_validity CHECK (not_after > not_before)
);

CREATE UNIQUE INDEX certificates_domain_key ON certificates (domain_id);
CREATE INDEX certificates_expiry_idx ON certificates (not_after) WHERE expiry_notified_at IS NULL;

CREATE TRIGGER certificates_set_updated_at
    BEFORE UPDATE ON certificates
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
