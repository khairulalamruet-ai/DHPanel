-- T05.17: a certificate a person uploaded keeps its chain (public) and its private
-- key (sealed with the panel's master key, never plain). An ACME certificate has
-- neither: its key stays on the node.
ALTER TABLE certificates
    ADD COLUMN chain_pem        text
        CONSTRAINT certificates_chain_len CHECK (length(chain_pem) <= 40000),
    ADD COLUMN private_key_enc  bytea
        CONSTRAINT certificates_key_len CHECK (octet_length(private_key_enc) <= 40000),
    ADD COLUMN uploaded_by      uuid REFERENCES accounts (id) ON DELETE SET NULL,
    ADD CONSTRAINT certificates_custom_has_material CHECK (
        (kind = 'custom') = (chain_pem IS NOT NULL AND private_key_enc IS NOT NULL)
    );
