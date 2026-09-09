/// Rebuilds the local SQLite index from remote encrypted metadata.
///
/// Used when the local index is lost or corrupted. Fetches all file versions
/// from the server, decrypts the metadata locally, and populates the SQLite index.
use std::collections::HashMap;

use api_types::remote_file_version::{ListAllVersionsRequest, RemoteFileVersion};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    applications::backup::env::BackupEnv,
    domain::{
        derived_keys::DerivedKeys,
        metadata_crypto::{self, EncryptedMetadata},
    },
    model::{base::AppResult, local_index::LocalIndexEntry},
    ports::{
        api::remote_file_version_api_port::RemoteFileVersionApiPort, local_index::LocalIndexPort,
    },
};

/// Result of a local index recovery operation.
#[derive(Debug)]
pub struct RecoveryResult {
    /// Number of unique files recovered into the index.
    pub files_recovered: usize,
    /// Total number of versions fetched from the server.
    pub total_versions: usize,
}

/// Rebuilds the local SQLite index by fetching all encrypted file versions
/// from the server and decrypting the metadata locally.
///
/// For each unique file (identified by `name_blind_index`), the entry with the
/// highest version number is used to populate the index.
pub async fn rebuild_local_index<E: BackupEnv>(
    env: &E,
    config_id: Uuid,
    derived_keys: &DerivedKeys,
) -> AppResult<RecoveryResult> {
    info!(config_id = %config_id, "Starting local index recovery from server");

    // Clear existing entries for this config
    env.local_index().clear_all(config_id).await?;

    // Fetch all versions from the server
    let response = env
        .remote_file_version_api()
        .list_all_versions(ListAllVersionsRequest {
            backup_config_id: config_id,
        })
        .await?;

    let total_versions = response.versions.len();
    debug!(total_versions, "Fetched versions from server");

    if total_versions == 0 {
        info!("No remote versions found, recovery complete with empty index");
        return Ok(RecoveryResult {
            files_recovered: 0,
            total_versions: 0,
        });
    }

    // Group by blind_index and keep only the latest version per file
    let mut latest_by_file: HashMap<Vec<u8>, &RemoteFileVersion> = HashMap::new();
    for version in &response.versions {
        let entry = latest_by_file
            .entry(version.name_blind_index.clone())
            .or_insert(version);
        if version.version > entry.version {
            *entry = version;
        }
    }

    // Decrypt and upsert each file into the local index
    let mut files_recovered = 0;
    for (_, version) in &latest_by_file {
        let encrypted = EncryptedMetadata {
            encrypted_name: version.encrypted_name.clone(),
            nonce: version.name_nonce.clone(),
            blind_index: version.name_blind_index.clone(),
        };

        let path = metadata_crypto::decrypt_file_path(&encrypted, &derived_keys.metadata_key)?;

        let entry = LocalIndexEntry {
            backup_config_id: config_id,
            path,
            size: version.size,
            mtime: version.local_file_updated_at,
            content_hash: None,
            encrypted_name: version.encrypted_name.clone(),
            encrypted_name_nonce: version.name_nonce.clone(),
            blind_index: version.name_blind_index.clone(),
            remote_file_id: Some(version.id),
            last_backed_up_version: Some(version.version as i32),
            synced_at: Some(version.created_at),
        };

        env.local_index().upsert_file(&entry).await?;
        files_recovered += 1;
    }

    info!(
        files_recovered,
        total_versions, "Local index recovery complete"
    );

    Ok(RecoveryResult {
        files_recovered,
        total_versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::domain::dek::Dek;
    use crate::ports::api::{
        ApiResult, backup_config_api_port::BackupConfigApiPort,
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
            CreateFileVersionRequest, CreateFileVersionResponse, FileVersionStatus,
            ListAllVersionsRequest, ListAllVersionsResponse, ListBackedUpFilesRequest,
            ListBackedUpFilesResponse, ListBinVersionsRequest, ListBinVersionsResponse,
            MoveAllVersionsToBinRequest, MoveVersionToBinRequest, RestoreVersionFromBinRequest,
            UpdateFileVersionStatusRequest,
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

    fn make_remote_version(path: &str, version: u32, keys: &DerivedKeys) -> RemoteFileVersion {
        let encrypted =
            metadata_crypto::encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        RemoteFileVersion {
            id: Uuid::now_v7(),
            device_id: Uuid::now_v7(),
            encrypted_name: encrypted.encrypted_name,
            name_nonce: encrypted.nonce,
            name_blind_index: encrypted.blind_index,
            version,
            size: 1024,
            status: FileVersionStatus::UploadCompleted,
            local_file_updated_at: Utc::now(),
            created_at: Utc::now(),
        }
    }

    // --- Stub port implementations ---

    struct MockRemoteFileVersionApi {
        versions: Vec<RemoteFileVersion>,
    }

    #[async_trait]
    impl RemoteFileVersionApiPort for MockRemoteFileVersionApi {
        async fn create(
            &self,
            _: CreateFileVersionRequest,
        ) -> ApiResult<CreateFileVersionResponse> {
            unimplemented!()
        }
        async fn update_status(&self, _: UpdateFileVersionStatusRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_backed_up_files(
            &self,
            _: ListBackedUpFilesRequest,
        ) -> ApiResult<ListBackedUpFilesResponse> {
            unimplemented!()
        }
        async fn list_all_versions(
            &self,
            _: ListAllVersionsRequest,
        ) -> ApiResult<ListAllVersionsResponse> {
            Ok(ListAllVersionsResponse {
                versions: self.versions.clone(),
            })
        }
        async fn move_to_bin(&self, _: MoveVersionToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn move_all_to_bin(&self, _: MoveAllVersionsToBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn restore_from_bin(&self, _: RestoreVersionFromBinRequest) -> ApiResult<()> {
            unimplemented!()
        }
        async fn list_bin_versions(
            &self,
            _: ListBinVersionsRequest,
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

    struct MockRecoveryEnv {
        file_version_api: MockRemoteFileVersionApi,
        local_index: SqliteLocalIndex,
    }

    impl BackupEnv for MockRecoveryEnv {
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

    /// Recovery with no remote versions produces an empty index.
    #[tokio::test]
    async fn test_recovery_empty_remote() {
        let keys = test_derived_keys();
        let config_id = Uuid::now_v7();
        let env = MockRecoveryEnv {
            file_version_api: MockRemoteFileVersionApi { versions: vec![] },
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        };

        let result = rebuild_local_index(&env, config_id, &keys).await.unwrap();
        assert_eq!(result.files_recovered, 0);
        assert_eq!(result.total_versions, 0);

        let all = env.local_index.list_all(config_id).await.unwrap();
        assert!(all.is_empty());
    }

    /// Recovery with N files produces N SQLite entries with correct decrypted paths.
    #[tokio::test]
    async fn test_recovery_populates_index() {
        let keys = test_derived_keys();
        let config_id = Uuid::now_v7();
        let env = MockRecoveryEnv {
            file_version_api: MockRemoteFileVersionApi {
                versions: vec![
                    make_remote_version("/docs/report.pdf", 1, &keys),
                    make_remote_version("/docs/notes.md", 1, &keys),
                    make_remote_version("/photos/cat.jpg", 1, &keys),
                ],
            },
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        };

        let result = rebuild_local_index(&env, config_id, &keys).await.unwrap();
        assert_eq!(result.files_recovered, 3);
        assert_eq!(result.total_versions, 3);

        let all = env.local_index.list_all(config_id).await.unwrap();
        assert_eq!(all.len(), 3);

        let paths: Vec<&str> = all.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"/docs/report.pdf"));
        assert!(paths.contains(&"/docs/notes.md"));
        assert!(paths.contains(&"/photos/cat.jpg"));
    }

    /// Recovery with multiple versions per file keeps the latest version.
    #[tokio::test]
    async fn test_recovery_keeps_latest_version() {
        let keys = test_derived_keys();
        let config_id = Uuid::now_v7();

        // Same path, two versions — use same blind_index
        let v1 = make_remote_version("/docs/report.pdf", 1, &keys);
        let mut v2 = make_remote_version("/docs/report.pdf", 2, &keys);
        // Ensure v2 has the same blind_index as v1 (same path + same key = same index)
        v2.name_blind_index = v1.name_blind_index.clone();

        let env = MockRecoveryEnv {
            file_version_api: MockRemoteFileVersionApi {
                versions: vec![v1, v2],
            },
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        };

        let result = rebuild_local_index(&env, config_id, &keys).await.unwrap();
        assert_eq!(result.files_recovered, 1);
        assert_eq!(result.total_versions, 2);

        let entry = env
            .local_index
            .get_by_path(config_id, "/docs/report.pdf")
            .await
            .unwrap()
            .expect("Expected entry for report.pdf");
        assert_eq!(entry.last_backed_up_version, Some(2));
    }
}
