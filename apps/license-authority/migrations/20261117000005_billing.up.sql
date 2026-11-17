-- T17.18-T17.20: payments, the subscription's life, and what the customer is told.
-- When the subscription entered its state: the dunning clocks run from it.
ALTER TABLE subscriptions ADD COLUMN state_since timestamptz NOT NULL DEFAULT now();
-- A reference names one subscription, whichever provider it came from.
CREATE UNIQUE INDEX subscriptions_reference_idx ON subscriptions (provider_reference)
    WHERE provider_reference IS NOT NULL;
-- A license has one subscription that is not over.
CREATE UNIQUE INDEX subscriptions_live_license_idx ON subscriptions (license_id)
    WHERE state NOT IN ('cancelled', 'expired');

-- Every payment event the provider sent, once: a repeat is answered, not applied again.
CREATE TABLE payment_events (
    event_id        text        PRIMARY KEY CHECK (length(event_id) BETWEEN 1 AND 200),
    kind            text        NOT NULL CHECK (length(kind) <= 60),
    subscription_id uuid        REFERENCES subscriptions (id),
    outcome         text        NOT NULL CHECK (length(outcome) <= 60),
    received_at     timestamptz NOT NULL DEFAULT now()
);

-- What the customer is to be told, until something has delivered it.
CREATE TABLE billing_notices (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id uuid        NOT NULL REFERENCES subscriptions (id),
    license_id      uuid        NOT NULL REFERENCES licenses (id),
    kind            text        NOT NULL CHECK (kind IN
        ('activated', 'renewed', 'renewal_upcoming', 'payment_failed', 'grace_started',
         'suspended', 'cancelled', 'expired')),
    -- Set for notices that must not be made twice (one reminder per period).
    dedupe_key      text,
    detail          jsonb       NOT NULL DEFAULT '{}',
    created_at      timestamptz NOT NULL DEFAULT now(),
    delivered_at    timestamptz
);
CREATE UNIQUE INDEX billing_notices_dedupe_idx ON billing_notices (subscription_id, dedupe_key)
    WHERE dedupe_key IS NOT NULL;
CREATE INDEX billing_notices_pending_idx ON billing_notices (created_at) WHERE delivered_at IS NULL;
