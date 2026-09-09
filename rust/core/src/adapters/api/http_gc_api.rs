use api_types::gc::{
    ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectRequest,
    GcCollectResponse, GcRunDetailResponse, GetGcRunDetailRequest, GetRetentionSettingsResponse,
    ListGcRunsRequest, ListGcRunsResponse, UpdateRetentionSettingsRequest,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, gc_api_port::GcApiPort},
};

#[derive(Clone)]
pub struct HttpGcApi {
    api: HttpApi,
}

impl HttpGcApi {
    pub fn new(api: HttpApi) -> Self {
        HttpGcApi { api }
    }
}

#[async_trait]
impl GcApiPort for HttpGcApi {
    async fn collect(&self) -> ApiResult<GcCollectResponse> {
        let url = self.api.get_url("api/gc/collect")?;
        let request = self.api.client.post(url).json(&GcCollectRequest {});
        self.api.send_with_auth(request).await
    }

    async fn confirm_chunk_deletions(
        &self,
        request: ConfirmChunkDeletionsRequest,
    ) -> ApiResult<ConfirmChunkDeletionsResponse> {
        let url = self.api.get_url("api/gc/confirm_chunk_deletions")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_runs(&self, request: ListGcRunsRequest) -> ApiResult<ListGcRunsResponse> {
        let url = self.api.get_url("api/gc/list_runs")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get_run_detail(
        &self,
        request: GetGcRunDetailRequest,
    ) -> ApiResult<GcRunDetailResponse> {
        let url = self.api.get_url("api/gc/run_detail")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get_retention_settings(&self) -> ApiResult<GetRetentionSettingsResponse> {
        let url = self.api.get_url("api/gc/retention_settings")?;
        let request = self.api.client.post(url);
        self.api.send_with_auth(request).await
    }

    async fn update_retention_settings(
        &self,
        request: UpdateRetentionSettingsRequest,
    ) -> ApiResult<()> {
        let url = self.api.get_url("api/gc/update_retention_settings")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }
}
