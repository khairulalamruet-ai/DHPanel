-- T07.13: where a tenant's databases of each kind live: on the node's shared server, or on a
-- server of the tenant's own (when its plan has the `db_dedicated` feature). No row means the
-- shared server. A change of plan starts a move, which is one operation on the node; the row
-- says which move is running, and the state flips when that operation has succeeded.
CREATE TABLE tenant_db_servers (
    tenant_id    uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    engine       text        NOT NULL CHECK (engine IN ('mysql', 'postgres')),
    state        text        NOT NULL DEFAULT 'shared'
        CHECK (state IN ('shared', 'dedicated', 'moving_to_dedicated', 'moving_to_shared')),
    operation_id uuid,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, engine)
);
