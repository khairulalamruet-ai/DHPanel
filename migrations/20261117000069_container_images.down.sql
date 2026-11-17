DROP TABLE runtime_app_images;
DELETE FROM runtime_apps WHERE kind = 'container';
ALTER TABLE runtime_apps DROP CONSTRAINT runtime_apps_kind_check;
ALTER TABLE runtime_apps ADD CONSTRAINT runtime_apps_kind_check CHECK (kind IN ('node', 'python'));
DROP TABLE container_images;
