-- Enforce cross-table ownership at the database level: a row may only reference
-- another user's resource if that resource actually belongs to the same user.
-- Previously this was only implicitly true (never checked), so a guessed foreign
-- UUID belonging to a different user could be attached without error.

ALTER TABLE remote_storages     ADD CONSTRAINT remote_storages_id_user_id_key     UNIQUE (id, user_id);
ALTER TABLE local_devices        ADD CONSTRAINT local_devices_id_user_id_key        UNIQUE (id, user_id);
ALTER TABLE backup_config        ADD CONSTRAINT backup_config_id_user_id_key        UNIQUE (id, user_id);
ALTER TABLE remote_file_versions ADD CONSTRAINT remote_file_versions_id_user_id_key UNIQUE (id, user_id);
ALTER TABLE chunks               ADD CONSTRAINT chunks_id_user_id_key               UNIQUE (id, user_id);

ALTER TABLE backup_config
    ADD CONSTRAINT backup_config_storage_owned_by_user FOREIGN KEY (storage_id, user_id) REFERENCES remote_storages(id, user_id),
    ADD CONSTRAINT backup_config_device_owned_by_user  FOREIGN KEY (local_device_id, user_id) REFERENCES local_devices(id, user_id);

ALTER TABLE remote_file_versions
    ADD CONSTRAINT rfv_config_owned_by_user  FOREIGN KEY (backup_config_id, user_id) REFERENCES backup_config(id, user_id),
    ADD CONSTRAINT rfv_device_owned_by_user  FOREIGN KEY (device_id, user_id) REFERENCES local_devices(id, user_id),
    ADD CONSTRAINT rfv_storage_owned_by_user FOREIGN KEY (storage_id, user_id) REFERENCES remote_storages(id, user_id);

ALTER TABLE chunks
    ADD CONSTRAINT chunks_storage_owned_by_user FOREIGN KEY (storage_id, user_id) REFERENCES remote_storages(id, user_id);

-- remote_file_version_chunks has no user_id today — a guessed foreign
-- remote_file_version_id could attach chunks to another user's version.
ALTER TABLE remote_file_version_chunks ADD COLUMN user_id UUID;
UPDATE remote_file_version_chunks rvc SET user_id = rfv.user_id
    FROM remote_file_versions rfv WHERE rfv.id = rvc.remote_file_version_id;
ALTER TABLE remote_file_version_chunks ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE remote_file_version_chunks
    ADD CONSTRAINT rvc_version_owned_by_user FOREIGN KEY (remote_file_version_id, user_id) REFERENCES remote_file_versions(id, user_id),
    ADD CONSTRAINT rvc_chunk_owned_by_user   FOREIGN KEY (chunk_id, user_id) REFERENCES chunks(id, user_id);
