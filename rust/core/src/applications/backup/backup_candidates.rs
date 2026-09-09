/// Client-side backup candidate detection using the local SQLite index.
///
/// Compares the local index against the filesystem scan results to find files
/// that need backing up (new files or files modified since last backup).
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    applications::backup::env::BackupEnv,
    model::{backup_candidate::ClientBackupCandidate, base::AppResult},
    ports::local_index::LocalIndexPort,
};

/// Queries the local SQLite index to find files that need backing up.
///
/// A file is a backup candidate if:
/// - It has no `last_backed_up_version` (never backed up), or
/// - Its `synced_at` is None (newly scanned, not yet confirmed backed up)
pub async fn find_backup_candidates<E: BackupEnv>(
    env: &E,
    source_directory: &str,
    config_id: Uuid,
) -> AppResult<Vec<ClientBackupCandidate>> {
    debug!(source = %source_directory, "Finding backup candidates from local index");

    let entries = env
        .local_index()
        .list_by_prefix(config_id, source_directory)
        .await?;

    let candidates: Vec<ClientBackupCandidate> = entries
        .into_iter()
        .filter(|entry| {
            // Candidate if never backed up or if mtime/size changed since last backup
            entry.last_backed_up_version.is_none() || entry.synced_at.is_none()
        })
        .map(|entry| {
            let base_version = entry.last_backed_up_version.map(|v| v as u32);
            ClientBackupCandidate {
                path: entry.path,
                size: entry.size,
                mtime: entry.mtime,
                base_version,
                encrypted_name: entry.encrypted_name,
                encrypted_name_nonce: entry.encrypted_name_nonce,
                blind_index: entry.blind_index,
                remote_file_id: entry.remote_file_id,
            }
        })
        .collect();

    info!(source = %source_directory, candidates = candidates.len(), "Backup candidates found");
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::domain::dek::Dek;
    use crate::domain::derived_keys::DerivedKeys;
    use crate::domain::metadata_crypto::encrypt_file_path;
    use crate::model::local_index::LocalIndexEntry;
    use crate::ports::api::{
        ApiResult, backup_config_api_port::BackupConfigApiPort,
        backup_job_api_port::BackupJobApiPort, chunk_api_port::ChunkApiPort,
        local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    };
    use api_types::{
        auth::*,
        backup_config::*,
        backup_job::*,
        chunk::*,
        email_verification::{
            ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
            VerifyEmailResponse,
        },
        local_device::*,
        remote_file_version::*,
        remote_storage::*,
        restore_file_info::*,
        user::*,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn test_keys() -> DerivedKeys {
        DerivedKeys::derive(&Dek {
            key: Zeroizing::new([42u8; 32]),
        })
        .unwrap()
    }

    fn test_config_id() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_entry(path: &str, keys: &DerivedKeys, backed_up: bool) -> LocalIndexEntry {
        let encrypted = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        LocalIndexEntry {
            backup_config_id: test_config_id(),
            path: path.to_string(),
            size: 1024,
            mtime: Utc::now(),
            content_hash: None,
            encrypted_name: encrypted.encrypted_name,
            encrypted_name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            remote_file_id: if backed_up {
                Some(Uuid::now_v7())
            } else {
                None
            },
            last_backed_up_version: if backed_up { Some(1) } else { None },
            synced_at: if backed_up { Some(Utc::now()) } else { None },
        }
    }

    // Minimal stubs for BackupEnv
    #[derive(Clone)]
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

    #[derive(Clone)]
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

    #[derive(Clone)]
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

    #[derive(Clone)]
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

    #[derive(Clone)]
    struct StubRemoteFileVersionApi;
    #[async_trait]
    impl RemoteFileVersionApiPort for StubRemoteFileVersionApi {
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
            unimplemented!()
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

    #[derive(Clone)]
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

    #[derive(Clone)]
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

    #[derive(Clone)]
    struct TestEnv {
        local_index: SqliteLocalIndex,
    }

    impl BackupEnv for TestEnv {
        type UserApi = StubUserApi;
        type BackupConfigApi = StubBackupConfigApi;
        type LocalIndex = SqliteLocalIndex;
        type LocalDeviceApi = StubLocalDeviceApi;
        type RemoteStorageApi = StubRemoteStorageApi;
        type RemoteFileVersionApi = StubRemoteFileVersionApi;
        type ChunkApi = StubChunkApi;
        type BackupJobApi = StubBackupJobApi;

        fn clone_env(&self) -> Self {
            self.clone()
        }
        fn user_api(&self) -> &Self::UserApi {
            &StubUserApi
        }
        fn backup_config_api(&self) -> &Self::BackupConfigApi {
            &StubBackupConfigApi
        }
        fn local_index(&self) -> &Self::LocalIndex {
            &self.local_index
        }
        fn local_device_api(&self) -> &Self::LocalDeviceApi {
            &StubLocalDeviceApi
        }
        fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
            &StubRemoteStorageApi
        }
        fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi {
            &StubRemoteFileVersionApi
        }
        fn chunk_api(&self) -> &Self::ChunkApi {
            &StubChunkApi
        }
        fn backup_job_api(&self) -> &Self::BackupJobApi {
            &StubBackupJobApi
        }
    }

    #[tokio::test]
    async fn test_no_candidates_when_all_backed_up() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();

        index
            .upsert_file(&make_entry("/dir/a.txt", &keys, true))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir/b.txt", &keys, true))
            .await
            .unwrap();

        let env = TestEnv { local_index: index };
        let candidates = find_backup_candidates(&env, "/dir/", test_config_id())
            .await
            .unwrap();
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn test_new_files_are_candidates() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();

        index
            .upsert_file(&make_entry("/dir/new.txt", &keys, false))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir/backed.txt", &keys, true))
            .await
            .unwrap();

        let env = TestEnv { local_index: index };
        let candidates = find_backup_candidates(&env, "/dir/", test_config_id())
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "/dir/new.txt");
        assert_eq!(candidates[0].base_version, None);
    }

    #[tokio::test]
    async fn test_modified_files_are_candidates() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();

        // File was backed up but then re-scanned (synced_at cleared)
        let mut entry = make_entry("/dir/modified.txt", &keys, true);
        entry.synced_at = None; // cleared during re-scan when mtime changed
        index.upsert_file(&entry).await.unwrap();

        let env = TestEnv { local_index: index };
        let candidates = find_backup_candidates(&env, "/dir/", test_config_id())
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "/dir/modified.txt");
        assert_eq!(candidates[0].base_version, Some(1)); // version 1 was backed up; server assigns the next
    }

    #[tokio::test]
    async fn test_prefix_filtering() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();

        index
            .upsert_file(&make_entry("/dir1/new.txt", &keys, false))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir2/new.txt", &keys, false))
            .await
            .unwrap();

        let env = TestEnv { local_index: index };
        let candidates = find_backup_candidates(&env, "/dir1/", test_config_id())
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "/dir1/new.txt");
    }
}
