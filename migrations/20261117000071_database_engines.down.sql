ALTER TABLE db_users DROP CONSTRAINT IF EXISTS db_users_postgres_length;
ALTER TABLE databases DROP CONSTRAINT IF EXISTS databases_postgres_length;
DELETE FROM databases WHERE engine <> 'mysql';
DELETE FROM db_users WHERE engine <> 'mysql';
ALTER TABLE db_users DROP COLUMN engine;
ALTER TABLE databases DROP COLUMN engine;
