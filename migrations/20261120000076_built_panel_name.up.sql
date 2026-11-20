-- The panel is now called Built Panel (it was DHPanel): the values the database itself holds.
ALTER TABLE schema_info ALTER COLUMN product SET DEFAULT 'builtpanel';
UPDATE schema_info SET product = 'builtpanel' WHERE product = 'dhpanel';

ALTER TABLE migration_transfers DROP CONSTRAINT migration_transfers_source_type_check;
UPDATE migration_transfers SET source_type = 'builtpanel_archive' WHERE source_type = 'dhpanel_archive';
ALTER TABLE migration_transfers ADD CONSTRAINT migration_transfers_source_type_check
    CHECK (source_type IN ('cpanel', 'cyberpanel', 'openpanel', 'builtpanel_archive'));

-- The first line of every web server template the panel wrote (an administrator's own text
-- after it is kept).
UPDATE vhost_templates
   SET body = '# Managed by Built Panel for {{domain}}' || substr(body, length('# Managed by DHPanel for {{domain}}') + 1)
 WHERE body LIKE '# Managed by DHPanel for {{domain}}%';
