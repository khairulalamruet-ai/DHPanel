DROP FUNCTION effective_account_status(uuid);
ALTER TABLE plans DROP COLUMN display_name, DROP COLUMN reseller_id;
ALTER TABLE tenants DROP COLUMN reseller_id;
DROP TABLE resellers;
DROP TABLE reseller_packages;
