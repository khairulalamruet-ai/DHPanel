-- T06.07: web server templates keep their history. Every save is a new version;
-- the template says which it is on. Old versions stay readable (and can be put
-- back by saving their text again).
ALTER TABLE vhost_templates
    ADD COLUMN version integer NOT NULL DEFAULT 1 CHECK (version >= 1);

CREATE TABLE vhost_template_versions (
    template_id uuid        NOT NULL REFERENCES vhost_templates (id) ON DELETE CASCADE,
    version     integer     NOT NULL CHECK (version >= 1),
    name        text        NOT NULL,
    body        text        NOT NULL CHECK (length(body) BETWEEN 1 AND 32768),
    changed_by  uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (template_id, version)
);

-- What exists now is version 1.
INSERT INTO vhost_template_versions (template_id, version, name, body)
SELECT id, 1, name, body FROM vhost_templates;
