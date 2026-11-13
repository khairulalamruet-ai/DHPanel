-- T04.13: a session an administrator opened "as" a client records who it is.
ALTER TABLE sessions
    ADD COLUMN impersonator_id uuid REFERENCES accounts (id) ON DELETE SET NULL;
