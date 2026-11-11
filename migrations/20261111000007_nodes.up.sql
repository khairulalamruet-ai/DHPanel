-- Nodes and their mTLS certificates (T02.04).
--
-- `nodes` is one row per enrolled server. A row is created when an agent
-- enrols (T02.06), so there is no "pending" status: until then only the
-- enrollment token exists.
--
-- `node_certificates` is one row per certificate the CA issued to a node. A node
-- has several over its life (certificates are rotated before they expire), so
-- the identity used in the mTLS handshake is the certificate fingerprint, which
-- is unique here. The handshake accepts a connection only if the fingerprint
-- matches a row that is not revoked, not expired, and belongs to a node that is
-- not revoked.
CREATE TABLE nodes (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    name             text        NOT NULL
        CONSTRAINT nodes_name_format CHECK (name ~ '^[a-z][a-z0-9-]{1,62}$'),
    hostname         text
        CONSTRAINT nodes_hostname_len CHECK (length(hostname) BETWEEN 1 AND 253),
    public_ip        inet,
    status           text        NOT NULL DEFAULT 'offline'
        CONSTRAINT nodes_status CHECK (status IN ('online', 'offline', 'draining', 'revoked')),
    agent_version    text
        CONSTRAINT nodes_agent_version_len CHECK (length(agent_version) BETWEEN 1 AND 64),
    -- Latest ReportCapabilities answer (CPU, memory, disk, Docker mode, ...).
    capabilities     jsonb       NOT NULL DEFAULT '{}'
        CONSTRAINT nodes_capabilities_object CHECK (jsonb_typeof(capabilities) = 'object')
        CONSTRAINT nodes_capabilities_size CHECK (octet_length(capabilities::text) <= 16384),
    capabilities_at  timestamptz,
    last_seen_at     timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX nodes_name_key ON nodes (name);
CREATE INDEX nodes_status_idx ON nodes (status);

CREATE TRIGGER nodes_set_updated_at
    BEFORE UPDATE ON nodes
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE node_certificates (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    node_id             uuid        NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    -- 32 lowercase hex digits (128-bit serial); the CA never reuses one.
    serial              text        NOT NULL
        CONSTRAINT node_certificates_serial_format CHECK (serial ~ '^[0-9a-f]{32}$'),
    -- SHA-256 of the DER certificate, 64 lowercase hex digits.
    fingerprint_sha256  text        NOT NULL
        CONSTRAINT node_certificates_fingerprint_format CHECK (fingerprint_sha256 ~ '^[0-9a-f]{64}$'),
    not_before          timestamptz NOT NULL,
    not_after           timestamptz NOT NULL,
    issued_at           timestamptz NOT NULL DEFAULT now(),
    revoked_at          timestamptz,
    revoke_reason       text
        CONSTRAINT node_certificates_revoke_reason CHECK (
            revoke_reason IN ('admin', 'rotated', 'compromised', 'node_revoked')
        ),
    CONSTRAINT node_certificates_validity CHECK (not_after > not_before),
    CONSTRAINT node_certificates_revoked_has_reason CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL))
);

CREATE UNIQUE INDEX node_certificates_fingerprint_key ON node_certificates (fingerprint_sha256);
CREATE UNIQUE INDEX node_certificates_serial_key ON node_certificates (serial);
CREATE INDEX node_certificates_node_idx ON node_certificates (node_id, not_after DESC);
-- The handshake only ever looks at certificates that are still usable.
CREATE INDEX node_certificates_active_idx ON node_certificates (node_id) WHERE revoked_at IS NULL;
