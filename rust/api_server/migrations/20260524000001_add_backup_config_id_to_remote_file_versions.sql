-- Add backup_config_id to remote_file_versions for direct config-scoped queries.
-- The column is initially nullable to allow backfill, then made NOT NULL.

ALTER TABLE remote_file_versions
    ADD COLUMN backup_config_id UUID REFERENCES backup_config(id);

-- Backfill: for each row, find the backup_config that owns this (user, device, storage) tuple.
-- Safe because each (user_id, device_id, storage_id) maps to exactly one config during testing.
UPDATE remote_file_versions rfv
SET backup_config_id = bc.id
FROM backup_config bc
WHERE bc.user_id    = rfv.user_id
  AND bc.local_device_id = rfv.device_id
  AND bc.storage_id = rfv.storage_id;

ALTER TABLE remote_file_versions
    ALTER COLUMN backup_config_id SET NOT NULL;

CREATE INDEX idx_rfv_backup_config_id ON remote_file_versions(backup_config_id);
