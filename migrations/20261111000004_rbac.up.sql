-- Role-based access control (T01.11).
--
-- A permission is a dotted key such as `admin.roles.manage`. A role grants
-- exact keys, a whole subtree (`admin.*`) or everything (`*`). Accounts get
-- permissions only through roles. Built-in roles cannot be renamed or deleted
-- by the API; modules add their own permission keys in later migrations.
CREATE TABLE roles (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    name         text        NOT NULL
        CONSTRAINT roles_name_format CHECK (name ~ '^[a-z][a-z0-9_]{1,31}$'),
    description  text        NOT NULL DEFAULT ''
        CONSTRAINT roles_description_len CHECK (length(description) <= 255),
    built_in     boolean     NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX roles_name_key ON roles (name);

CREATE TRIGGER roles_set_updated_at
    BEFORE UPDATE ON roles
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE role_permissions (
    role_id     uuid NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    permission  text NOT NULL
        CONSTRAINT role_permissions_format CHECK (
            permission = '*'
            OR permission ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*\.\*$'
            OR permission ~ '^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$'
        ),
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE account_roles (
    account_id  uuid        NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    role_id     uuid        NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    granted_at  timestamptz NOT NULL DEFAULT now(),
    -- Who granted it; NULL for the command line or a deleted administrator.
    granted_by  uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    PRIMARY KEY (account_id, role_id)
);

CREATE INDEX account_roles_role_idx ON account_roles (role_id);

-- Built-in roles (docs/01-architecture/07-security-model.md).
INSERT INTO roles (name, description, built_in) VALUES
    ('super_admin', 'Full access, including managing administrators and roles', true),
    ('admin',       'Day-to-day panel administration', true),
    ('support',     'Read-only access for support staff', true),
    ('reseller',    'Manages own clients (permissions added by the reseller module)', true),
    ('client',      'Hosting customer (permissions added by feature modules)', true);

INSERT INTO role_permissions (role_id, permission)
SELECT r.id, p.permission
FROM (VALUES
    ('super_admin', '*'),
    ('admin',       'admin.administrators.read'),
    ('admin',       'admin.roles.read'),
    ('admin',       'admin.audit.read'),
    ('support',     'admin.audit.read')
) AS p (role, permission)
JOIN roles r ON r.name = p.role;

-- Administrators created before roles existed keep full access.
INSERT INTO account_roles (account_id, role_id)
SELECT a.id, r.id
FROM accounts a
CROSS JOIN roles r
WHERE a.kind = 'admin' AND r.name = 'super_admin';
