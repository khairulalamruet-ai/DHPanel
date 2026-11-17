DROP TABLE IF EXISTS billing_notices;
DROP TABLE IF EXISTS payment_events;
DROP INDEX IF EXISTS subscriptions_live_license_idx;
DROP INDEX IF EXISTS subscriptions_reference_idx;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS state_since;
