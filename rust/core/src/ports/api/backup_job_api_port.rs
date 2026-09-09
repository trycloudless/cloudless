use api_types::backup_job::{
    AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
    CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
    CreateBackupJobResponse, GetBackupJobDetailRequest, GetBackupJobDetailResponse,
    GetLatestBackupJobResponse, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
    ListBackupJobsRequest, ListBackupJobsResponse, LogCleanupFilesRequest,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait BackupJobApiPort: Send + Sync {
    async fn create_job(&self, req: CreateBackupJobRequest) -> ApiResult<CreateBackupJobResponse>;
    async fn create_job_file(
        &self,
        req: CreateBackupJobFileRequest,
    ) -> ApiResult<CreateBackupJobFileResponse>;
    async fn complete_job_file(&self, req: CompleteBackupJobFileRequest) -> ApiResult<()>;
    async fn complete_job(&self, req: CompleteBackupJobRequest) -> ApiResult<()>;
    async fn list_jobs(&self, req: ListBackupJobsRequest) -> ApiResult<ListBackupJobsResponse>;
    async fn get_job_detail(
        &self,
        req: GetBackupJobDetailRequest,
    ) -> ApiResult<GetBackupJobDetailResponse>;
    async fn get_latest_job(&self) -> ApiResult<GetLatestBackupJobResponse>;
    async fn get_resumable_job(
        &self,
        req: GetResumableBackupJobRequest,
    ) -> ApiResult<GetResumableBackupJobResponse>;

    async fn log_cleanup_files(&self, req: LogCleanupFilesRequest) -> ApiResult<()>;

    async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse>;
}
