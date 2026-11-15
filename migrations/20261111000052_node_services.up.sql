-- T10.13: the state of each node's core services (database server, phpMyAdmin, file server,
-- web server, agent), as the node last reported it.
CREATE TABLE node_services (
    node_id    uuid        NOT NULL REFERENCES nodes (id) ON DELETE CASCADE,
    name       text        NOT NULL CHECK (name ~ '^[a-z][a-z0-9_]{0,31}$'),
    state      text        NOT NULL CHECK (state IN ('running', 'stopped', 'unreachable', 'not_installed')),
    detail     text        NOT NULL DEFAULT '' CHECK (length(detail) <= 500),
    -- When it came to be in this state.
    since      timestamptz NOT NULL DEFAULT now(),
    checked_at timestamptz NOT NULL DEFAULT now(),
    -- Whether the administrators were told it is down (until it runs again).
    alerted    boolean     NOT NULL DEFAULT false,
    PRIMARY KEY (node_id, name)
);
