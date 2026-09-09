use api_types::remote_storage::{
    CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageRequest,
    GetRemoteStorageResponse, ListRemoteStoragesResponse, ReauthRemoteStorageRequest,
    ReauthRemoteStorageResponse, UpdateRemoteStorageStatusRequest,
    UpdateRemoteStorageStatusResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait RemoteStorageApiPort: Send + Sync {
    async fn create(
        &self,
        storage: CreateRemoteStorageRequest,
    ) -> ApiResult<CreateRemoteStorageResponse>;

    async fn get_by_id(
        &self,
        request: GetRemoteStorageRequest,
    ) -> ApiResult<GetRemoteStorageResponse>;

    async fn list(&self) -> ApiResult<ListRemoteStoragesResponse>;

    /// Update the status of a remote storage (e.g. mark as AuthTokenExpired).
    async fn update_status(
        &self,
        request: UpdateRemoteStorageStatusRequest,
    ) -> ApiResult<UpdateRemoteStorageStatusResponse>;

    /// Reauth a remote storage with new encrypted credentials.
    /// Called after client-side identity verification succeeds.
    async fn reauth(
        &self,
        request: ReauthRemoteStorageRequest,
    ) -> ApiResult<ReauthRemoteStorageResponse>;
}
