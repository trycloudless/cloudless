use api_types::local_device::{
    CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
    GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
    GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
    ListDevicesByPlatformResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait LocalDeviceApiPort: Send + Sync {
    async fn create(
        &self,
        local_device: CreateLocalDeviceRequest,
    ) -> ApiResult<CreateLocalDeviceResponse>;

    async fn get_by_physical_id(
        &self,
        request: GetLocalDeviceByPhysicalIdRequest,
    ) -> ApiResult<GetLocalDeviceByPhysicalIdResponse>;

    async fn get_or_create(
        &self,
        request: GetOrCreateLocalDeviceRequest,
    ) -> ApiResult<GetOrCreateLocalDeviceResponse>;

    async fn list_by_platform(
        &self,
        request: ListDevicesByPlatformRequest,
    ) -> ApiResult<ListDevicesByPlatformResponse>;

    async fn list_all(&self) -> ApiResult<ListAllDevicesResponse>;
}
