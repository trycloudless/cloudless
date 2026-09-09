use api_types::restore_job::{
    CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
    CreateRestoreJobFileResponse, CreateRestoreJobRequest, CreateRestoreJobResponse,
    GetLatestRestoreJobResponse, GetRestoreJobDetailRequest, GetRestoreJobDetailResponse,
    GetResumableRestoreJobResponse, ListRestoreJobsRequest, ListRestoreJobsResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait RestoreJobApiPort: Send + Sync {
    async fn create_job(&self, req: CreateRestoreJobRequest)
    -> ApiResult<CreateRestoreJobResponse>;
    async fn create_job_file(
        &self,
        req: CreateRestoreJobFileRequest,
    ) -> ApiResult<CreateRestoreJobFileResponse>;
    async fn complete_job_file(&self, req: CompleteRestoreJobFileRequest) -> ApiResult<()>;
    async fn complete_job(&self, req: CompleteRestoreJobRequest) -> ApiResult<()>;
    async fn list_jobs(&self, req: ListRestoreJobsRequest) -> ApiResult<ListRestoreJobsResponse>;
    async fn get_job_detail(
        &self,
        req: GetRestoreJobDetailRequest,
    ) -> ApiResult<GetRestoreJobDetailResponse>;
    async fn get_latest_job(&self) -> ApiResult<GetLatestRestoreJobResponse>;
    async fn get_resumable_job(&self) -> ApiResult<GetResumableRestoreJobResponse>;
}
