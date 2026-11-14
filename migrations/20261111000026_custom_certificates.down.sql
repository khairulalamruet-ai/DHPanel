DELETE FROM certificates WHERE kind = 'custom';
ALTER TABLE certificates
    DROP CONSTRAINT IF EXISTS certificates_custom_has_material,
    DROP COLUMN IF EXISTS chain_pem,
    DROP COLUMN IF EXISTS private_key_enc,
    DROP COLUMN IF EXISTS uploaded_by;
