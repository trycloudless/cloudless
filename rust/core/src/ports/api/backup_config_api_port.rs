use api_types::backup_config::{
    CreateBackupConfigRequest, CreateBackupConfigResponse, GetBackupConfigRequest,
    GetBackupConfigResponse, ListAllBackupConfigsResponse,
    ListBackupConfigWithRemoteStorageRequest, ListBackupConfigWithRemoteStorageResponse,
    RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
    ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
    UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait BackupConfigApiPort: Send + Sync {
    async fn create(
        &self,
        local_file: CreateBackupConfigRequest,
    ) -> ApiResult<CreateBackupConfigResponse>;

    async fn list_with_remote_storage(
        &self,
        local_file: ListBackupConfigWithRemoteStorageRequest,
    ) -> ApiResult<ListBackupConfigWithRemoteStorageResponse>;

    async fn get_by_id(
        &self,
        request: GetBackupConfigRequest,
    ) -> ApiResult<GetBackupConfigResponse>;

    async fn list_all(&self) -> ApiResult<ListAllBackupConfigsResponse>;

    async fn toggle_active(
        &self,
        request: ToggleBackupConfigRequest,
    ) -> ApiResult<ToggleBackupConfigResponse>;

    async fn rename(
        &self,
        request: RenameBackupConfigRequest,
    ) -> ApiResult<RenameBackupConfigResponse>;

    async fn update_cleanup_type(
        &self,
        request: UpdateCleanupTypeRequest,
    ) -> ApiResult<UpdateCleanupTypeResponse>;

    async fn update_exclusion_config(
        &self,
        request: UpdateExclusionConfigRequest,
    ) -> ApiResult<UpdateExclusionConfigResponse>;
}
