-- One-time tokens that let a new server enrol as a node (T02.05).
--
-- An administrator creates a token for a planned node name; the install
-- command carries it to the server; the agent presents it once together with
-- its CSR (T02.06). Only the SHA-256 of the token is stored, so a database leak
-- does not reveal usable tokens (they carry 256 bits of randomness, so a fast
-- hash is enough). A token ends in exactly one of three ways: it is consumed,
-- it is revoked, or it expires; none of them can be undone.
CREATE TABLE node_enrollment_tokens (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- The name the node will get; checked against nodes at enrolment too.
    name          text        NOT NULL
        CONSTRAINT node_enrollment_tokens_name_format CHECK (name ~ '^[a-z][a-z0-9-]{1,62}$'),
    token_sha256  bytea       NOT NULL
        CONSTRAINT node_enrollment_tokens_hash_len CHECK (octet_length(token_sha256) = 32),
    created_by    uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL,
    consumed_at   timestamptz,
    revoked_at    timestamptz,
    -- The node that was enrolled with this token.
    node_id       uuid        REFERENCES nodes (id) ON DELETE SET NULL,
    CONSTRAINT node_enrollment_tokens_expiry CHECK (expires_at > created_at),
    CONSTRAINT node_enrollment_tokens_one_ending CHECK (consumed_at IS NULL OR revoked_at IS NULL),
    CONSTRAINT node_enrollment_tokens_node_needs_consumption CHECK (node_id IS NULL OR consumed_at IS NOT NULL)
);

CREATE UNIQUE INDEX node_enrollment_tokens_hash_key ON node_enrollment_tokens (token_sha256);
CREATE INDEX node_enrollment_tokens_created_idx ON node_enrollment_tokens (created_at DESC, id DESC);
CREATE INDEX node_enrollment_tokens_pending_name_idx ON node_enrollment_tokens (name)
    WHERE consumed_at IS NULL AND revoked_at IS NULL;

-- Viewing nodes is allowed to administrators and support staff; changing them
-- (admin.nodes.manage) stays with super administrators, who hold `*`.
INSERT INTO role_permissions (role_id, permission)
SELECT r.id, 'admin.nodes.read'
FROM roles r
WHERE r.name IN ('admin', 'support');
