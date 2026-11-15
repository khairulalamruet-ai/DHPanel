-- T11.02: how the web application firewall behaves: for the whole panel, for one account, or for
-- one domain. The most specific one that exists decides the mode (off, detect, block); the
-- rules that are switched off (false positives) are those of all of them together.
CREATE TABLE waf_policies (
    id             uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    scope          text        NOT NULL CHECK (scope IN ('global', 'tenant', 'domain')),
    tenant_id      uuid        REFERENCES tenants (id) ON DELETE CASCADE,
    domain_id      uuid        REFERENCES domains (id) ON DELETE CASCADE,
    mode           text        NOT NULL CHECK (mode IN ('off', 'detect', 'block')),
    -- Rule ids (such as 942100) that do not apply.
    disabled_rules integer[]   NOT NULL DEFAULT '{}'
                   CHECK (cardinality(disabled_rules) <= 200
                         ),
    updated_by     uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CHECK ((scope = 'global' AND tenant_id IS NULL AND domain_id IS NULL)
        OR (scope = 'tenant' AND tenant_id IS NOT NULL AND domain_id IS NULL)
        OR (scope = 'domain' AND tenant_id IS NOT NULL AND domain_id IS NOT NULL))
);
CREATE UNIQUE INDEX waf_policies_global_idx ON waf_policies (scope) WHERE scope = 'global';
CREATE UNIQUE INDEX waf_policies_tenant_idx ON waf_policies (tenant_id) WHERE scope = 'tenant';
CREATE UNIQUE INDEX waf_policies_domain_idx ON waf_policies (domain_id) WHERE scope = 'domain';
