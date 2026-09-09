ALTER TABLE restore_job_files
    ADD COLUMN IF NOT EXISTS destination_path TEXT NULL;
