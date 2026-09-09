ALTER TABLE backup_config
    ADD COLUMN encrypted_exclusion_config BYTEA,
    ADD COLUMN exclusion_config_nonce     BYTEA;
