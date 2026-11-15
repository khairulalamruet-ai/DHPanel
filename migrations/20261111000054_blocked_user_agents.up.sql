-- T11.05: User-Agent texts the web servers refuse (any request whose User-Agent contains one of
-- them, in any case, is answered 403 before a site sees it). One list for the whole panel.
CREATE TABLE blocked_user_agents (
    id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    pattern    text        NOT NULL CHECK (length(pattern) BETWEEN 1 AND 100 AND pattern !~ '[[:cntrl:]]'),
    note       text        CHECK (note IS NULL OR length(note) <= 200),
    created_by uuid        REFERENCES accounts (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX blocked_user_agents_pattern_idx ON blocked_user_agents (lower(pattern));
