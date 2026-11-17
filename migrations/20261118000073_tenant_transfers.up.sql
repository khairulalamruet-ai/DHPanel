-- T15.07: moving a hosting account from one node to another. The work of one transfer is a few
-- backups and restores through a destination both nodes can reach (made by the backup code),
-- with the account set up on the new node first and switched over last; this table says where
-- each transfer is, and `transfer_extras` carries what a backup or a restore of a transfer needs
-- beyond what an ordinary one has.
CREATE TABLE tenant_transfers (
    id               uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    from_node        uuid        NOT NULL REFERENCES nodes (id),
    to_node          uuid        NOT NULL REFERENCES nodes (id),
    destination_id   uuid        NOT NULL REFERENCES backup_destinations (id),
    -- Go on to the last pass as soon as the first copy is on the new node.
    auto_cutover     boolean     NOT NULL DEFAULT false,
    state            text        NOT NULL DEFAULT 'requested'
        CHECK (state IN ('requested', 'provisioning', 'precopy_backup', 'precopy_restore',
                         'precopied', 'freezing', 'delta_backup', 'delta_restore',
                         'switching', 'switched', 'cleaning', 'done', 'cancelling',
                         'cancelled', 'failed')),
    -- The operation the transfer is waiting for, or the backup or restore it made.
    waiting_for      uuid,
    first_backup     uuid,
    delta_backup     uuid,
    -- Files changed after this time are the last pass's (Unix seconds).
    copy_started_at  bigint,
    -- When the account was frozen on the old node for the last pass (Unix seconds).
    frozen_at        bigint,
    -- How long the site was off (seconds), when it is over.
    downtime_secs    integer,
    error            text        CHECK (error IS NULL OR length(error) <= 500),
    created_by       uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CHECK (from_node <> to_node)
);
-- One transfer at a time for an account.
CREATE UNIQUE INDEX tenant_transfers_live_idx ON tenant_transfers (tenant_id)
    WHERE state NOT IN ('done', 'cancelled', 'failed');

CREATE TABLE transfer_extras (
    -- A backup's or a restore's id.
    id               uuid        PRIMARY KEY,
    transfer_id      uuid        NOT NULL REFERENCES tenant_transfers (id) ON DELETE CASCADE,
    -- The node that does it (a transfer's backup is made on the old node, its restore on the
    -- new one; the account is on one of them at a time).
    node_id          uuid        NOT NULL REFERENCES nodes (id),
    files_since      bigint,
    with_db_users    boolean     NOT NULL DEFAULT false,
    create_databases boolean     NOT NULL DEFAULT false,
    restore_db_users boolean     NOT NULL DEFAULT false
);
