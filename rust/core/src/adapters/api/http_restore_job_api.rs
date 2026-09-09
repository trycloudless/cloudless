use api_types::restore_job::{
    CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
    CreateRestoreJobFileResponse, CreateRestoreJobRequest, CreateRestoreJobResponse,
    GetLatestRestoreJobResponse, GetRestoreJobDetailRequest, GetRestoreJobDetailResponse,
    GetResumableRestoreJobResponse, ListRestoreJobsRequest, ListRestoreJobsResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, restore_job_api_port::RestoreJobApiPort},
};

#[derive(Clone)]
pub struct HttpRestoreJobApi {
    api: HttpApi,
}

impl HttpRestoreJobApi {
    pub fn new(api: HttpApi) -> Self {
        HttpRestoreJobApi { api }
    }
}

#[async_trait]
impl RestoreJobApiPort for HttpRestoreJobApi {
    async fn create_job(
        &self,
        req: CreateRestoreJobRequest,
    ) -> ApiResult<CreateRestoreJobResponse> {
        let url = self.api.get_url("api/restore_job/create")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn create_job_file(
        &self,
        req: CreateRestoreJobFileRequest,
    ) -> ApiResult<CreateRestoreJobFileResponse> {
        let url = self.api.get_url("api/restore_job/file/create")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn complete_job_file(&self, req: CompleteRestoreJobFileRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/restore_job/file/complete")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn complete_job(&self, req: CompleteRestoreJobRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/restore_job/complete")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn list_jobs(&self, req: ListRestoreJobsRequest) -> ApiResult<ListRestoreJobsResponse> {
        let url = self.api.get_url("api/restore_job/list")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn get_job_detail(
        &self,
        req: GetRestoreJobDetailRequest,
    ) -> ApiResult<GetRestoreJobDetailResponse> {
        let url = self.api.get_url("api/restore_job/detail")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn get_latest_job(&self) -> ApiResult<GetLatestRestoreJobResponse> {
        let url = self.api.get_url("api/restore_job/latest")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn get_resumable_job(&self) -> ApiResult<GetResumableRestoreJobResponse> {
        let url = self.api.get_url("api/restore_job/resumable")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }
}
