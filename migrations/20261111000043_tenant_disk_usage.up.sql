-- T08.10: what each account's files use, as the node last measured it.
CREATE TABLE tenant_disk_usage (
    tenant_id   uuid PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    measured_at timestamptz NOT NULL DEFAULT now(),
    total_bytes bigint      NOT NULL CHECK (total_bytes >= 0),
    inodes      bigint      NOT NULL CHECK (inodes >= 0),
    partial     boolean     NOT NULL DEFAULT false,
    -- The biggest folders: [{"path": "...", "bytes": 1, "inodes": 1}, ...]
    folders     jsonb       NOT NULL DEFAULT '[]'
);
