use api_types::{
    backup_config::{
        CreateBackupConfigRequest, CreateBackupConfigResponse, ListAllBackupConfigsResponse,
        ListBackupConfigWithRemoteStorageRequest, ListBackupConfigWithRemoteStorageResponse,
        RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
        ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
        UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
    },
    local_device::{
        CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
        GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
        GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
        ListDevicesByPlatformResponse,
    },
    remote_storage::{
        CreateRemoteStorageRequest, CreateRemoteStorageResponse, ListRemoteStoragesResponse,
    },
};

use tracing::{debug, info};

use crate::{
    applications::config::env::ConfigEnv,
    model::base::AppResult,
    ports::api::{
        backup_config_api_port::BackupConfigApiPort, local_device_api_port::LocalDeviceApiPort,
        remote_storage_api_port::RemoteStorageApiPort,
    },
};

pub async fn register_local_device<E: ConfigEnv>(
    env: &E,
    request: CreateLocalDeviceRequest,
) -> AppResult<CreateLocalDeviceResponse> {
    let local_device_api = env.local_device_api();
    let response = local_device_api.create(request).await?;
    info!(device_id = %response.id, display_name = ?response.display_name, platform = %response.platform, "Local device registered");
    Ok(response)
}

pub async fn register_remote_storage<E: ConfigEnv>(
    env: &E,
    request: CreateRemoteStorageRequest,
) -> AppResult<CreateRemoteStorageResponse> {
    let name = request.name.clone();
    let storage_type = request.storage_type.clone();
    let remote_storage_api = env.remote_storage_api();
    let response = remote_storage_api.create(request).await?;
    info!(storage_id = %response.id, name = %name, storage_type = ?storage_type, "Remote storage registered");
    Ok(response)
}

pub async fn create_backup_config<E: ConfigEnv>(
    env: &E,
    request: CreateBackupConfigRequest,
) -> AppResult<CreateBackupConfigResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.create(request).await?;
    info!(config_id = %response.id, "Backup config created");
    Ok(response)
}

pub async fn get_local_device<E: ConfigEnv>(
    env: &E,
    physical_device_id: String,
) -> AppResult<GetLocalDeviceByPhysicalIdResponse> {
    debug!(physical_device_id = %physical_device_id, "Looking up local device");
    let local_device_api = env.local_device_api();
    let response = local_device_api
        .get_by_physical_id(GetLocalDeviceByPhysicalIdRequest { physical_device_id })
        .await?;
    Ok(response)
}

pub async fn get_or_create_local_device<E: ConfigEnv>(
    env: &E,
    request: GetOrCreateLocalDeviceRequest,
) -> AppResult<GetOrCreateLocalDeviceResponse> {
    debug!(physical_device_id = %request.physical_device_id, "Getting or creating local device");
    let local_device_api = env.local_device_api();
    let response = local_device_api.get_or_create(request).await?;
    debug!(device_id = %response.id, was_created = response.was_created, "Device resolved");
    Ok(response)
}

pub async fn list_remote_storages<E: ConfigEnv>(env: &E) -> AppResult<ListRemoteStoragesResponse> {
    let remote_storage_api = env.remote_storage_api();
    let response = remote_storage_api.list().await?;
    debug!(count = response.list.len(), "Listed remote storages");
    Ok(response)
}

pub async fn list_backup_configs<E: ConfigEnv>(
    env: &E,
    physical_device_id: String,
) -> AppResult<ListBackupConfigWithRemoteStorageResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api
        .list_with_remote_storage(ListBackupConfigWithRemoteStorageRequest { physical_device_id })
        .await?;
    debug!(
        count = response.list.len(),
        "Listed backup configs for device"
    );
    Ok(response)
}

pub async fn list_all_backup_configs<E: ConfigEnv>(
    env: &E,
) -> AppResult<ListAllBackupConfigsResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.list_all().await?;
    debug!(count = response.list.len(), "Listed all backup configs");
    Ok(response)
}

pub async fn list_devices_by_platform<E: ConfigEnv>(
    env: &E,
    request: ListDevicesByPlatformRequest,
) -> AppResult<ListDevicesByPlatformResponse> {
    let local_device_api = env.local_device_api();
    let response = local_device_api.list_by_platform(request).await?;
    debug!(count = response.devices.len(), "Listed devices by platform");
    Ok(response)
}

pub async fn list_all_devices<E: ConfigEnv>(env: &E) -> AppResult<ListAllDevicesResponse> {
    let local_device_api = env.local_device_api();
    let response = local_device_api.list_all().await?;
    debug!(count = response.devices.len(), "Listed all devices");
    Ok(response)
}

pub async fn toggle_backup_config<E: ConfigEnv>(
    env: &E,
    request: ToggleBackupConfigRequest,
) -> AppResult<ToggleBackupConfigResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.toggle_active(request).await?;
    info!(config_id = %response.id, is_active = response.is_active, "Backup config toggled");
    Ok(response)
}

pub async fn rename_backup_config<E: ConfigEnv>(
    env: &E,
    request: RenameBackupConfigRequest,
) -> AppResult<RenameBackupConfigResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.rename(request).await?;
    info!(config_id = %response.id, display_name = %response.display_name, "Backup config renamed");
    Ok(response)
}

pub async fn update_cleanup_type<E: ConfigEnv>(
    env: &E,
    request: UpdateCleanupTypeRequest,
) -> AppResult<UpdateCleanupTypeResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.update_cleanup_type(request).await?;
    info!(config_id = %response.id, "Backup config cleanup type updated");
    Ok(response)
}

pub async fn update_exclusion_config<E: ConfigEnv>(
    env: &E,
    request: UpdateExclusionConfigRequest,
) -> AppResult<UpdateExclusionConfigResponse> {
    let backup_config_api = env.backup_config_api();
    let response = backup_config_api.update_exclusion_config(request).await?;
    info!(config_id = %response.id, "Backup config exclusions updated");
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use uuid::Uuid;

    use crate::ports::api::{ApiClientError, ApiResult};
    use api_types::{
        backup_config::{
            BackupConfigWithRemoteStorage, GetBackupConfigRequest, GetBackupConfigResponse,
            RenameBackupConfigRequest, RenameBackupConfigResponse, UpdateCleanupTypeRequest,
            UpdateCleanupTypeResponse, UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
        },
        common::{Base64EncryptedData, EncryptionAlgorithm},
        local_device::DeviceSummary,
        remote_storage::{
            GetRemoteStorageRequest, GetRemoteStorageResponse, ReauthRemoteStorageRequest,
            ReauthRemoteStorageResponse, RemoteStorageStatus, RemoteStorageSummary,
            RemoteStorageType, UpdateRemoteStorageStatusRequest, UpdateRemoteStorageStatusResponse,
        },
    };

    // --- Mock implementations ---

    struct MockLocalDeviceApi {
        should_fail: bool,
    }

    #[async_trait]
    impl LocalDeviceApiPort for MockLocalDeviceApi {
        async fn create(
            &self,
            req: CreateLocalDeviceRequest,
        ) -> ApiResult<CreateLocalDeviceResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("create device failed".into()));
            }
            Ok(CreateLocalDeviceResponse {
                id: Uuid::now_v7(),
                physical_device_id: req.physical_device_id,
                display_name: req.display_name,
                platform: req.platform,
                created_at: Utc::now(),
            })
        }

        async fn get_by_physical_id(
            &self,
            req: GetLocalDeviceByPhysicalIdRequest,
        ) -> ApiResult<GetLocalDeviceByPhysicalIdResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("get device failed".into()));
            }
            Ok(GetLocalDeviceByPhysicalIdResponse {
                id: Uuid::now_v7(),
                physical_device_id: req.physical_device_id,
                display_name: Some("Test Device".into()),
                platform: "macos".into(),
                created_at: Utc::now(),
            })
        }

        async fn get_or_create(
            &self,
            req: GetOrCreateLocalDeviceRequest,
        ) -> ApiResult<GetOrCreateLocalDeviceResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("get_or_create failed".into()));
            }
            Ok(GetOrCreateLocalDeviceResponse {
                id: Uuid::now_v7(),
                physical_device_id: req.physical_device_id,
                display_name: req.display_name,
                platform: req.platform,
                created_at: Utc::now(),
                was_created: true,
            })
        }

        async fn list_by_platform(
            &self,
            req: ListDevicesByPlatformRequest,
        ) -> ApiResult<ListDevicesByPlatformResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("list by platform failed".into()));
            }
            Ok(ListDevicesByPlatformResponse {
                devices: vec![DeviceSummary {
                    id: Uuid::now_v7(),
                    physical_device_id: "phys-1".into(),
                    display_name: Some("Device 1".into()),
                    platform: req.platform,
                    created_at: Utc::now(),
                }],
            })
        }

        async fn list_all(&self) -> ApiResult<ListAllDevicesResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("list all devices failed".into()));
            }
            Ok(ListAllDevicesResponse { devices: vec![] })
        }
    }

    struct MockRemoteStorageApi {
        should_fail: bool,
    }

    #[async_trait]
    impl RemoteStorageApiPort for MockRemoteStorageApi {
        async fn create(
            &self,
            _req: CreateRemoteStorageRequest,
        ) -> ApiResult<CreateRemoteStorageResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("create storage failed".into()));
            }
            Ok(CreateRemoteStorageResponse { id: Uuid::now_v7() })
        }

        async fn get_by_id(
            &self,
            _req: GetRemoteStorageRequest,
        ) -> ApiResult<GetRemoteStorageResponse> {
            unimplemented!()
        }

        async fn list(&self) -> ApiResult<ListRemoteStoragesResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("list storages failed".into()));
            }
            Ok(ListRemoteStoragesResponse {
                list: vec![RemoteStorageSummary {
                    id: Uuid::now_v7(),
                    name: "My S3".into(),
                    storage_type: RemoteStorageType::Aws,
                    status: RemoteStorageStatus::Active,
                    created_at: Utc::now(),
                }],
            })
        }

        async fn update_status(
            &self,
            _req: UpdateRemoteStorageStatusRequest,
        ) -> ApiResult<UpdateRemoteStorageStatusResponse> {
            unimplemented!()
        }

        async fn reauth(
            &self,
            _req: ReauthRemoteStorageRequest,
        ) -> ApiResult<ReauthRemoteStorageResponse> {
            unimplemented!()
        }
    }

    struct MockBackupConfigApi {
        should_fail: bool,
    }

    fn dummy_encrypted_data() -> Base64EncryptedData {
        Base64EncryptedData {
            nonce: "test-nonce".into(),
            ciphertext: "test-ciphertext".into(),
            algorithm: EncryptionAlgorithm::Aes256Gcm,
        }
    }

    #[async_trait]
    impl BackupConfigApiPort for MockBackupConfigApi {
        async fn create(
            &self,
            _req: CreateBackupConfigRequest,
        ) -> ApiResult<CreateBackupConfigResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("create config failed".into()));
            }
            Ok(CreateBackupConfigResponse { id: Uuid::now_v7() })
        }

        async fn list_with_remote_storage(
            &self,
            _req: ListBackupConfigWithRemoteStorageRequest,
        ) -> ApiResult<ListBackupConfigWithRemoteStorageResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("list configs failed".into()));
            }
            Ok(ListBackupConfigWithRemoteStorageResponse {
                list: vec![BackupConfigWithRemoteStorage {
                    storage_id: Uuid::now_v7(),
                    encrypted_source_dir: vec![1, 2, 3],
                    source_dir_nonce: vec![4, 5, 6],
                    config_id: Uuid::now_v7(),
                    display_name: "Test Config".into(),
                    storage_type: RemoteStorageType::Aws,
                    config: dummy_encrypted_data(),
                    local_device_id: Some(Uuid::now_v7()),
                    physical_device_id: Some("phys-1".into()),
                    is_active: true,
                    cleanup_type: Default::default(),
                    encrypted_exclusion_config: None,
                    exclusion_config_nonce: None,
                }],
            })
        }

        async fn get_by_id(
            &self,
            _req: GetBackupConfigRequest,
        ) -> ApiResult<GetBackupConfigResponse> {
            unimplemented!()
        }

        async fn list_all(&self) -> ApiResult<ListAllBackupConfigsResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("list all configs failed".into()));
            }
            Ok(ListAllBackupConfigsResponse { list: vec![] })
        }

        async fn toggle_active(
            &self,
            req: ToggleBackupConfigRequest,
        ) -> ApiResult<ToggleBackupConfigResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("toggle failed".into()));
            }
            Ok(ToggleBackupConfigResponse {
                id: req.id,
                is_active: req.is_active,
            })
        }

        async fn rename(
            &self,
            _req: RenameBackupConfigRequest,
        ) -> ApiResult<RenameBackupConfigResponse> {
            unimplemented!()
        }

        async fn update_cleanup_type(
            &self,
            req: UpdateCleanupTypeRequest,
        ) -> ApiResult<UpdateCleanupTypeResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl(
                    "update cleanup type failed".into(),
                ));
            }
            Ok(UpdateCleanupTypeResponse {
                id: req.id,
                cleanup_type: req.cleanup_type,
            })
        }

        async fn update_exclusion_config(
            &self,
            _req: UpdateExclusionConfigRequest,
        ) -> ApiResult<UpdateExclusionConfigResponse> {
            unimplemented!()
        }
    }

    // --- Mock environment ---

    struct MockConfigEnv {
        device_api: MockLocalDeviceApi,
        storage_api: MockRemoteStorageApi,
        config_api: MockBackupConfigApi,
    }

    impl MockConfigEnv {
        fn new() -> Self {
            Self {
                device_api: MockLocalDeviceApi { should_fail: false },
                storage_api: MockRemoteStorageApi { should_fail: false },
                config_api: MockBackupConfigApi { should_fail: false },
            }
        }

        fn with_device_failure() -> Self {
            Self {
                device_api: MockLocalDeviceApi { should_fail: true },
                storage_api: MockRemoteStorageApi { should_fail: false },
                config_api: MockBackupConfigApi { should_fail: false },
            }
        }

        fn with_storage_failure() -> Self {
            Self {
                device_api: MockLocalDeviceApi { should_fail: false },
                storage_api: MockRemoteStorageApi { should_fail: true },
                config_api: MockBackupConfigApi { should_fail: false },
            }
        }

        fn with_config_failure() -> Self {
            Self {
                device_api: MockLocalDeviceApi { should_fail: false },
                storage_api: MockRemoteStorageApi { should_fail: false },
                config_api: MockBackupConfigApi { should_fail: true },
            }
        }
    }

    impl ConfigEnv for MockConfigEnv {
        type LocalDeviceApi = MockLocalDeviceApi;
        type RemoteStorageApi = MockRemoteStorageApi;
        type BackupConfigApi = MockBackupConfigApi;

        fn local_device_api(&self) -> &Self::LocalDeviceApi {
            &self.device_api
        }
        fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
            &self.storage_api
        }
        fn backup_config_api(&self) -> &Self::BackupConfigApi {
            &self.config_api
        }
    }

    // --- Tests: register_local_device ---

    #[tokio::test]
    async fn test_register_local_device_success() {
        let env = MockConfigEnv::new();
        let req = CreateLocalDeviceRequest {
            physical_device_id: "phys-123".into(),
            display_name: Some("My Mac".into()),
            platform: "macos".into(),
        };
        let result = register_local_device(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.physical_device_id, "phys-123");
        assert_eq!(resp.display_name, Some("My Mac".into()));
        assert_eq!(resp.platform, "macos");
    }

    #[tokio::test]
    async fn test_register_local_device_failure() {
        let env = MockConfigEnv::with_device_failure();
        let req = CreateLocalDeviceRequest {
            physical_device_id: "phys-123".into(),
            display_name: None,
            platform: "macos".into(),
        };
        let result = register_local_device(&env, req).await;
        assert!(result.is_err());
    }

    // --- Tests: register_remote_storage ---

    #[tokio::test]
    async fn test_register_remote_storage_success() {
        let env = MockConfigEnv::new();
        let req = CreateRemoteStorageRequest {
            name: "Test S3".into(),
            storage_type: RemoteStorageType::Aws,
            config: dummy_encrypted_data(),
        };
        let result = register_remote_storage(&env, req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_register_remote_storage_failure() {
        let env = MockConfigEnv::with_storage_failure();
        let req = CreateRemoteStorageRequest {
            name: "Test S3".into(),
            storage_type: RemoteStorageType::Aws,
            config: dummy_encrypted_data(),
        };
        let result = register_remote_storage(&env, req).await;
        assert!(result.is_err());
    }

    // --- Tests: create_backup_config ---

    #[tokio::test]
    async fn test_create_backup_config_success() {
        let env = MockConfigEnv::new();
        let req = CreateBackupConfigRequest {
            storage_id: Uuid::now_v7(),
            display_name: "Daily Backup".into(),
            local_device_id: Uuid::now_v7(),
            encrypted_source_dir: vec![1, 2, 3],
            source_dir_nonce: vec![4, 5, 6],
            source_dir_blind_index: vec![7, 8, 9],
            cleanup_type: Default::default(),
            encrypted_exclusion_config: None,
            exclusion_config_nonce: None,
        };
        let result = create_backup_config(&env, req).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_backup_config_failure() {
        let env = MockConfigEnv::with_config_failure();
        let req = CreateBackupConfigRequest {
            storage_id: Uuid::now_v7(),
            display_name: "Daily Backup".into(),
            local_device_id: Uuid::now_v7(),
            encrypted_source_dir: vec![1, 2, 3],
            source_dir_nonce: vec![4, 5, 6],
            source_dir_blind_index: vec![7, 8, 9],
            cleanup_type: Default::default(),
            encrypted_exclusion_config: None,
            exclusion_config_nonce: None,
        };
        let result = create_backup_config(&env, req).await;
        assert!(result.is_err());
    }

    // --- Tests: get_local_device ---

    #[tokio::test]
    async fn test_get_local_device_success() {
        let env = MockConfigEnv::new();
        let result = get_local_device(&env, "phys-abc".into()).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.physical_device_id, "phys-abc");
    }

    #[tokio::test]
    async fn test_get_local_device_failure() {
        let env = MockConfigEnv::with_device_failure();
        let result = get_local_device(&env, "phys-abc".into()).await;
        assert!(result.is_err());
    }

    // --- Tests: get_or_create_local_device ---

    #[tokio::test]
    async fn test_get_or_create_local_device_success() {
        let env = MockConfigEnv::new();
        let req = GetOrCreateLocalDeviceRequest {
            physical_device_id: "phys-new".into(),
            display_name: Some("New Device".into()),
            platform: "ios".into(),
        };
        let result = get_or_create_local_device(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.was_created);
        assert_eq!(resp.platform, "ios");
    }

    #[tokio::test]
    async fn test_get_or_create_local_device_failure() {
        let env = MockConfigEnv::with_device_failure();
        let req = GetOrCreateLocalDeviceRequest {
            physical_device_id: "phys-new".into(),
            display_name: None,
            platform: "ios".into(),
        };
        let result = get_or_create_local_device(&env, req).await;
        assert!(result.is_err());
    }

    // --- Tests: list_remote_storages ---

    #[tokio::test]
    async fn test_list_remote_storages_success() {
        let env = MockConfigEnv::new();
        let result = list_remote_storages(&env).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().list.len(), 1);
    }

    #[tokio::test]
    async fn test_list_remote_storages_failure() {
        let env = MockConfigEnv::with_storage_failure();
        let result = list_remote_storages(&env).await;
        assert!(result.is_err());
    }

    // --- Tests: list_backup_configs ---

    #[tokio::test]
    async fn test_list_backup_configs_success() {
        let env = MockConfigEnv::new();
        let result = list_backup_configs(&env, "phys-1".into()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().list.len(), 1);
    }

    #[tokio::test]
    async fn test_list_backup_configs_failure() {
        let env = MockConfigEnv::with_config_failure();
        let result = list_backup_configs(&env, "phys-1".into()).await;
        assert!(result.is_err());
    }

    // --- Tests: list_all_backup_configs ---

    #[tokio::test]
    async fn test_list_all_backup_configs_success() {
        let env = MockConfigEnv::new();
        let result = list_all_backup_configs(&env).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_all_backup_configs_failure() {
        let env = MockConfigEnv::with_config_failure();
        let result = list_all_backup_configs(&env).await;
        assert!(result.is_err());
    }

    // --- Tests: list_devices_by_platform ---

    #[tokio::test]
    async fn test_list_devices_by_platform_success() {
        let env = MockConfigEnv::new();
        let req = ListDevicesByPlatformRequest {
            platform: "android".into(),
        };
        let result = list_devices_by_platform(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.devices.len(), 1);
        assert_eq!(resp.devices[0].platform, "android");
    }

    #[tokio::test]
    async fn test_list_devices_by_platform_failure() {
        let env = MockConfigEnv::with_device_failure();
        let req = ListDevicesByPlatformRequest {
            platform: "android".into(),
        };
        let result = list_devices_by_platform(&env, req).await;
        assert!(result.is_err());
    }

    // --- Tests: list_all_devices ---

    #[tokio::test]
    async fn test_list_all_devices_success() {
        let env = MockConfigEnv::new();
        let result = list_all_devices(&env).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_all_devices_failure() {
        let env = MockConfigEnv::with_device_failure();
        let result = list_all_devices(&env).await;
        assert!(result.is_err());
    }

    // --- Tests: toggle_backup_config ---

    #[tokio::test]
    async fn test_toggle_backup_config_success() {
        let env = MockConfigEnv::new();
        let config_id = Uuid::now_v7();
        let req = ToggleBackupConfigRequest {
            id: config_id,
            is_active: false,
        };
        let result = toggle_backup_config(&env, req).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, config_id);
        assert!(!resp.is_active);
    }

    #[tokio::test]
    async fn test_toggle_backup_config_failure() {
        let env = MockConfigEnv::with_config_failure();
        let req = ToggleBackupConfigRequest {
            id: Uuid::now_v7(),
            is_active: true,
        };
        let result = toggle_backup_config(&env, req).await;
        assert!(result.is_err());
    }
}
