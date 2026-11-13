-- The transactional outbox (T03.05).
--
-- A module that changes something other modules care about writes an event
-- here *in the same transaction* as the change. If the transaction rolls back,
-- the event was never written; if it commits, the event will be delivered. A
-- relay reads undelivered rows and hands each event to every enabled module.
--
-- Delivery is at-least-once: a handler may see an event again after a crash
-- or a failed attempt, so handlers must be safe to repeat.
CREATE TABLE outbox (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- Wire name of the event (`tenant_created`, ...).
    event_type       text        NOT NULL
        CONSTRAINT outbox_event_type_format CHECK (event_type ~ '^[a-z][a-z0-9_]{1,62}$'),
    -- The whole event as JSON (including its type).
    payload          jsonb       NOT NULL
        CONSTRAINT outbox_payload_object CHECK (jsonb_typeof(payload) = 'object')
        CONSTRAINT outbox_payload_size CHECK (octet_length(payload::text) <= 65536),
    created_at       timestamptz NOT NULL DEFAULT now(),
    -- Failed deliveries so far, and when the next one may be tried.
    attempts         integer     NOT NULL DEFAULT 0
        CONSTRAINT outbox_attempts_range CHECK (attempts >= 0),
    next_attempt_at  timestamptz NOT NULL DEFAULT now(),
    last_error       text
        CONSTRAINT outbox_last_error_len CHECK (length(last_error) <= 2000),
    -- Exactly one of these is set when the relay is done with the row:
    -- delivered to every module, or given up on after too many failures.
    delivered_at     timestamptz,
    dead_at          timestamptz,
    CONSTRAINT outbox_one_ending CHECK (delivered_at IS NULL OR dead_at IS NULL)
);

-- What the relay scans: events still to deliver, oldest first.
CREATE INDEX outbox_pending_idx ON outbox (next_attempt_at, id)
    WHERE delivered_at IS NULL AND dead_at IS NULL;
-- For the administrator's view of events that could not be delivered.
CREATE INDEX outbox_dead_idx ON outbox (dead_at) WHERE dead_at IS NOT NULL;
