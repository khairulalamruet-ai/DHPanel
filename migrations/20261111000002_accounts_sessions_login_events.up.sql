-- T01.01: accounts, sessions and login history.

-- Keeps updated_at current on every UPDATE. Shared by later tables.
CREATE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END
$$;

CREATE TYPE account_kind AS ENUM ('admin', 'reseller', 'client');
CREATE TYPE account_status AS ENUM ('active', 'suspended', 'disabled');

-- Every person who can log in (admins, resellers, clients).
CREATE TABLE accounts (
    id                  uuid           PRIMARY KEY DEFAULT uuidv7(),
    kind                account_kind   NOT NULL,
    -- Stored normalised (lowercase); the format is enforced here as well as in code.
    username            text           NOT NULL
        CONSTRAINT accounts_username_format CHECK (username ~ '^[a-z][a-z0-9_-]{2,31}$'),
    email               text           NOT NULL
        CONSTRAINT accounts_email_format CHECK (
            email = lower(email)
            AND length(email) <= 254
            AND email ~ '^[^@[:space:]]+@[^@[:space:]]+\.[^@[:space:]]+$'
        ),
    -- PHC string (argon2id). Never a plain password.
    password_hash       text           NOT NULL
        CONSTRAINT accounts_password_hash_phc CHECK (password_hash LIKE '$argon2id$%'),
    status              account_status NOT NULL DEFAULT 'active',
    locale              text           NOT NULL DEFAULT 'en'
        CONSTRAINT accounts_locale_supported CHECK (locale IN ('en', 'bn')),
    password_changed_at timestamptz    NOT NULL DEFAULT now(),
    created_at          timestamptz    NOT NULL DEFAULT now(),
    updated_at          timestamptz    NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX accounts_username_key ON accounts (username);
CREATE UNIQUE INDEX accounts_email_key ON accounts (email);
CREATE INDEX accounts_kind_status_idx ON accounts (kind, status);

CREATE TRIGGER accounts_set_updated_at
    BEFORE UPDATE ON accounts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Durable session history. The live session lives in Redis; only a SHA-256
-- of the session token is stored here, never the token itself.
CREATE TABLE sessions (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    account_id     uuid        NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_sha256   bytea       NOT NULL
        CONSTRAINT sessions_token_sha256_len CHECK (octet_length(token_sha256) = 32),
    ip             inet,
    user_agent     text
        CONSTRAINT sessions_user_agent_len CHECK (length(user_agent) <= 512),
    created_at     timestamptz NOT NULL DEFAULT now(),
    last_seen_at   timestamptz NOT NULL DEFAULT now(),
    expires_at     timestamptz NOT NULL,
    revoked_at     timestamptz,
    revoke_reason  text
        CONSTRAINT sessions_revoke_reason CHECK (
            revoke_reason IN ('logout', 'logout_all', 'admin', 'password_changed', 'expired', 'security')
        ),
    CONSTRAINT sessions_revoked_has_reason CHECK ((revoked_at IS NULL) = (revoke_reason IS NULL)),
    CONSTRAINT sessions_expiry_after_creation CHECK (expires_at > created_at)
);

CREATE UNIQUE INDEX sessions_token_sha256_key ON sessions (token_sha256);
CREATE INDEX sessions_account_active_idx ON sessions (account_id, created_at DESC) WHERE revoked_at IS NULL;

-- Every login attempt, successful or not. account_id is NULL when the
-- username did not match an account (still recorded for rate limiting/abuse).
CREATE TABLE login_events (
    id                  uuid        PRIMARY KEY DEFAULT uuidv7(),
    account_id          uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    username_attempted  text        NOT NULL
        CONSTRAINT login_events_username_len CHECK (length(username_attempted) <= 254),
    ip                  inet        NOT NULL,
    user_agent          text
        CONSTRAINT login_events_user_agent_len CHECK (length(user_agent) <= 512),
    country             text
        CONSTRAINT login_events_country_iso CHECK (country ~ '^[A-Z]{2}$'),
    success             boolean     NOT NULL,
    reason              text        NOT NULL
        CONSTRAINT login_events_reason CHECK (reason IN (
            'ok', 'bad_credentials', 'account_suspended', 'account_disabled',
            'rate_limited', 'locked_out', 'mfa_required', 'mfa_failed'
        )),
    at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT login_events_success_reason CHECK (success = (reason = 'ok'))
);

CREATE INDEX login_events_account_at_idx ON login_events (account_id, at DESC);
CREATE INDEX login_events_ip_at_idx ON login_events (ip, at DESC);
CREATE INDEX login_events_failed_at_idx ON login_events (at DESC) WHERE NOT success;
