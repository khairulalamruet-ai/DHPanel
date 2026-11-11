-- Administrators can be required to use two-factor authentication (T01.13).
--
-- While `mfa_required` is set and the account has no confirmed second factor,
-- the account may sign in but every permission-protected endpoint answers
-- 403 `mfa_setup_required`; only the account's own security endpoints work, so
-- it can enrol a factor and then continue.
ALTER TABLE accounts ADD COLUMN mfa_required boolean NOT NULL DEFAULT false;
