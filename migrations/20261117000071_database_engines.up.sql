-- T07.12: a database or a database user lives on one kind of server: the node's MariaDB
-- (what every row so far is) or its PostgreSQL. A grant joins a database and a user of the
-- same engine (the module checks it; the two columns cannot be compared in a foreign key).
ALTER TABLE databases ADD COLUMN engine text NOT NULL DEFAULT 'mysql'
    CONSTRAINT databases_engine CHECK (engine IN ('mysql', 'postgres'));
ALTER TABLE db_users ADD COLUMN engine text NOT NULL DEFAULT 'mysql'
    CONSTRAINT db_users_engine CHECK (engine IN ('mysql', 'postgres'));
-- PostgreSQL names are at most 63 bytes, and the agent adds a four character suffix to the
-- group roles of a database: so a PostgreSQL database or user is at most 59 characters.
ALTER TABLE databases ADD CONSTRAINT databases_postgres_length
    CHECK (engine <> 'postgres' OR length(name) <= 59);
ALTER TABLE db_users ADD CONSTRAINT db_users_postgres_length
    CHECK (engine <> 'postgres' OR length(name) <= 59);
