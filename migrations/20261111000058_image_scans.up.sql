-- T11.12: the images of the allow-list (deploy/stack/images.toml): what a registry says their
-- tag is now, and what the last vulnerability scan found. Rows are made by the dispatcher from
-- the list shipped with the build.
CREATE TABLE image_scans (
    reference       text        PRIMARY KEY CHECK (length(reference) BETWEEN 1 AND 300),
    role            text        NOT NULL CHECK (role ~ '^[a-z][a-z0-9_]{0,31}$'),
    tag             text        CHECK (tag IS NULL OR length(tag) BETWEEN 1 AND 128),
    note            text        NOT NULL DEFAULT '' CHECK (length(note) <= 500),
    -- What the registry says the tag is now, and when it was asked.
    latest_digest   text        CHECK (latest_digest IS NULL OR latest_digest ~ '^sha256:[0-9a-f]{64}$'),
    checked_at      timestamptz,
    -- The last scan.
    scanned_at      timestamptz,
    critical        integer,
    high            integer,
    medium          integer,
    low             integer,
    -- [{id, package, installed, fixed, severity, title}], at most 50, worst first.
    top             jsonb       NOT NULL DEFAULT '[]',
    -- An administrator asked for a scan now.
    scan_requested  boolean     NOT NULL DEFAULT false,
    -- A scan was sent and its answer is awaited (not asked again for a while).
    scan_started_at timestamptz
);
