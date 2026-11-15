-- T12.02: passwords an installer needs while it runs (the new database user's, the first
-- administrator's), sealed with the master key. Removed when the install ends.
CREATE TABLE app_secrets (
    app_id uuid  NOT NULL REFERENCES apps (id) ON DELETE CASCADE,
    name   text  NOT NULL CHECK (name ~ '^[a-z_]{1,32}$'),
    sealed bytea NOT NULL,
    PRIMARY KEY (app_id, name)
);
