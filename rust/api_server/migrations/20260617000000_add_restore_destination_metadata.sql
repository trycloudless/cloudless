ALTER TABLE restore_jobs
    ADD COLUMN IF NOT EXISTS destination_type TEXT NOT NULL DEFAULT 'download_folder'
        CHECK (destination_type IN ('original_path', 'download_folder', 'custom_path', 'configured_storage')),
    ADD COLUMN IF NOT EXISTS destination_storage_id UUID NULL REFERENCES remote_storages(id),
    ADD COLUMN IF NOT EXISTS destination_prefix TEXT NULL;

ALTER TABLE restore_job_files
    DROP CONSTRAINT IF EXISTS restore_job_files_status_check,
    ADD CONSTRAINT restore_job_files_status_check
        CHECK (status IN ('pending', 'restoring', 'completed', 'failed', 'skipped'));
