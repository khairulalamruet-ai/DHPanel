-- A scan is for one release (the version the installation says it runs), which is what its
-- files are compared with.
ALTER TABLE integrity_scans ADD COLUMN version text CHECK (version IS NULL OR length(version) <= 64);
