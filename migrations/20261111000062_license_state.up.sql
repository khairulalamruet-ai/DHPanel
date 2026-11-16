-- T17.09: what this installation knows about its license. One row. `stored` is what the license
-- client keeps (the entitlement the authority signed, when it was last confirmed, a refusal);
-- the columns below it are the verdict of the last evaluation, so that every part of the panel
-- (the restricted-mode middleware, the plan and node limits) reads one cheap row instead of
-- verifying signatures.
CREATE TABLE license_state (
    id            boolean     PRIMARY KEY DEFAULT true CHECK (id),
    -- Licensing is on: an authority is configured.
    enforced      boolean     NOT NULL DEFAULT false,
    stored        jsonb       NOT NULL DEFAULT '{}',
    -- unlicensed | active | grace | restricted
    mode          text        NOT NULL DEFAULT 'unlicensed' CHECK (mode IN ('unlicensed', 'active', 'grace', 'restricted')),
    reason        text,
    features      text[]      NOT NULL DEFAULT '{}',
    -- Numeric limits of the entitlement (-1: none).
    limits        jsonb       NOT NULL DEFAULT '{}',
    license_id    uuid,
    plan          text,
    license_status text,
    license_expires_at timestamptz,
    entitlement_expires_at timestamptz,
    grace_ends_at timestamptz,
    last_error    text        CHECK (last_error IS NULL OR length(last_error) <= 500),
    evaluated_at  timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
INSERT INTO license_state (id) VALUES (true);
