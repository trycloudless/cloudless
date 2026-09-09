use api_types::backup_job::{
    AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
    CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
    CreateBackupJobResponse, GetBackupJobDetailRequest, GetBackupJobDetailResponse,
    GetLatestBackupJobResponse, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
    ListBackupJobsRequest, ListBackupJobsResponse, LogCleanupFilesRequest,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, backup_job_api_port::BackupJobApiPort},
};

#[derive(Clone)]
pub struct HttpBackupJobApi {
    api: HttpApi,
}

impl HttpBackupJobApi {
    pub fn new(api: HttpApi) -> Self {
        HttpBackupJobApi { api }
    }
}

#[async_trait]
impl BackupJobApiPort for HttpBackupJobApi {
    async fn create_job(&self, req: CreateBackupJobRequest) -> ApiResult<CreateBackupJobResponse> {
        let url = self.api.get_url("api/backup_job/create")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn create_job_file(
        &self,
        req: CreateBackupJobFileRequest,
    ) -> ApiResult<CreateBackupJobFileResponse> {
        let url = self.api.get_url("api/backup_job/file/create")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn complete_job_file(&self, req: CompleteBackupJobFileRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/backup_job/file/complete")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn complete_job(&self, req: CompleteBackupJobRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/backup_job/complete")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn list_jobs(&self, req: ListBackupJobsRequest) -> ApiResult<ListBackupJobsResponse> {
        let url = self.api.get_url("api/backup_job/list")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn get_job_detail(
        &self,
        req: GetBackupJobDetailRequest,
    ) -> ApiResult<GetBackupJobDetailResponse> {
        let url = self.api.get_url("api/backup_job/detail")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn get_latest_job(&self) -> ApiResult<GetLatestBackupJobResponse> {
        let url = self.api.get_url("api/backup_job/latest")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn get_resumable_job(
        &self,
        req: GetResumableBackupJobRequest,
    ) -> ApiResult<GetResumableBackupJobResponse> {
        let url = self.api.get_url("api/backup_job/resumable")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn log_cleanup_files(&self, req: LogCleanupFilesRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/backup_job/cleanup/log")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse> {
        let url = self.api.get_url("api/backup_job/abandon_stale")?;
        let request = self.api.client.post(url);
        self.api.send_with_auth(request).await
    }
}
