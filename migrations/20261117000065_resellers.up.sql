-- Resellers (T14.01, T14.03, T14.04, T14.06): accounts that sell hosting to clients of their own,
-- inside the limits of a package the administrator gives them.

CREATE TABLE reseller_packages (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    name              text        NOT NULL
        CONSTRAINT reseller_packages_name_len CHECK (length(name) BETWEEN 1 AND 100),
    max_accounts      integer     NOT NULL CHECK (max_accounts BETWEEN 0 AND 1000000),
    max_disk_gb       integer     NOT NULL CHECK (max_disk_gb BETWEEN 0 AND 100000000),
    max_bandwidth_gb  integer     NOT NULL CHECK (max_bandwidth_gb BETWEEN 0 AND 100000000),
    -- When true the sums of the reseller's plans may pass the disk and bandwidth caps
    -- (only the number of accounts is enforced).
    allow_overselling boolean     NOT NULL DEFAULT false,
    -- Plans of the administrator the reseller may hand out as they are.
    allowed_plans     uuid[]      NOT NULL DEFAULT '{}',
    created_at        timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX reseller_packages_name_key ON reseller_packages (lower(name));

CREATE TABLE resellers (
    account_id   uuid        PRIMARY KEY REFERENCES accounts (id) ON DELETE RESTRICT,
    company_name text        NOT NULL
        CONSTRAINT resellers_company_len CHECK (length(company_name) BETWEEN 1 AND 100),
    package_id   uuid        NOT NULL REFERENCES reseller_packages (id) ON DELETE RESTRICT,
    status       text        NOT NULL DEFAULT 'active'
        CONSTRAINT resellers_status CHECK (status IN ('active', 'suspended')),
    branding     jsonb       NOT NULL DEFAULT '{}'
        CONSTRAINT resellers_branding_object CHECK (jsonb_typeof(branding) = 'object')
        CONSTRAINT resellers_branding_size CHECK (octet_length(branding::text) <= 4096),
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX resellers_package_idx ON resellers (package_id);

-- Whose client a hosting account is. NULL: the administrator's own.
ALTER TABLE tenants
    ADD COLUMN reseller_id uuid REFERENCES resellers (account_id) ON DELETE RESTRICT;
CREATE INDEX tenants_reseller_idx ON tenants (reseller_id) WHERE reseller_id IS NOT NULL;

-- A reseller's own plans live in `plans` (so every limit works as it does for any plan) but
-- never show up in the administrator's list; `display_name` is what the reseller called it.
ALTER TABLE plans
    ADD COLUMN reseller_id   uuid REFERENCES resellers (account_id) ON DELETE RESTRICT,
    ADD COLUMN display_name  text
        CONSTRAINT plans_display_name_len CHECK (display_name IS NULL OR length(display_name) BETWEEN 1 AND 100);
CREATE INDEX plans_reseller_idx ON plans (reseller_id) WHERE reseller_id IS NOT NULL;

-- The status an account is treated as having: the account's own, unless it is a reseller
-- that is suspended or a client of one. A suspended reseller's clients cannot sign in, but
-- their sites keep running (their hosting accounts are not touched).
CREATE FUNCTION effective_account_status(acct uuid) RETURNS account_status
LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN a.status = 'active' AND (
                   EXISTS (SELECT 1 FROM resellers r
                            WHERE r.account_id = a.id AND r.status = 'suspended')
                OR EXISTS (SELECT 1 FROM tenants t
                             JOIN resellers r ON r.account_id = t.reseller_id
                            WHERE t.owner_account_id = a.id AND r.status = 'suspended'))
                THEN 'suspended'::account_status
                ELSE a.status END
      FROM accounts a WHERE a.id = acct
$$;
