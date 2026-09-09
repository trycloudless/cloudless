CREATE INDEX IF NOT EXISTS idx_files_synced_at ON files(backup_config_id, synced_at)
    WHERE synced_at IS NOT NULL;
