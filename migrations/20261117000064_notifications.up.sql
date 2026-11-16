-- T18.03: notifications. An event that matters (a certificate that ends, a backup that failed, a
-- node that is offline, a node's service that is down) becomes a notification in the inbox of the
-- people it concerns; the channels they set up (an email address, a webhook) get it too.

CREATE TABLE notifications (
    id          uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- Whose inbox.
    account_id  uuid        NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    event_type  text        NOT NULL CHECK (event_type ~ '^[a-z][a-z0-9_]{1,62}$'),
    severity    text        NOT NULL CHECK (severity IN ('info', 'warning', 'critical')),
    subject     text        NOT NULL CHECK (length(subject) BETWEEN 1 AND 200),
    body        text        NOT NULL CHECK (length(body) <= 4000),
    -- The ids the notification is about (what a link in the page needs).
    data        jsonb       NOT NULL DEFAULT '{}',
    -- The same event is never turned into the same notification twice (events are delivered at
    -- least once).
    dedupe_key  text        NOT NULL CHECK (length(dedupe_key) <= 200),
    created_at  timestamptz NOT NULL DEFAULT now(),
    read_at     timestamptz,
    UNIQUE (account_id, dedupe_key)
);
CREATE INDEX notifications_account_idx ON notifications (account_id, created_at DESC);
CREATE INDEX notifications_unread_idx ON notifications (account_id) WHERE read_at IS NULL;

-- Pause stops everything outside the inbox; a snooze does the same until a time.
CREATE TABLE notification_settings (
    account_id    uuid        PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    paused        boolean     NOT NULL DEFAULT false,
    snoozed_until timestamptz,
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE notification_channels (
    id            uuid        PRIMARY KEY DEFAULT uuidv7(),
    account_id    uuid        NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind          text        NOT NULL CHECK (kind IN ('email', 'webhook')),
    name          text        NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
    -- An email address or a webhook URL.
    target        text        NOT NULL CHECK (length(target) BETWEEN 3 AND 500),
    -- A webhook's signing secret, sealed with the master key (shown once, when it is made).
    sealed_secret bytea       CHECK (sealed_secret IS NULL OR octet_length(sealed_secret) <= 1024),
    enabled       boolean     NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (account_id, name)
);
CREATE INDEX notification_channels_account_idx ON notification_channels (account_id);

-- Which events go to which channel.
CREATE TABLE notification_rules (
    channel_id uuid NOT NULL REFERENCES notification_channels (id) ON DELETE CASCADE,
    event_type text NOT NULL CHECK (event_type ~ '^[a-z][a-z0-9_]{1,62}$'),
    PRIMARY KEY (channel_id, event_type)
);

-- What is still to be sent to a channel, and how it went.
CREATE TABLE notification_deliveries (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    notification_id uuid        NOT NULL REFERENCES notifications (id) ON DELETE CASCADE,
    channel_id      uuid        NOT NULL REFERENCES notification_channels (id) ON DELETE CASCADE,
    status          text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed')),
    attempts        integer     NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    last_error      text        CHECK (last_error IS NULL OR length(last_error) <= 500),
    created_at      timestamptz NOT NULL DEFAULT now(),
    sent_at         timestamptz,
    UNIQUE (notification_id, channel_id)
);
CREATE INDEX notification_deliveries_due_idx ON notification_deliveries (next_attempt_at) WHERE status = 'pending';

-- The panel's mail server (one row), for the email channel. The password is sealed.
CREATE TABLE notification_smtp (
    id              boolean     PRIMARY KEY DEFAULT true CHECK (id),
    host            text        NOT NULL CHECK (length(host) BETWEEN 1 AND 253),
    port            integer     NOT NULL CHECK (port BETWEEN 1 AND 65535),
    security        text        NOT NULL CHECK (security IN ('starttls', 'tls', 'none')),
    username        text        CHECK (username IS NULL OR length(username) <= 254),
    sealed_password bytea       CHECK (sealed_password IS NULL OR octet_length(sealed_password) <= 1024),
    from_address    text        NOT NULL CHECK (length(from_address) BETWEEN 3 AND 254),
    from_name       text        CHECK (from_name IS NULL OR length(from_name) <= 100),
    enabled         boolean     NOT NULL DEFAULT true,
    updated_at      timestamptz NOT NULL DEFAULT now()
);

-- Two events that nobody announced yet. They are written to the outbox by triggers, so that every
-- way a backup can fail and every way a node can go offline (the session ends, the node falls
-- silent) is covered without each place having to remember to say so.
CREATE OR REPLACE FUNCTION announce_backup_failed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO outbox (event_type, payload) VALUES (
        'backup_failed',
        jsonb_build_object(
            'type', 'backup_failed',
            'backup_id', NEW.id,
            'tenant_id', NEW.tenant_id,
            'error', left(COALESCE(NEW.error, ''), 300)
        )
    );
    RETURN NEW;
END;
$$;
CREATE TRIGGER backups_failed AFTER UPDATE OF status ON backups
    FOR EACH ROW WHEN (NEW.status = 'failed' AND OLD.status IS DISTINCT FROM 'failed')
    EXECUTE FUNCTION announce_backup_failed();

CREATE OR REPLACE FUNCTION announce_node_offline() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO outbox (event_type, payload) VALUES (
        'node_offline',
        jsonb_build_object('type', 'node_offline', 'node_id', NEW.id, 'name', NEW.name)
    );
    RETURN NEW;
END;
$$;
CREATE TRIGGER nodes_offline AFTER UPDATE OF status ON nodes
    FOR EACH ROW WHEN (OLD.status = 'online' AND NEW.status = 'offline')
    EXECUTE FUNCTION announce_node_offline();
