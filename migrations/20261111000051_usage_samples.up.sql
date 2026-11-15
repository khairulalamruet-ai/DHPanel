-- T10.09: how much each account (and node) uses, as time series.
--
-- The node's `collect_metrics` gives counters; the control plane keeps the previous ones in
-- `usage_counters` and stores the difference. Per account, one row a minute for two days
-- (`usage_minute`), then one row an hour for 400 days (`usage_hour`, a table partitioned by
-- month: old months are dropped, not deleted from). Per node, one row a minute for 30 days.

CREATE TABLE usage_counters (
    node_id    uuid        NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    container  text        NOT NULL CHECK (length(container) <= 200),
    at         timestamptz NOT NULL,
    cpu_nanos  bigint      NOT NULL,
    net_rx     bigint      NOT NULL,
    net_tx     bigint      NOT NULL,
    io_read    bigint      NOT NULL,
    io_write   bigint      NOT NULL,
    PRIMARY KEY (node_id, container)
);

CREATE TABLE usage_minute (
    tenant_id      uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    at             timestamptz NOT NULL,
    -- Thousandths of a CPU used on average since the last reading (1000 = one whole CPU).
    cpu_millis     integer     NOT NULL CHECK (cpu_millis >= 0),
    memory_bytes   bigint      NOT NULL CHECK (memory_bytes >= 0),
    pids           integer     NOT NULL CHECK (pids >= 0),
    -- Bytes since the last reading.
    net_rx_bytes   bigint      NOT NULL CHECK (net_rx_bytes >= 0),
    net_tx_bytes   bigint      NOT NULL CHECK (net_tx_bytes >= 0),
    io_read_bytes  bigint      NOT NULL CHECK (io_read_bytes >= 0),
    io_write_bytes bigint      NOT NULL CHECK (io_write_bytes >= 0),
    PRIMARY KEY (tenant_id, at)
);
CREATE INDEX usage_minute_at_idx ON usage_minute (at);

-- The same per hour: CPU and memory averaged, the bytes added up.
CREATE TABLE usage_hour (
    tenant_id      uuid        NOT NULL,
    at             timestamptz NOT NULL,
    cpu_millis     integer     NOT NULL,
    memory_bytes   bigint      NOT NULL,
    pids           integer     NOT NULL,
    net_rx_bytes   bigint      NOT NULL,
    net_tx_bytes   bigint      NOT NULL,
    io_read_bytes  bigint      NOT NULL,
    io_write_bytes bigint      NOT NULL,
    PRIMARY KEY (tenant_id, at)
) PARTITION BY RANGE (at);
-- Anything outside the months made ahead still has a place; `usage::maintain` makes the months.
CREATE TABLE usage_hour_default PARTITION OF usage_hour DEFAULT;

CREATE TABLE node_usage_minute (
    node_id               uuid        NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    at                    timestamptz NOT NULL,
    cpus                  integer     NOT NULL,
    -- Thousandths of the node's CPU that were busy since the last reading (1000 = all of it).
    cpu_busy_millis       integer     NOT NULL CHECK (cpu_busy_millis BETWEEN 0 AND 1000),
    memory_used_bytes     bigint      NOT NULL,
    memory_total_bytes    bigint      NOT NULL,
    swap_used_bytes       bigint      NOT NULL,
    load1_x100            integer     NOT NULL,
    disk_used_bytes       bigint      NOT NULL,
    disk_total_bytes      bigint      NOT NULL,
    containers            integer     NOT NULL,
    -- The jiffies of the reading, to make the next difference.
    cpu_busy_jiffies      bigint      NOT NULL,
    cpu_total_jiffies     bigint      NOT NULL,
    PRIMARY KEY (node_id, at)
);
