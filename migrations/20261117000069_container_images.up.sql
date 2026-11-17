-- T12.12: containers a client runs from images the administrator approved. The catalog holds the
-- images (by digest, never by tag); a client's container is an application (runtime_apps) of
-- kind 'container' that points at one of them.
CREATE TABLE container_images (
    id           uuid        PRIMARY KEY DEFAULT uuidv7(),
    -- What clients see.
    name         text        NOT NULL UNIQUE CHECK (name ~ '^[A-Za-z0-9][A-Za-z0-9 ._-]{0,60}$'),
    -- `registry/repo@sha256:<digest>`: pinned, so the content cannot change under a client.
    reference    text        NOT NULL UNIQUE
        CHECK (reference ~ '^[a-z0-9][a-z0-9._/:-]{0,199}@sha256:[0-9a-f]{64}$'),
    -- The port the image listens on, and the memory it starts with.
    default_port integer     NOT NULL CHECK (default_port BETWEEN 1024 AND 65535),
    default_memory_mb integer NOT NULL DEFAULT 256 CHECK (default_memory_mb BETWEEN 32 AND 4096),
    note         text        NOT NULL DEFAULT '' CHECK (length(note) <= 500),
    enabled      boolean     NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE runtime_apps DROP CONSTRAINT runtime_apps_kind_check;
ALTER TABLE runtime_apps ADD CONSTRAINT runtime_apps_kind_check
    CHECK (kind IN ('node', 'python', 'container'));

-- Which catalog image a container application runs. An image in use cannot be deleted.
CREATE TABLE runtime_app_images (
    app_id   uuid PRIMARY KEY REFERENCES runtime_apps (id) ON DELETE CASCADE,
    image_id uuid NOT NULL REFERENCES container_images (id) ON DELETE RESTRICT
);
