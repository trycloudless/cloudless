use api_types::remote_file_version::{
    CreateFileVersionRequest, CreateFileVersionResponse, ListAllVersionsRequest,
    ListAllVersionsResponse, ListBackedUpFilesRequest, ListBackedUpFilesResponse,
    ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
    MoveVersionToBinRequest, RestoreVersionFromBinRequest, UpdateFileVersionStatusRequest,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, remote_file_version_api_port::RemoteFileVersionApiPort},
};

#[derive(Clone)]
pub struct HttpRemoteFileVersionApi {
    api: HttpApi,
}

impl HttpRemoteFileVersionApi {
    pub fn new(api: HttpApi) -> Self {
        HttpRemoteFileVersionApi { api }
    }
}

#[async_trait]
impl RemoteFileVersionApiPort for HttpRemoteFileVersionApi {
    async fn create(
        &self,
        request: CreateFileVersionRequest,
    ) -> ApiResult<CreateFileVersionResponse> {
        let url = self.api.get_url("api/remote_file_version/create")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn update_status(&self, request: UpdateFileVersionStatusRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/remote_file_version/update_status")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_backed_up_files(
        &self,
        request: ListBackedUpFilesRequest,
    ) -> ApiResult<ListBackedUpFilesResponse> {
        let url = self
            .api
            .get_url("api/remote_file_version/list_backed_up_files")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_all_versions(
        &self,
        request: ListAllVersionsRequest,
    ) -> ApiResult<ListAllVersionsResponse> {
        let url = self
            .api
            .get_url("api/remote_file_version/list_all_versions")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn move_to_bin(&self, request: MoveVersionToBinRequest) -> ApiResult<()> {
        let url = self.api.get_url("api/remote_file_version/move_to_bin")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn move_all_to_bin(&self, request: MoveAllVersionsToBinRequest) -> ApiResult<()> {
        let url = self
            .api
            .get_url("api/remote_file_version/move_all_to_bin")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn restore_from_bin(&self, request: RestoreVersionFromBinRequest) -> ApiResult<()> {
        let url = self
            .api
            .get_url("api/remote_file_version/restore_from_bin")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_bin_versions(
        &self,
        request: ListBinVersionsRequest,
    ) -> ApiResult<ListBinVersionsResponse> {
        let url = self.api.get_url("api/remote_file_version/list_bin")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }
}
