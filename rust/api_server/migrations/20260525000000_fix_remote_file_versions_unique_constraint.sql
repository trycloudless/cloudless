-- Replace the (user_id, device_id, storage_id, name_blind_index, version) unique constraint
-- with one scoped to backup_config_id now that multiple configs can share the same
-- device + storage combination.
ALTER TABLE remote_file_versions
    DROP CONSTRAINT remote_file_versions_user_id_device_id_storage_id_name_blin_key;

ALTER TABLE remote_file_versions
    ADD CONSTRAINT remote_file_versions_config_blind_version_key
    UNIQUE (backup_config_id, name_blind_index, version);
