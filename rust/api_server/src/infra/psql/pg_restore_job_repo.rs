use api_types::restore_job::{
    CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
    CreateRestoreJobFileResponse, CreateRestoreJobRequest, CreateRestoreJobResponse,
    GetLatestRestoreJobResponse, GetRestoreJobDetailResponse, GetResumableRestoreJobResponse,
    ListRestoreJobsRequest, ListRestoreJobsResponse, RestoreJobDetail, RestoreJobFileSummary,
    RestoreJobSummary, ResumableRestoreJob,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreError, CoreResult, ports::RestoreJobRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgRestoreJobRepo {
    pub pool: PgPool,
}

impl PgRestoreJobRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RestoreJobRepo for PgRestoreJobRepo {
    async fn create_job(
        &self,
        user_id: Uuid,
        req: CreateRestoreJobRequest,
    ) -> CoreResult<CreateRestoreJobResponse> {
        let id = Uuid::now_v7();
        let overwrite_str = match req.overwrite_behavior {
            api_types::restore_job::OverwriteBehavior::Overwrite => "overwrite",
            api_types::restore_job::OverwriteBehavior::KeepBoth => "keep_both",
            api_types::restore_job::OverwriteBehavior::SkipIfExists => "skip_if_exists",
        };

        if let Some(storage_id) = req.destination_storage_id {
            let owns_storage = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM remote_storages WHERE id = $1 AND user_id = $2) as "exists!""#,
                storage_id,
                user_id,
            )
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            if !owns_storage {
                return Err(CoreError::forbidden(
                    "destination storage does not belong to the authenticated user",
                ));
            }
        }

        sqlx::query!(
            r#"
            INSERT INTO restore_jobs (
                id, user_id, backup_config_id, overwrite_behavior,
                destination_type, destination_storage_id, destination_prefix,
                status, started_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'running', $8)
            "#,
            id,
            user_id,
            req.backup_config_id,
            overwrite_str,
            req.destination_type,
            req.destination_storage_id,
            req.destination_prefix,
            req.started_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreateRestoreJobResponse { id })
    }

    async fn create_job_file(
        &self,
        user_id: Uuid,
        req: CreateRestoreJobFileRequest,
    ) -> CoreResult<CreateRestoreJobFileResponse> {
        let id = Uuid::now_v7();
        let _ = user_id;

        sqlx::query!(
            r#"
            INSERT INTO restore_job_files (id, job_id, remote_file_version_id, encrypted_name, name_nonce, blind_index, original_size, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
            "#,
            id,
            req.job_id,
            req.remote_file_version_id,
            req.encrypted_name,
            req.name_nonce,
            req.blind_index,
            req.original_size,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreateRestoreJobFileResponse { id })
    }

    async fn complete_job_file(
        &self,
        user_id: Uuid,
        req: CompleteRestoreJobFileRequest,
    ) -> CoreResult<()> {
        let _ = user_id;

        sqlx::query!(
            r#"
            UPDATE restore_job_files
            SET restored_size = $2,
                total_chunks = $3,
                chunks_completed = $3,
                status = $4,
                destination_path = $5,
                error_message = $6
            WHERE id = $1
            "#,
            req.id,
            req.restored_size,
            req.total_chunks,
            req.status,
            req.destination_path,
            req.error_message,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn complete_job(&self, user_id: Uuid, req: CompleteRestoreJobRequest) -> CoreResult<()> {
        let _ = user_id;

        sqlx::query!(
            r#"
            UPDATE restore_jobs
            SET status = $2,
                error_message = $3,
                completed_at = NOW(),
                total_files = (SELECT COUNT(*) FROM restore_job_files WHERE job_id = $1)::INTEGER,
                total_files_succeeded = (SELECT COUNT(*) FROM restore_job_files WHERE job_id = $1 AND status IN ('completed', 'skipped'))::INTEGER,
                total_files_failed = (SELECT COUNT(*) FROM restore_job_files WHERE job_id = $1 AND status = 'failed')::INTEGER,
                original_bytes = (SELECT COALESCE(SUM(original_size), 0) FROM restore_job_files WHERE job_id = $1),
                restored_bytes = (SELECT COALESCE(SUM(restored_size), 0) FROM restore_job_files WHERE job_id = $1)
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
        req: ListRestoreJobsRequest,
    ) -> CoreResult<ListRestoreJobsResponse> {
        let offset = (req.page - 1) * req.page_size;

        let rows = sqlx::query!(
            r#"
            SELECT id, backup_config_id, overwrite_behavior,
                   destination_type, destination_storage_id, destination_prefix,
                   status,
                   total_files, original_bytes, restored_bytes, started_at, completed_at
            FROM restore_jobs
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
            r#"SELECT COUNT(*) as "count!" FROM restore_jobs WHERE user_id = $1"#,
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let jobs = rows
            .into_iter()
            .map(|r| RestoreJobSummary {
                id: r.id,
                backup_config_id: r.backup_config_id,
                overwrite_behavior: r.overwrite_behavior,
                destination_type: r.destination_type,
                destination_storage_id: r.destination_storage_id,
                destination_prefix: r.destination_prefix,
                status: r.status,
                total_files: r.total_files,
                original_bytes: r.original_bytes,
                restored_bytes: r.restored_bytes,
                started_at: r.started_at,
                completed_at: r.completed_at,
            })
            .collect();

        Ok(ListRestoreJobsResponse { jobs, total_count })
    }

    async fn get_job_detail(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> CoreResult<GetRestoreJobDetailResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, overwrite_behavior,
                   destination_type, destination_storage_id, destination_prefix,
                   status,
                   total_files, total_files_succeeded, total_files_failed,
                   original_bytes, restored_bytes, error_message, started_at, completed_at
            FROM restore_jobs
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
            SELECT id, encrypted_name, name_nonce, blind_index, destination_path, original_size, restored_size,
                   total_chunks, chunks_completed, status, error_message
            FROM restore_job_files
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
            .map(|f| RestoreJobFileSummary {
                id: f.id,
                encrypted_name: f.encrypted_name,
                name_nonce: f.name_nonce,
                blind_index: f.blind_index,
                destination_path: f.destination_path,
                original_size: f.original_size,
                restored_size: f.restored_size,
                total_chunks: f.total_chunks,
                chunks_completed: f.chunks_completed,
                status: f.status,
                error_message: f.error_message,
            })
            .collect();

        Ok(GetRestoreJobDetailResponse {
            job: RestoreJobDetail {
                id: row.id,
                backup_config_id: row.backup_config_id,
                overwrite_behavior: row.overwrite_behavior,
                destination_type: row.destination_type,
                destination_storage_id: row.destination_storage_id,
                destination_prefix: row.destination_prefix,
                status: row.status,
                total_files: row.total_files,
                total_files_succeeded: row.total_files_succeeded,
                total_files_failed: row.total_files_failed,
                original_bytes: row.original_bytes,
                restored_bytes: row.restored_bytes,
                error_message: row.error_message,
                started_at: row.started_at,
                completed_at: row.completed_at,
                files,
            },
        })
    }

    async fn get_latest_job(&self, user_id: Uuid) -> CoreResult<GetLatestRestoreJobResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, overwrite_behavior,
                   destination_type, destination_storage_id, destination_prefix,
                   status,
                   total_files, original_bytes, restored_bytes, started_at, completed_at
            FROM restore_jobs
            WHERE user_id = $1
            ORDER BY started_at DESC
            LIMIT 1
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let job = row.map(|r| RestoreJobSummary {
            id: r.id,
            backup_config_id: r.backup_config_id,
            overwrite_behavior: r.overwrite_behavior,
            destination_type: r.destination_type,
            destination_storage_id: r.destination_storage_id,
            destination_prefix: r.destination_prefix,
            status: r.status,
            total_files: r.total_files,
            original_bytes: r.original_bytes,
            restored_bytes: r.restored_bytes,
            started_at: r.started_at,
            completed_at: r.completed_at,
        });

        Ok(GetLatestRestoreJobResponse { job })
    }

    /// Finds a restore job with status 'running' and its non-completed file entries.
    /// Returns None if no running restore job exists.
    async fn get_resumable_job(&self, user_id: Uuid) -> CoreResult<GetResumableRestoreJobResponse> {
        let job_row = sqlx::query!(
            r#"
            SELECT id, backup_config_id, overwrite_behavior,
                   destination_type, destination_storage_id, destination_prefix,
                   status,
                   total_files, original_bytes, restored_bytes, started_at, completed_at
            FROM restore_jobs
            WHERE user_id = $1 AND status = 'running'
            ORDER BY started_at DESC
            LIMIT 1
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let Some(r) = job_row else {
            return Ok(GetResumableRestoreJobResponse { job: None });
        };

        let job_id = r.id;
        let job_summary = RestoreJobSummary {
            id: r.id,
            backup_config_id: r.backup_config_id,
            overwrite_behavior: r.overwrite_behavior,
            destination_type: r.destination_type,
            destination_storage_id: r.destination_storage_id,
            destination_prefix: r.destination_prefix,
            status: r.status,
            total_files: r.total_files,
            original_bytes: r.original_bytes,
            restored_bytes: r.restored_bytes,
            started_at: r.started_at,
            completed_at: r.completed_at,
        };

        let pending_rows = sqlx::query!(
            r#"
            SELECT id, encrypted_name, name_nonce, blind_index, destination_path, original_size, restored_size,
                   total_chunks, chunks_completed, status, error_message
            FROM restore_job_files
            WHERE job_id = $1 AND status NOT IN ('completed', 'skipped')
            ORDER BY blind_index ASC
            "#,
            job_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let pending_files = pending_rows
            .into_iter()
            .map(|f| RestoreJobFileSummary {
                id: f.id,
                encrypted_name: f.encrypted_name,
                name_nonce: f.name_nonce,
                blind_index: f.blind_index,
                destination_path: f.destination_path,
                original_size: f.original_size,
                restored_size: f.restored_size,
                total_chunks: f.total_chunks,
                chunks_completed: f.chunks_completed,
                status: f.status,
                error_message: f.error_message,
            })
            .collect();

        Ok(GetResumableRestoreJobResponse {
            job: Some(ResumableRestoreJob {
                job: job_summary,
                pending_files,
            }),
        })
    }
}
