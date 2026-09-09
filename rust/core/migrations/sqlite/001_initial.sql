CREATE TABLE IF NOT EXISTS files (
    backup_config_id TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime TEXT NOT NULL,
    content_hash TEXT,
    encrypted_name BLOB NOT NULL,
    encrypted_name_nonce BLOB NOT NULL,
    blind_index BLOB NOT NULL,
    remote_file_id TEXT,
    last_backed_up_version INTEGER,
    synced_at TEXT,
    PRIMARY KEY (backup_config_id, path)
);

CREATE INDEX IF NOT EXISTS idx_files_config_prefix ON files(backup_config_id, path);
CREATE INDEX IF NOT EXISTS idx_files_blind_index ON files(backup_config_id, blind_index);
