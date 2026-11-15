-- T11.06: the firewall policy of a node (who may reach which port). The document is the node
-- protocol's FirewallPolicy, checked by the API before it is stored; a node with no row has no
-- panel-managed firewall.
CREATE TABLE node_firewalls (
    node_id    uuid        PRIMARY KEY REFERENCES nodes (id) ON DELETE CASCADE,
    policy     jsonb       NOT NULL,
    updated_by uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
