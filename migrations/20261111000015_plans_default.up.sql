-- One plan is the default for new accounts (T04.03). The `default` preset starts as it.
ALTER TABLE plans ADD COLUMN is_default boolean NOT NULL DEFAULT false;
CREATE UNIQUE INDEX plans_one_default ON plans (is_default) WHERE is_default;
UPDATE plans SET is_default = true WHERE name = 'default';
