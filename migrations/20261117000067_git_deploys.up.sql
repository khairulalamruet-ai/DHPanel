-- T12.09: a site or an application that is deployed from a git repository: fetched, built in a
-- sandbox, then switched in with one atomic rename. A push can start it (webhook).
CREATE TABLE git_deploys (
    id                uuid        PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name              text        NOT NULL CHECK (name ~ '^[a-z][a-z0-9-]{0,30}[a-z0-9]$'),
    -- https only (the repository's address, without a login).
    repo_url          text        NOT NULL CHECK (length(repo_url) <= 300 AND repo_url ~ '^https://[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+$'
                                                  AND repo_url !~ '^https://[^/]*@'),
    branch            text        NOT NULL DEFAULT 'main'
        CHECK (branch ~ '^[A-Za-z0-9._/-]{1,100}$' AND branch !~ '(^|/)\.\.?(/|$)' AND branch !~ '^-'),
    -- The folder below the web directory that is a link to the release in use.
    dest              text        NOT NULL
        CHECK (length(dest) BETWEEN 1 AND 200 AND dest !~ '(^|/)\.\.?(/|$)' AND dest !~ '^/'
               AND dest !~ '//' AND dest !~ '[^A-Za-z0-9._/-]'),
    -- Commands that build the release (each one a line of words, no shell), in `build_kind`'s
    -- runtime image.
    build_steps       text[]      NOT NULL DEFAULT '{}' CHECK (cardinality(build_steps) <= 5),
    build_kind        text        CHECK (build_kind IN ('node', 'python')),
    build_version     text        CHECK (build_version ~ '^[0-9.]{1,6}$'),
    -- The application to run again after a deploy.
    app_id            uuid        REFERENCES runtime_apps (id) ON DELETE SET NULL,
    -- A token for a private repository over https, sealed.
    sealed_token      bytea,
    -- The secret the webhook is signed with, sealed.
    sealed_webhook    bytea       NOT NULL,
    status            text        NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'deploying', 'ok', 'failed', 'removing')),
    op_id             uuid,
    last_commit       text        CHECK (last_commit ~ '^[0-9a-f]{40}$'),
    last_log          text        CHECK (length(last_log) <= 8000),
    last_error        text        CHECK (length(last_error) <= 500),
    last_deployed_at  timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name),
    UNIQUE (tenant_id, dest),
    CHECK ((build_kind IS NULL) = (build_version IS NULL)),
    CHECK (cardinality(build_steps) = 0 OR build_kind IS NOT NULL)
);
CREATE INDEX git_deploys_awaiting_idx ON git_deploys (id) WHERE op_id IS NOT NULL;
