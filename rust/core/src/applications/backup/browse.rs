use api_types::remote_file_version::{
    ListBackedUpFilesRequest, ListBackedUpFilesResponse, ListBinVersionsRequest,
    ListBinVersionsResponse,
};
use tracing::debug;
use uuid::Uuid;

use crate::{
    applications::backup::env::BackupEnv,
    domain::{derived_keys::DerivedKeys, metadata_crypto},
    model::base::AppResult,
    ports::api::remote_file_version_api_port::RemoteFileVersionApiPort,
};

/// Lists backed-up files for a given backup config with cursor-based pagination.
///
/// Fetches encrypted file metadata from the server, then decrypts
/// each file's name using the provided `DerivedKeys.metadata_key`.
pub async fn list_backed_up_files<E: BackupEnv>(
    env: &E,
    backup_config_id: Uuid,
    cursor: Option<Vec<u8>>,
    limit: i64,
    derived_keys: &DerivedKeys,
) -> AppResult<ListBackedUpFilesResponse> {
    let request = ListBackedUpFilesRequest {
        backup_config_id,
        cursor,
        limit,
    };
    let mut response = env
        .remote_file_version_api()
        .list_backed_up_files(request)
        .await?;

    // Decrypt file paths from encrypted metadata
    for file in &mut response.files {
        let encrypted = metadata_crypto::EncryptedMetadata {
            encrypted_name: file.encrypted_name.clone(),
            nonce: file.name_nonce.clone(),
            blind_index: file.blind_index.clone(),
        };
        file.path = metadata_crypto::decrypt_file_path(&encrypted, &derived_keys.metadata_key)?;
    }

    debug!(
        config_id = %backup_config_id,
        files = response.files.len(),
        has_more = response.has_more,
        "Listed backed up files (page)"
    );
    Ok(response)
}

/// Lists all file versions currently in the bin for a given backup config.
///
/// Fetches encrypted file metadata from the server, then decrypts
/// each file's name using the provided `DerivedKeys.metadata_key`.
pub async fn list_bin_versions<E: BackupEnv>(
    env: &E,
    backup_config_id: Uuid,
    derived_keys: &DerivedKeys,
) -> AppResult<ListBinVersionsResponse> {
    let request = ListBinVersionsRequest { backup_config_id };
    let mut response = env
        .remote_file_version_api()
        .list_bin_versions(request)
        .await?;

    // Decrypt file paths from encrypted metadata
    for file in &mut response.files {
        let encrypted = metadata_crypto::EncryptedMetadata {
            encrypted_name: file.encrypted_name.clone(),
            nonce: file.name_nonce.clone(),
            blind_index: file.blind_index.clone(),
        };
        file.path = metadata_crypto::decrypt_file_path(&encrypted, &derived_keys.metadata_key)?;
    }

    debug!(config_id = %backup_config_id, files = response.files.len(), "Listed bin versions");
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::domain::dek::Dek;
    use crate::ports::api::{
        ApiClientError, ApiResult, backup_config_api_port::BackupConfigApiPort,
        backup_job_api_port::BackupJobApiPort, chunk_api_port::ChunkApiPort,
        local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    };
    use api_types::{
        auth::{LoginRequest, LoginResponse, Tokens},
        backup_config::{
            CreateBackupConfigRequest, CreateBackupConfigResponse, GetBackupConfigRequest,
            GetBackupConfigResponse, ListAllBackupConfigsResponse,
            ListBackupConfigWithRemoteStorageRequest, ListBackupConfigWithRemoteStorageResponse,
            RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
            ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
            UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
        },
        backup_job::{
            AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
            CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
            CreateBackupJobResponse, GetBackupJobDetailRequest, GetBackupJobDetailResponse,
            GetLatestBackupJobResponse, GetResumableBackupJobRequest,
            GetResumableBackupJobResponse, ListBackupJobsRequest, ListBackupJobsResponse,
            LogCleanupFilesRequest,
        },
        chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
        email_verification::{
            ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
            VerifyEmailResponse,
        },
        local_device::{
            CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
            GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
            GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
            ListDevicesByPlatformResponse,
        },
        remote_file_version::{
            BackedUpFile, CreateFileVersionRequest, CreateFileVersionResponse, FileVersionStatus,
            FileVersionSummary, ListAllVersionsRequest, ListAllVersionsResponse,
            ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
            MoveVersionToBinRequest, RestoreVersionFromBinRequest, UpdateFileVersionStatusRequest,
        },
        remote_storage::{
            CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageRequest,
            GetRemoteStorageResponse, ListRemoteStoragesResponse, ReauthRemoteStorageRequest,
            ReauthRemoteStorageResponse, UpdateRemoteStorageStatusRequest,
            UpdateRemoteStorageStatusResponse,
        },
        restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
        user::{AdminUserListResponse, UserCreateRequest, UserCreateResponse, UserInfo},
    };
    use async_trait::async_trait;
    use chrono::Utc;

    fn test_derived_keys() -> DerivedKeys {
        let dek = Dek::generate().unwrap();
        DerivedKeys::derive(&dek).unwrap()
    }

    fn make_encrypted_file(path: &str, keys: &DerivedKeys) -> BackedUpFile {
        let encrypted =
            metadata_crypto::encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        BackedUpFile {
            file_id: Uuid::now_v7(),
            encrypted_name: encrypted.encrypted_name,
            name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            path: String::new(),
            size: 1024,
            file_updated_at: Utc::now(),
            versions: vec![],
        }
    }

    // --- Stub port implementations ---
    // Only RemoteFileVersionApi needs real behavior; the rest are unreachable stubs.

    struct MockRemoteFileVersionApi {
        should_fail: bool,
        files: Vec<BackedUpFile>,
    }

    #[async_trait]
    impl RemoteFileVersionApiPort for MockRemoteFileVersionApi {
        async fn create(
            &self,
            _req: CreateFileVersionRequest,
        ) -> ApiResult<CreateFileVersionResponse> {
            unimplemented!()
        }
        async fn update_status(&self, _req: UpdateFileVersionStatusRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_backed_up_files(
            &self,
            _req: ListBackedUpFilesRequest,
        ) -> ApiResult<ListBackedUpFilesResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl(
                    "list backed up files failed".into(),
                ));
            }
            Ok(ListBackedUpFilesResponse {
                files: self.files.clone(),
                has_more: false,
                next_cursor: None,
                total_files: Some(self.files.len() as i64),
            })
        }
        async fn list_all_versions(
            &self,
            _req: ListAllVersionsRequest,
        ) -> ApiResult<ListAllVersionsResponse> {
            unimplemented!()
        }
        async fn move_to_bin(&self, _req: MoveVersionToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn move_all_to_bin(&self, _req: MoveAllVersionsToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn restore_from_bin(&self, _req: RestoreVersionFromBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_bin_versions(
            &self,
            _req: ListBinVersionsRequest,
        ) -> ApiResult<ListBinVersionsResponse> {
            unimplemented!()
        }
    }

    struct StubUserApi;
    #[async_trait]
    impl UserApiPort for StubUserApi {
        async fn create_user(&self, _: UserCreateRequest) -> ApiResult<UserCreateResponse> {
            unimplemented!()
        }
        async fn login(&self, _: LoginRequest) -> ApiResult<LoginResponse> {
            unimplemented!()
        }
        async fn get_me(&self, _: &str) -> ApiResult<UserInfo> {
            unimplemented!()
        }
        async fn list_all_users(
            &self,
            _: &str,
            _: u32,
            _: u32,
        ) -> ApiResult<AdminUserListResponse> {
            unimplemented!()
        }
        async fn verify_email(&self, _: VerifyEmailRequest) -> ApiResult<VerifyEmailResponse> {
            unimplemented!()
        }
        async fn resend_verification(
            &self,
            _: ResendVerificationRequest,
        ) -> ApiResult<ResendVerificationResponse> {
            unimplemented!()
        }

        async fn refresh(&self, _: &str) -> ApiResult<Tokens> {
            unimplemented!()
        }
    }

    struct StubBackupConfigApi;
    #[async_trait]
    impl BackupConfigApiPort for StubBackupConfigApi {
        async fn create(
            &self,
            _: CreateBackupConfigRequest,
        ) -> ApiResult<CreateBackupConfigResponse> {
            unimplemented!()
        }
        async fn list_with_remote_storage(
            &self,
            _: ListBackupConfigWithRemoteStorageRequest,
        ) -> ApiResult<ListBackupConfigWithRemoteStorageResponse> {
            unimplemented!()
        }
        async fn get_by_id(&self, _: GetBackupConfigRequest) -> ApiResult<GetBackupConfigResponse> {
            unimplemented!()
        }
        async fn list_all(&self) -> ApiResult<ListAllBackupConfigsResponse> {
            unimplemented!()
        }
        async fn toggle_active(
            &self,
            _: ToggleBackupConfigRequest,
        ) -> ApiResult<ToggleBackupConfigResponse> {
            unimplemented!()
        }
        async fn rename(
            &self,
            _: RenameBackupConfigRequest,
        ) -> ApiResult<RenameBackupConfigResponse> {
            unimplemented!()
        }
        async fn update_cleanup_type(
            &self,
            _: UpdateCleanupTypeRequest,
        ) -> ApiResult<UpdateCleanupTypeResponse> {
            unimplemented!()
        }
        async fn update_exclusion_config(
            &self,
            _: UpdateExclusionConfigRequest,
        ) -> ApiResult<UpdateExclusionConfigResponse> {
            unimplemented!()
        }
    }

    struct StubLocalDeviceApi;
    #[async_trait]
    impl LocalDeviceApiPort for StubLocalDeviceApi {
        async fn create(
            &self,
            _: CreateLocalDeviceRequest,
        ) -> ApiResult<CreateLocalDeviceResponse> {
            unimplemented!()
        }
        async fn get_by_physical_id(
            &self,
            _: GetLocalDeviceByPhysicalIdRequest,
        ) -> ApiResult<GetLocalDeviceByPhysicalIdResponse> {
            unimplemented!()
        }
        async fn get_or_create(
            &self,
            _: GetOrCreateLocalDeviceRequest,
        ) -> ApiResult<GetOrCreateLocalDeviceResponse> {
            unimplemented!()
        }
        async fn list_by_platform(
            &self,
            _: ListDevicesByPlatformRequest,
        ) -> ApiResult<ListDevicesByPlatformResponse> {
            unimplemented!()
        }
        async fn list_all(&self) -> ApiResult<ListAllDevicesResponse> {
            unimplemented!()
        }
    }

    struct StubRemoteStorageApi;
    #[async_trait]
    impl RemoteStorageApiPort for StubRemoteStorageApi {
        async fn create(
            &self,
            _: CreateRemoteStorageRequest,
        ) -> ApiResult<CreateRemoteStorageResponse> {
            unimplemented!()
        }
        async fn get_by_id(
            &self,
            _: GetRemoteStorageRequest,
        ) -> ApiResult<GetRemoteStorageResponse> {
            unimplemented!()
        }
        async fn list(&self) -> ApiResult<ListRemoteStoragesResponse> {
            unimplemented!()
        }
        async fn update_status(
            &self,
            _: UpdateRemoteStorageStatusRequest,
        ) -> ApiResult<UpdateRemoteStorageStatusResponse> {
            unimplemented!()
        }
        async fn reauth(
            &self,
            _: ReauthRemoteStorageRequest,
        ) -> ApiResult<ReauthRemoteStorageResponse> {
            unimplemented!()
        }
    }

    struct StubChunkApi;
    #[async_trait]
    impl ChunkApiPort for StubChunkApi {
        async fn create(&self, _: CreateChunkRequest) -> ApiResult<CreateChunkResponse> {
            unimplemented!()
        }
        async fn get_chunks_for_version(
            &self,
            _: GetFileVersionChunksRequest,
        ) -> ApiResult<GetFileVersionChunksResponse> {
            unimplemented!()
        }
        async fn update_storage_meta(&self, _: UpdateChunkStorageMetaRequest) -> ApiResult<()> {
            unimplemented!()
        }
    }

    struct StubBackupJobApi;
    #[async_trait]
    impl BackupJobApiPort for StubBackupJobApi {
        async fn create_job(
            &self,
            _: CreateBackupJobRequest,
        ) -> ApiResult<CreateBackupJobResponse> {
            unimplemented!()
        }
        async fn create_job_file(
            &self,
            _: CreateBackupJobFileRequest,
        ) -> ApiResult<CreateBackupJobFileResponse> {
            unimplemented!()
        }
        async fn complete_job_file(&self, _: CompleteBackupJobFileRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn complete_job(&self, _: CompleteBackupJobRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_jobs(&self, _: ListBackupJobsRequest) -> ApiResult<ListBackupJobsResponse> {
            unimplemented!()
        }
        async fn get_job_detail(
            &self,
            _: GetBackupJobDetailRequest,
        ) -> ApiResult<GetBackupJobDetailResponse> {
            unimplemented!()
        }
        async fn get_latest_job(&self) -> ApiResult<GetLatestBackupJobResponse> {
            unimplemented!()
        }
        async fn get_resumable_job(
            &self,
            _: GetResumableBackupJobRequest,
        ) -> ApiResult<GetResumableBackupJobResponse> {
            unimplemented!()
        }
        async fn log_cleanup_files(&self, _: LogCleanupFilesRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse> {
            unimplemented!()
        }
    }

    // --- Mock environment ---

    struct MockBrowseEnv {
        file_version_api: MockRemoteFileVersionApi,
        local_index: SqliteLocalIndex,
    }

    impl BackupEnv for MockBrowseEnv {
        type UserApi = StubUserApi;
        type BackupConfigApi = StubBackupConfigApi;
        type LocalIndex = SqliteLocalIndex;
        type LocalDeviceApi = StubLocalDeviceApi;
        type RemoteStorageApi = StubRemoteStorageApi;
        type RemoteFileVersionApi = MockRemoteFileVersionApi;
        type ChunkApi = StubChunkApi;
        type BackupJobApi = StubBackupJobApi;

        fn clone_env(&self) -> Self {
            unimplemented!()
        }
        fn user_api(&self) -> &Self::UserApi {
            unimplemented!()
        }
        fn backup_config_api(&self) -> &Self::BackupConfigApi {
            unimplemented!()
        }
        fn local_index(&self) -> &Self::LocalIndex {
            &self.local_index
        }
        fn local_device_api(&self) -> &Self::LocalDeviceApi {
            unimplemented!()
        }
        fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
            unimplemented!()
        }
        fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi {
            &self.file_version_api
        }
        fn chunk_api(&self) -> &Self::ChunkApi {
            unimplemented!()
        }
        fn backup_job_api(&self) -> &Self::BackupJobApi {
            unimplemented!()
        }
    }

    async fn make_browse_env(should_fail: bool, keys: &DerivedKeys) -> MockBrowseEnv {
        let mut file1 = make_encrypted_file("/docs/report.pdf", keys);
        file1.size = 1_048_576;
        file1.versions = vec![FileVersionSummary {
            version_id: Uuid::now_v7(),
            version: 1,
            size: 1_048_576,
            status: FileVersionStatus::UploadCompleted,
            created_at: Utc::now(),
        }];

        let mut file2 = make_encrypted_file("/docs/notes.txt", keys);
        file2.size = 512;

        MockBrowseEnv {
            file_version_api: MockRemoteFileVersionApi {
                should_fail,
                files: vec![file1, file2],
            },
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        }
    }

    /// Tests that list_backed_up_files decrypts file paths on success.
    #[tokio::test]
    async fn test_list_backed_up_files_success() {
        let keys = test_derived_keys();
        let env = make_browse_env(false, &keys).await;
        let config_id = Uuid::now_v7();
        let result = list_backed_up_files(&env, config_id, None, 50, &keys).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.files.len(), 2);
        assert_eq!(response.files[0].path, "/docs/report.pdf");
        assert_eq!(response.files[1].path, "/docs/notes.txt");
    }

    /// Tests that list_backed_up_files propagates API errors.
    #[tokio::test]
    async fn test_list_backed_up_files_failure() {
        let keys = test_derived_keys();
        let env = make_browse_env(true, &keys).await;
        let config_id = Uuid::now_v7();
        let result = list_backed_up_files(&env, config_id, None, 50, &keys).await;
        assert!(result.is_err());
    }

    /// Tests that the returned files contain expected version information.
    #[tokio::test]
    async fn test_list_backed_up_files_contains_versions() {
        let keys = test_derived_keys();
        let env = make_browse_env(false, &keys).await;
        let config_id = Uuid::now_v7();
        let response = list_backed_up_files(&env, config_id, None, 50, &keys)
            .await
            .unwrap();
        // First file has one version
        assert_eq!(response.files[0].versions.len(), 1);
        assert_eq!(response.files[0].versions[0].version, 1);
        assert_eq!(
            response.files[0].versions[0].status,
            FileVersionStatus::UploadCompleted
        );
        // Second file has no versions
        assert!(response.files[1].versions.is_empty());
    }
}
