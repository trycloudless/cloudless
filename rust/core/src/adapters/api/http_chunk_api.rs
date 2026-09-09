use api_types::{
    chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
    restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, chunk_api_port::ChunkApiPort},
};

#[derive(Clone)]
pub struct HttpChunkApi {
    api: HttpApi,
}

impl HttpChunkApi {
    pub fn new(api: HttpApi) -> Self {
        HttpChunkApi { api }
    }
}

#[async_trait]
impl ChunkApiPort for HttpChunkApi {
    async fn create(&self, request: CreateChunkRequest) -> ApiResult<CreateChunkResponse> {
        let url = self.api.get_url("api/chunk/create")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get_chunks_for_version(
        &self,
        req: GetFileVersionChunksRequest,
    ) -> ApiResult<GetFileVersionChunksResponse> {
        let url = self.api.get_url("api/chunk/version-chunks")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn update_storage_meta(&self, req: UpdateChunkStorageMetaRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/chunk/update-storage-meta")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }
}
