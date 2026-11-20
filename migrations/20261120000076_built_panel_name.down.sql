UPDATE vhost_templates
   SET body = '# Managed by DHPanel for {{domain}}' || substr(body, length('# Managed by Built Panel for {{domain}}') + 1)
 WHERE body LIKE '# Managed by Built Panel for {{domain}}%';
ALTER TABLE migration_transfers DROP CONSTRAINT migration_transfers_source_type_check;
UPDATE migration_transfers SET source_type = 'dhpanel_archive' WHERE source_type = 'builtpanel_archive';
ALTER TABLE migration_transfers ADD CONSTRAINT migration_transfers_source_type_check
    CHECK (source_type IN ('cpanel', 'cyberpanel', 'openpanel', 'dhpanel_archive'));
UPDATE schema_info SET product = 'dhpanel' WHERE product = 'builtpanel';
ALTER TABLE schema_info ALTER COLUMN product SET DEFAULT 'dhpanel';
