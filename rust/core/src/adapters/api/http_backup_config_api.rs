use api_types::backup_config::{
    CreateBackupConfigRequest, CreateBackupConfigResponse, GetBackupConfigRequest,
    GetBackupConfigResponse, ListAllBackupConfigsResponse,
    ListBackupConfigWithRemoteStorageRequest, ListBackupConfigWithRemoteStorageResponse,
    RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
    ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
    UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, backup_config_api_port::BackupConfigApiPort},
};

#[derive(Clone)]
pub struct HttpBackupConfigApi {
    api: HttpApi,
}

impl HttpBackupConfigApi {
    pub fn new(api: HttpApi) -> Self {
        HttpBackupConfigApi { api }
    }
}

#[async_trait]
impl BackupConfigApiPort for HttpBackupConfigApi {
    async fn create(
        &self,
        request: CreateBackupConfigRequest,
    ) -> ApiResult<CreateBackupConfigResponse> {
        let url = self.api.get_url("api/backup_config/create")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_with_remote_storage(
        &self,
        request: ListBackupConfigWithRemoteStorageRequest,
    ) -> ApiResult<ListBackupConfigWithRemoteStorageResponse> {
        let url = self
            .api
            .get_url("api/backup_config/list_config_with_storage")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get_by_id(
        &self,
        request: GetBackupConfigRequest,
    ) -> ApiResult<GetBackupConfigResponse> {
        let url = self.api.get_url("api/backup_config/get")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list_all(&self) -> ApiResult<ListAllBackupConfigsResponse> {
        let url = self.api.get_url("api/backup_config/list_all")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn toggle_active(
        &self,
        request: ToggleBackupConfigRequest,
    ) -> ApiResult<ToggleBackupConfigResponse> {
        let url = self.api.get_url("api/backup_config/toggle-active")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn rename(
        &self,
        request: RenameBackupConfigRequest,
    ) -> ApiResult<RenameBackupConfigResponse> {
        let url = self.api.get_url("api/backup_config/rename")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn update_cleanup_type(
        &self,
        request: UpdateCleanupTypeRequest,
    ) -> ApiResult<UpdateCleanupTypeResponse> {
        let url = self.api.get_url("api/backup_config/update-cleanup-type")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn update_exclusion_config(
        &self,
        request: UpdateExclusionConfigRequest,
    ) -> ApiResult<UpdateExclusionConfigResponse> {
        let url = self.api.get_url("api/backup_config/update-exclusions")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }
}
