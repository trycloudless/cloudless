use api_types::local_device::{
    CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
    GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
    GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
    ListDevicesByPlatformResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, local_device_api_port::LocalDeviceApiPort},
};

#[derive(Clone)]
pub struct HttpLocalDeviceApi {
    api: HttpApi,
}

impl HttpLocalDeviceApi {
    pub fn new(api: HttpApi) -> Self {
        HttpLocalDeviceApi { api }
    }
}

#[async_trait]
impl LocalDeviceApiPort for HttpLocalDeviceApi {
    async fn create(
        &self,
        local_device: CreateLocalDeviceRequest,
    ) -> ApiResult<CreateLocalDeviceResponse> {
        let url = self.api.get_url("api/local_device/create")?;
        let request = self.api.client.post(url).json(&local_device);
        self.api.send_with_auth(request).await
    }

    async fn get_by_physical_id(
        &self,
        request: GetLocalDeviceByPhysicalIdRequest,
    ) -> ApiResult<GetLocalDeviceByPhysicalIdResponse> {
        let url = self.api.get_url("api/local_device/get")?;
        let req = self.api.client.post(url).json(&request);
        self.api.send_with_auth(req).await
    }

    async fn get_or_create(
        &self,
        request: GetOrCreateLocalDeviceRequest,
    ) -> ApiResult<GetOrCreateLocalDeviceResponse> {
        let url = self.api.get_url("api/local_device/get-or-create")?;
        let req = self.api.client.post(url).json(&request);
        self.api.send_with_auth(req).await
    }

    async fn list_by_platform(
        &self,
        request: ListDevicesByPlatformRequest,
    ) -> ApiResult<ListDevicesByPlatformResponse> {
        let url = self.api.get_url("api/local_device/list-by-platform")?;
        let req = self.api.client.post(url).json(&request);
        self.api.send_with_auth(req).await
    }

    async fn list_all(&self) -> ApiResult<ListAllDevicesResponse> {
        let url = self.api.get_url("api/local_device/list-all")?;
        let req = self.api.client.post(url);
        self.api.send_with_auth(req).await
    }
}
