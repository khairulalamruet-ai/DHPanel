-- What the daily scans say about an installation: `ok`; `failed` (a program is not the
-- release's or is missing: its license is refused until an administrator has looked);
-- `recovering` (a recovery was approved: the next clean scan makes it `ok` again).
ALTER TABLE installations
    ADD COLUMN integrity_state      text NOT NULL DEFAULT 'ok'
        CHECK (integrity_state IN ('ok', 'failed', 'recovering')),
    ADD COLUMN integrity_changed_at timestamptz;
