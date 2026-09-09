use api_types::remote_storage::{
    CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageRequest,
    GetRemoteStorageResponse, ListRemoteStoragesResponse, ReauthRemoteStorageRequest,
    ReauthRemoteStorageResponse, UpdateRemoteStorageStatusRequest,
    UpdateRemoteStorageStatusResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, remote_storage_api_port::RemoteStorageApiPort},
};

#[derive(Clone)]
pub struct HttpRemoteStorageApi {
    api: HttpApi,
}

impl HttpRemoteStorageApi {
    pub fn new(api: HttpApi) -> Self {
        HttpRemoteStorageApi { api }
    }
}

#[async_trait]
impl RemoteStorageApiPort for HttpRemoteStorageApi {
    async fn create(
        &self,
        request: CreateRemoteStorageRequest,
    ) -> ApiResult<CreateRemoteStorageResponse> {
        let url = self.api.get_url("api/remote_storage/create")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get_by_id(
        &self,
        request: GetRemoteStorageRequest,
    ) -> ApiResult<GetRemoteStorageResponse> {
        let url = self.api.get_url("api/remote_storage/get")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list(&self) -> ApiResult<ListRemoteStoragesResponse> {
        let url = self.api.get_url("api/remote_storage/list")?;
        let request = self.api.client.post(url);
        self.api.send_with_auth(request).await
    }

    async fn update_status(
        &self,
        request: UpdateRemoteStorageStatusRequest,
    ) -> ApiResult<UpdateRemoteStorageStatusResponse> {
        let url = self.api.get_url("api/remote_storage/update_status")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn reauth(
        &self,
        request: ReauthRemoteStorageRequest,
    ) -> ApiResult<ReauthRemoteStorageResponse> {
        let url = self.api.get_url("api/remote_storage/reauth")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }
}
