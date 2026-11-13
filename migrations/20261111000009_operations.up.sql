-- The operations ledger (T02.10).
--
-- Every action the control plane asks a node to perform is a row here before
-- it is sent, so that nothing is lost when a process restarts, a node is
-- offline, or a request is retried.
--
--   pending ──► dispatched ──► succeeded
--      ▲            │  └─────► failed
--      │            └───────► retryable ──► dispatched (again) / failed
--      └─ (pending is also where a not-yet-delivered operation waits)
--
-- `succeeded` and `failed` are final. The statement that wants a transition
-- names the status it expects, and a trigger refuses every other move, so two
-- workers can never both finish the same operation.
--
-- Idempotency: (node_id, idempotency_key) is unique. Asking again with the same
-- key returns the existing row (the service checks that it is the same request).
-- `tenant_id` has no foreign key yet: tenants arrive with phase 04.
CREATE TABLE operations (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- Wire name of the operation (`ping`, `report_capabilities`, ...).
    kind             text        NOT NULL
        CONSTRAINT operations_kind_format CHECK (kind ~ '^[a-z][a-z0-9_]{1,62}$'),
    tenant_id        uuid,
    node_id          uuid        NOT NULL REFERENCES nodes (id) ON DELETE RESTRICT,
    -- The typed operation as sent (never secrets: those travel by other means).
    payload          jsonb       NOT NULL
        CONSTRAINT operations_payload_object CHECK (jsonb_typeof(payload) = 'object')
        CONSTRAINT operations_payload_size CHECK (octet_length(payload::text) <= 65536),
    status           text        NOT NULL DEFAULT 'pending'
        CONSTRAINT operations_status CHECK (
            status IN ('pending', 'dispatched', 'succeeded', 'failed', 'retryable')
        ),
    -- Deliveries so far, and the most there may be.
    attempts         integer     NOT NULL DEFAULT 0
        CONSTRAINT operations_attempts_range CHECK (attempts >= 0),
    max_attempts     integer     NOT NULL DEFAULT 3
        CONSTRAINT operations_max_attempts_range CHECK (max_attempts BETWEEN 1 AND 20),
    idempotency_key  text        NOT NULL
        CONSTRAINT operations_idempotency_key_len CHECK (length(idempotency_key) BETWEEN 1 AND 128),
    -- What the agent produced, on success.
    output           jsonb,
    -- Why it failed: {"code": ..., "message": ..., "retryable": ...}.
    error            jsonb,
    created_by       uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    -- After this the operation is not sent any more (and the agent refuses it).
    expires_at       timestamptz NOT NULL,
    -- A retryable operation waits until then before it is sent again.
    next_attempt_at  timestamptz NOT NULL DEFAULT now(),
    dispatched_at    timestamptz,
    finished_at      timestamptz,
    CONSTRAINT operations_expiry CHECK (expires_at > created_at),
    CONSTRAINT operations_attempts_within_max CHECK (attempts <= max_attempts),
    CONSTRAINT operations_finished_iff_final CHECK (
        (status IN ('succeeded', 'failed')) = (finished_at IS NOT NULL)
    ),
    CONSTRAINT operations_output_only_on_success CHECK (output IS NULL OR status = 'succeeded'),
    -- The last failure stays visible while the operation is tried again.
    CONSTRAINT operations_no_error_on_success CHECK (error IS NULL OR status <> 'succeeded')
);

CREATE UNIQUE INDEX operations_idempotency_key ON operations (node_id, idempotency_key);
CREATE INDEX operations_node_idx ON operations (node_id, created_at DESC);
CREATE INDEX operations_status_idx ON operations (status, created_at DESC);
-- What the dispatcher scans.
CREATE INDEX operations_due_idx ON operations (next_attempt_at, created_at)
    WHERE status IN ('pending', 'retryable');

CREATE TRIGGER operations_set_updated_at
    BEFORE UPDATE ON operations
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Only the moves in the diagram above are possible, and what identifies the
-- operation never changes. Rows are never deleted: the ledger is the history.
CREATE FUNCTION operations_guard() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'operations are never deleted' USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.node_id IS DISTINCT FROM OLD.node_id
       OR NEW.payload IS DISTINCT FROM OLD.payload
       OR NEW.idempotency_key IS DISTINCT FROM OLD.idempotency_key
       OR NEW.max_attempts IS DISTINCT FROM OLD.max_attempts
       OR NEW.created_by IS DISTINCT FROM OLD.created_by
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'an operation''s identity cannot be changed' USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    IF OLD.status IN ('succeeded', 'failed') THEN
        RAISE EXCEPTION 'operation % is final (%)', OLD.id, OLD.status USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    IF NEW.status <> OLD.status AND NOT (
           (OLD.status = 'pending'    AND NEW.status IN ('dispatched', 'failed'))
        OR (OLD.status = 'dispatched' AND NEW.status IN ('succeeded', 'failed', 'retryable', 'pending'))
        OR (OLD.status = 'retryable'  AND NEW.status IN ('dispatched', 'failed'))
    ) THEN
        RAISE EXCEPTION 'operation % cannot go from % to %', OLD.id, OLD.status, NEW.status
            USING ERRCODE = 'integrity_constraint_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER operations_guard_update
    BEFORE UPDATE ON operations
    FOR EACH ROW EXECUTE FUNCTION operations_guard();
CREATE TRIGGER operations_guard_delete
    BEFORE DELETE ON operations
    FOR EACH ROW EXECUTE FUNCTION operations_guard();
