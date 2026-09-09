use api_types::backup_job::{
    AbandonStaleJobsResponse, BackupJobDetail, BackupJobFileSummary, BackupJobSummary,
    CompleteBackupJobFileRequest, CompleteBackupJobRequest, CreateBackupJobFileRequest,
    CreateBackupJobFileResponse, CreateBackupJobRequest, CreateBackupJobResponse,
    GetBackupJobDetailResponse, GetLatestBackupJobResponse, GetResumableBackupJobRequest,
    GetResumableBackupJobResponse, ListBackupJobsRequest, ListBackupJobsResponse,
    LogCleanupFilesRequest, ResumableBackupJob,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::BackupJobRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgBackupJobRepo {
    pub pool: PgPool,
}

impl PgBackupJobRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BackupJobRepo for PgBackupJobRepo {
    async fn create_job(
        &self,
        user_id: Uuid,
        req: CreateBackupJobRequest,
    ) -> CoreResult<CreateBackupJobResponse> {
        let id = Uuid::now_v7();

        sqlx::query!(
            r#"
            INSERT INTO backup_jobs (id, user_id, backup_config_id, status, started_at)
            VALUES ($1, $2, $3, 'running', $4)
            "#,
            id,
            user_id,
            req.backup_config_id,
            req.started_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreateBackupJobResponse { id })
    }

    async fn create_job_file(
        &self,
        user_id: Uuid,
        req: CreateBackupJobFileRequest,
    ) -> CoreResult<CreateBackupJobFileResponse> {
        let id = Uuid::now_v7();
        let _ = user_id;

        sqlx::query!(
            r#"
            INSERT INTO backup_job_files (id, job_id, encrypted_name, name_nonce, blind_index, original_size, status)
            VALUES ($1, $2, $3, $4, $5, $6, 'uploading')
            "#,
            id,
            req.job_id,
            req.encrypted_name,
            req.name_nonce,
            req.blind_index,
            req.original_size,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreateBackupJobFileResponse { id })
    }

    async fn complete_job_file(
        &self,
        user_id: Uuid,
        req: CompleteBackupJobFileRequest,
    ) -> CoreResult<()> {
        let _ = user_id;

        sqlx::query!(
            r#"
            UPDATE backup_job_files
            SET uploaded_size = $2,
                deduplicated_size = $3,
                total_chunks = $4,
                deduplicated_chunks = $5,
                status = $6,
                error_message = $7
            WHERE id = $1
            "#,
            req.id,
            req.uploaded_size,
            req.deduplicated_size,
            req.total_chunks,
            req.deduplicated_chunks,
            req.status,
            req.error_message,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn complete_job(&self, user_id: Uuid, req: CompleteBackupJobRequest) -> CoreResult<()> {
        let _ = user_id;

        sqlx::query!(
            r#"
            UPDATE backup_jobs
            SET status = $2,
                error_message = $3,
                completed_at = NOW(),
                total_files = (SELECT COUNT(*) FROM backup_job_files WHERE job_id = $1)::INTEGER,
                total_files_succeeded = (SELECT COUNT(*) FROM backup_job_files WHERE job_id = $1 AND status = 'completed')::INTEGER,
                total_files_failed = (SELECT COUNT(*) FROM backup_job_files WHERE job_id = $1 AND status = 'failed')::INTEGER,
                original_bytes = (SELECT COALESCE(SUM(original_size), 0) FROM backup_job_files WHERE job_id = $1),
                uploaded_bytes = (SELECT COALESCE(SUM(uploaded_size), 0) FROM backup_job_files WHERE job_id = $1),
                deduplicated_bytes = (SELECT COALESCE(SUM(deduplicated_size), 0) FROM backup_job_files WHERE job_id = $1),
                deduplicated_chunks = (SELECT COALESCE(SUM(deduplicated_chunks), 0) FROM backup_job_files WHERE job_id = $1)::INTEGER
            WHERE id = $1
            "#,
            req.id,
            req.status,
            req.error_message,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn list_jobs(
        &self,
        user_id: Uuid,
        req: ListBackupJobsRequest,
    ) -> CoreResult<ListBackupJobsResponse> {
        let offset = (req.page - 1) * req.page_size;

        let rows = sqlx::query!(
            r#"
            SELECT id, backup_config_id, status, total_files, original_bytes,
                   uploaded_bytes, deduplicated_bytes, started_at, completed_at
            FROM backup_jobs
            WHERE user_id = $1
            ORDER BY started_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            req.page_size,
            offset,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let total_count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!" FROM backup_jobs WHERE user_id = $1"#,
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let jobs = rows
            .into_iter()
            .map(|r| BackupJobSummary {
                id: r.id,
                backup_config_id: r.backup_config_id,
                status: r.status,
                total_files: r.total_files,
                original_bytes: r.original_bytes,
                uploaded_bytes: r.uploaded_bytes,
                deduplicated_bytes: r.deduplicated_bytes,
                started_at: r.started_at,
                completed_at: r.completed_at,
            })
            .collect();

        Ok(ListBackupJobsResponse { jobs, total_count })
    }

    async fn get_job_detail(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> CoreResult<GetBackupJobDetailResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, status, total_files, total_files_succeeded,
                   total_files_failed, original_bytes, uploaded_bytes, deduplicated_bytes,
                   deduplicated_chunks, error_message, started_at, completed_at
            FROM backup_jobs
            WHERE id = $1 AND user_id = $2
            "#,
            job_id,
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let file_rows = sqlx::query!(
            r#"
            SELECT id, encrypted_name, name_nonce, blind_index, original_size, uploaded_size,
                   deduplicated_size, total_chunks, deduplicated_chunks, status, error_message
            FROM backup_job_files
            WHERE job_id = $1
            ORDER BY blind_index ASC
            "#,
            job_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let files = file_rows
            .into_iter()
            .map(|f| BackupJobFileSummary {
                id: f.id,
                encrypted_name: f.encrypted_name,
                name_nonce: f.name_nonce,
                blind_index: f.blind_index,
                original_size: f.original_size,
                uploaded_size: f.uploaded_size,
                deduplicated_size: f.deduplicated_size,
                total_chunks: f.total_chunks,
                deduplicated_chunks: f.deduplicated_chunks,
                status: f.status,
                error_message: f.error_message,
            })
            .collect();

        Ok(GetBackupJobDetailResponse {
            job: BackupJobDetail {
                id: row.id,
                backup_config_id: row.backup_config_id,
                status: row.status,
                total_files: row.total_files,
                total_files_succeeded: row.total_files_succeeded,
                total_files_failed: row.total_files_failed,
                original_bytes: row.original_bytes,
                uploaded_bytes: row.uploaded_bytes,
                deduplicated_bytes: row.deduplicated_bytes,
                deduplicated_chunks: row.deduplicated_chunks,
                error_message: row.error_message,
                started_at: row.started_at,
                completed_at: row.completed_at,
                files,
            },
        })
    }

    async fn get_latest_job(&self, user_id: Uuid) -> CoreResult<GetLatestBackupJobResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, status, total_files, original_bytes,
                   uploaded_bytes, deduplicated_bytes, started_at, completed_at
            FROM backup_jobs
            WHERE user_id = $1
            ORDER BY started_at DESC
            LIMIT 1
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let job = row.map(|r| BackupJobSummary {
            id: r.id,
            backup_config_id: r.backup_config_id,
            status: r.status,
            total_files: r.total_files,
            original_bytes: r.original_bytes,
            uploaded_bytes: r.uploaded_bytes,
            deduplicated_bytes: r.deduplicated_bytes,
            started_at: r.started_at,
            completed_at: r.completed_at,
        });

        Ok(GetLatestBackupJobResponse { job })
    }

    async fn get_resumable_job(
        &self,
        user_id: Uuid,
        req: GetResumableBackupJobRequest,
    ) -> CoreResult<GetResumableBackupJobResponse> {
        // Find the most recent resumable backup job for this config (running or interrupted)
        let row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, status, total_files, original_bytes,
                   uploaded_bytes, deduplicated_bytes, started_at, completed_at
            FROM backup_jobs
            WHERE user_id = $1 AND backup_config_id = $2 AND status IN ('running', 'interrupted')
            ORDER BY started_at DESC
            LIMIT 1
            "#,
            user_id,
            req.backup_config_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let job = match row {
            Some(r) => {
                let job_id = r.id;

                // Get blind indexes of files that are already completed in this job
                let completed_blind_indexes = sqlx::query_scalar!(
                    r#"
                    SELECT blind_index
                    FROM backup_job_files
                    WHERE job_id = $1 AND status = 'completed'
                    "#,
                    job_id,
                )
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

                Some(ResumableBackupJob {
                    job: BackupJobSummary {
                        id: r.id,
                        backup_config_id: r.backup_config_id,
                        status: r.status,
                        total_files: r.total_files,
                        original_bytes: r.original_bytes,
                        uploaded_bytes: r.uploaded_bytes,
                        deduplicated_bytes: r.deduplicated_bytes,
                        started_at: r.started_at,
                        completed_at: r.completed_at,
                    },
                    completed_blind_indexes,
                })
            }
            None => None,
        };

        Ok(GetResumableBackupJobResponse { job })
    }

    async fn abandon_stale_jobs(&self, user_id: Uuid) -> CoreResult<AbandonStaleJobsResponse> {
        // Fetch all jobs stuck in 'running' for this user
        let job_ids = sqlx::query_scalar!(
            r#"SELECT id FROM backup_jobs WHERE user_id = $1 AND status = 'running'"#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let count = job_ids.len() as u32;
        for job_id in job_ids {
            // Reuse complete_job to compute aggregates and set terminal status
            self.complete_job(
                user_id,
                CompleteBackupJobRequest {
                    id: job_id,
                    status: "failed".to_string(),
                    error_message: Some("Backup was interrupted when the app closed".to_string()),
                },
            )
            .await?;
        }

        Ok(AbandonStaleJobsResponse { count })
    }

    async fn log_cleanup_files(
        &self,
        _user_id: Uuid,
        req: LogCleanupFilesRequest,
    ) -> CoreResult<()> {
        if req.files.is_empty() {
            return Ok(());
        }

        let files_deleted = req.files.len() as i64;
        let bytes_freed: i64 = req.files.iter().map(|f| f.size).sum();

        for file in &req.files {
            let id = Uuid::now_v7();
            sqlx::query!(
                r#"
                INSERT INTO backup_job_cleanup_files (id, job_id, encrypted_name, name_nonce, blind_index, size)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                id,
                req.job_id,
                file.encrypted_name,
                file.name_nonce,
                file.blind_index,
                file.size,
            )
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        }

        sqlx::query!(
            r#"
            UPDATE backup_jobs
            SET cleanup_files_deleted = cleanup_files_deleted + $2,
                cleanup_bytes_freed   = cleanup_bytes_freed   + $3
            WHERE id = $1
            "#,
            req.job_id,
            files_deleted as i32,
            bytes_freed,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }
}
