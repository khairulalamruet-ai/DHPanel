-- T13.03: email. A hosting account turns mail on for one of its domains by making a mailbox
-- in it. The panel keeps what is wanted and a version counter per thing; the mail tick sends
-- the node's mail server (Stalwart) what it does not have yet and writes the DNS records it
-- answers with into the domain's zone.

-- The mail server of a node: the name it says it is, and the name the node has been told.
CREATE TABLE mail_servers (
    node_id          uuid        PRIMARY KEY REFERENCES nodes (id) ON DELETE CASCADE,
    hostname         text        NOT NULL CHECK (length(hostname) BETWEEN 4 AND 253),
    applied_hostname text,
    last_error       text,
    updated_at       timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE mail_domains (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- When the domain or the account is deleted the row stays, empty of them, in the state
    -- `removing` until the node's mail server has let go of the domain (the triggers below).
    tenant_id       uuid        REFERENCES tenants (id) ON DELETE SET NULL,
    domain_id       uuid        UNIQUE REFERENCES domains (id) ON DELETE SET NULL,
    fqdn            text        NOT NULL UNIQUE,
    node_id         uuid        NOT NULL REFERENCES nodes (id),
    -- A change of the domain's settings (limit, hold, catch-all) raises `version`; the node has
    -- them when `applied_version` has caught up. The same for the list of forwarders.
    version         bigint      NOT NULL DEFAULT 1,
    applied_version bigint      NOT NULL DEFAULT 0,
    fwd_version     bigint      NOT NULL DEFAULT 1,
    fwd_applied     bigint      NOT NULL DEFAULT 0,
    state           text        NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'active', 'error', 'removing')),
    last_error      text,
    -- Most messages an hour (the plan's limit); none: no limit.
    per_hour        integer     CHECK (per_hour IS NULL OR per_hour > 0),
    held            boolean     NOT NULL DEFAULT false,
    held_reason     text,
    held_at         timestamptz,
    -- What the mail server said the zone needs, and whether it is in the zone.
    records         jsonb       NOT NULL DEFAULT '[]',
    dns_applied     boolean     NOT NULL DEFAULT false,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX mail_domains_tenant_idx ON mail_domains (tenant_id);
CREATE INDEX mail_domains_node_idx ON mail_domains (node_id);

CREATE TABLE mailboxes (
    id              uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    mail_domain_id  uuid        NOT NULL REFERENCES mail_domains (id) ON DELETE CASCADE,
    local_part      text        NOT NULL CHECK (local_part ~ '^[a-z0-9]([a-z0-9._+-]{0,62}[a-z0-9])?$'),
    quota_mb        integer     NOT NULL CHECK (quota_mb BETWEEN 1 AND 1048576),
    used_bytes      bigint      NOT NULL DEFAULT 0 CHECK (used_bytes >= 0),
    state           text        NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'active', 'error', 'removing')),
    last_error      text,
    version         bigint      NOT NULL DEFAULT 1,
    applied_version bigint      NOT NULL DEFAULT 0,
    -- The automatic reply (at most one per mailbox).
    ar_subject      text,
    ar_body         text,
    ar_from         timestamptz,
    ar_until        timestamptz,
    ar_version      bigint      NOT NULL DEFAULT 0,
    ar_applied      bigint      NOT NULL DEFAULT 0,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (mail_domain_id, local_part)
);
CREATE INDEX mailboxes_tenant_idx ON mailboxes (tenant_id);

-- A password is never kept in the ledger or in a plain column: the API seals it with the
-- panel's master key, the dispatcher opens it for the one message that carries it, and the
-- row goes when the node has it.
CREATE TABLE mailbox_passwords (
    mailbox_id uuid        PRIMARY KEY REFERENCES mailboxes (id) ON DELETE CASCADE,
    sealed     bytea       NOT NULL CHECK (octet_length(sealed) <= 1024),
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Addresses that send mail on: `local` is a name, or `*` for the catch-all.
CREATE TABLE mail_forwarders (
    id             uuid        PRIMARY KEY DEFAULT uuidv7(),
    mail_domain_id uuid        NOT NULL REFERENCES mail_domains (id) ON DELETE CASCADE,
    local_part     text        NOT NULL CHECK (local_part = '*' OR local_part ~ '^[a-z0-9]([a-z0-9._+-]{0,62}[a-z0-9])?$'),
    destination    text        NOT NULL CHECK (length(destination) BETWEEN 3 AND 254),
    created_at     timestamptz NOT NULL DEFAULT now(),
    UNIQUE (mail_domain_id, local_part, destination)
);

-- What each node's mail server last reported about its outgoing queue (for the admin's page
-- and for the abuse check).
CREATE TABLE mail_queue_reports (
    node_id    uuid        PRIMARY KEY REFERENCES nodes (id) ON DELETE CASCADE,
    items      jsonb       NOT NULL DEFAULT '[]',
    total      bigint      NOT NULL DEFAULT 0,
    reported_at timestamptz NOT NULL DEFAULT now()
);

CREATE FUNCTION mail_domain_going() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'domains' THEN
        UPDATE mail_domains SET state = 'removing', version = version + 1 WHERE domain_id = OLD.id;
    ELSE
        UPDATE mail_domains SET state = 'removing', version = version + 1 WHERE tenant_id = OLD.id;
    END IF;
    RETURN OLD;
END;
$$;
CREATE TRIGGER domains_mail_going BEFORE DELETE ON domains
    FOR EACH ROW EXECUTE FUNCTION mail_domain_going();
CREATE TRIGGER tenants_mail_going BEFORE DELETE ON tenants
    FOR EACH ROW EXECUTE FUNCTION mail_domain_going();

CREATE TRIGGER mail_domains_set_updated_at BEFORE UPDATE ON mail_domains
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER mailboxes_set_updated_at BEFORE UPDATE ON mailboxes
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER mail_servers_set_updated_at BEFORE UPDATE ON mail_servers
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
