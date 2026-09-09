use api_types::{
    backup_config::GetBackupConfigRequest,
    backup_job::{CompleteBackupJobFileRequest, CompleteBackupJobRequest, CreateBackupJobRequest},
    remote_storage::{
        GetRemoteStorageRequest, RemoteStorageConfig, RemoteStorageStatus,
        UpdateRemoteStorageStatusRequest,
    },
};
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    adapters::{
        aes_gcm_encryptor::AesGcmEncryptor,
        google_drive_storage_adaptor::GoogleDriveStorageAdaptor,
        onedrive_storage_adaptor::OneDriveStorageAdaptor, s3_storage_adaptor::S3StorageAdaptor,
        sftp_storage_adaptor::SftpStorageAdaptor,
    },
    applications::backup::{
        backup_candidates::find_backup_candidates,
        backup_config::{decrypt_backup_config, encrypt_create_job_file_request},
        backup_file::backup_file,
        cleanup::run_local_cleanup,
        env::BackupEnv,
        filesystem_scan::{ScanOptions, sync_filesystem_to_index},
    },
    domain::{dek::Dek, derived_keys::DerivedKeys},
    model::app_error::AppError,
    model::base::{AppResult, BackupFileEvent, EncryptedData},
    ports::local_index::LocalIndexPort,
    ports::{
        Encryptor, api::backup_config_api_port::BackupConfigApiPort,
        api::backup_job_api_port::BackupJobApiPort,
        api::remote_storage_api_port::RemoteStorageApiPort, storage::StoragePort,
    },
};

pub use api_types::backup::BackupResult;

/// Starts a backup job in a background task, returning a channel of progress events.
///
/// The caller receives `BackupResult` events as the backup progresses through
/// filesystem scanning, candidate detection, and per-file chunk upload.
pub fn start_backup<E: BackupEnv + Send + Sync + 'static>(
    env: E,
    dek: Dek,
    derived_keys: DerivedKeys,
    device_id: Uuid,
    config_id: Uuid,
) -> mpsc::Receiver<BackupResult> {
    let (tx, rx) = mpsc::channel(32);

    tokio::spawn(async move {
        if let Err(reason) = run_backup(&tx, &env, &dek, &derived_keys, device_id, config_id).await
        {
            error!(config_id = %config_id, error = %reason, "Backup failed");

            // If the token expired, mark the storage as AuthTokenExpired via the API
            // so the UI can prompt the user to reauth.
            if let AppError::AuthTokenExpired { storage_id } = &reason {
                let update_req = UpdateRemoteStorageStatusRequest {
                    id: *storage_id,
                    status: RemoteStorageStatus::AuthTokenExpired,
                };
                if let Err(e) = env.remote_storage_api().update_status(update_req).await {
                    error!(
                        storage_id = %storage_id,
                        error = %e,
                        "Failed to mark storage as AuthTokenExpired"
                    );
                }
            }

            if tx
                .send(BackupResult::Failed {
                    reason: reason.to_string(),
                    file_path: None,
                })
                .await
                .is_err()
            {
                error!(config_id = %config_id, "Backup result channel closed — UI may not receive failure notification");
            }
        }
    });

    rx
}

/// Outer backup orchestrator. Handles setup up to job creation, then delegates to
/// `run_backup_job`. If `run_backup_job` fails after the job was created, this
/// function marks the job as "failed" in the DB before propagating the error.
#[tracing::instrument(skip(tx, env, dek, derived_keys), fields(config_id = %config_id))]
async fn run_backup<E: BackupEnv + Send + Sync + 'static>(
    tx: &mpsc::Sender<BackupResult>,
    env: &E,
    dek: &Dek,
    derived_keys: &DerivedKeys,
    device_id: Uuid,
    config_id: Uuid,
) -> AppResult<()> {
    // Fetch the backup config by ID and decrypt source directory
    let request = GetBackupConfigRequest { id: config_id };
    let config_response = env.backup_config_api().get_by_id(request).await?;
    let cleanup_type = config_response.config.cleanup_type.clone();
    let backup_config = decrypt_backup_config(config_response.config, &derived_keys.metadata_key)?;

    // Fetch the remote storage configuration
    let storage_request = GetRemoteStorageRequest {
        id: backup_config.storage_id,
    };
    let storage_response = env.remote_storage_api().get_by_id(storage_request).await?;
    let remote_storage = storage_response.storage;

    // Decrypt the storage config using DEK
    let encrypted_data: EncryptedData = remote_storage.config.try_into()?;
    let encryptor = AesGcmEncryptor::new(dek)?;
    let decrypted_storage_config = encryptor.decrypt(&encrypted_data)?;
    let storage_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_storage_config)?;

    let _ = tx
        .send(BackupResult::ConfigLoaded {
            config_id,
            storage_id: backup_config.storage_id,
        })
        .await;

    // Sync filesystem state into the local SQLite index
    let scan_options = ScanOptions {
        follow_symlinks: true,
        exclusions: backup_config.exclusion_config.clone(),
    };
    let scan_result = sync_filesystem_to_index(
        env,
        &backup_config.source_directory,
        derived_keys,
        &scan_options,
        config_id,
    )
    .await?;

    let _ = tx
        .send(BackupResult::IndexUpdated {
            files_updated: scan_result.new_files + scan_result.modified_files,
            files_failed: scan_result.errors.len(),
        })
        .await;

    // Find backup candidates from the local index (files that need backing up)
    let candidates =
        find_backup_candidates(env, &backup_config.source_directory, config_id).await?;

    let total_bytes: u64 = candidates.iter().map(|c| c.size as u64).sum();
    let total_files = candidates.len();

    let _ = tx
        .send(BackupResult::CandidatesFound {
            total_files,
            total_bytes,
        })
        .await;

    if candidates.is_empty() {
        let _ = tx
            .send(BackupResult::Completed {
                total_files: 0,
                total_bytes: 0,
                uploaded_bytes: 0,
                deduplicated_bytes: 0,
            })
            .await;
        return Ok(());
    }

    let job_response = env
        .backup_job_api()
        .create_job(CreateBackupJobRequest {
            backup_config_id: config_id,
            started_at: Utc::now(),
        })
        .await?;
    let job_id = job_response.id;
    info!(job_id = %job_id, "Created new backup job");

    // Delegate to the file-loop runner. If it fails for any reason (e.g. OAuth
    // refresh error while creating a storage adaptor), mark the job as "failed"
    // in the DB so it does not stay stuck in "in_progress".
    if let Err(reason) = run_backup_job(
        tx,
        env,
        dek,
        derived_keys,
        device_id,
        config_id,
        job_id,
        backup_config.storage_id,
        backup_config.user_id,
        candidates,
        storage_config,
        cleanup_type,
    )
    .await
    {
        error!(job_id = %job_id, error = %reason, "Backup job failed — marking as failed in DB");
        if let Err(e) = env
            .backup_job_api()
            .complete_job(CompleteBackupJobRequest {
                id: job_id,
                status: "failed".to_string(),
                error_message: Some(reason.to_string()),
            })
            .await
        {
            error!(job_id = %job_id, error = %e, "Failed to mark backup job as failed in DB");
        }
        return Err(reason);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_backup_job<E: BackupEnv + Send + Sync + 'static>(
    tx: &mpsc::Sender<BackupResult>,
    env: &E,
    dek: &Dek,
    derived_keys: &DerivedKeys,
    device_id: Uuid,
    config_id: Uuid,
    job_id: Uuid,
    storage_id: Uuid,
    user_id: Uuid,
    candidates: Vec<crate::model::backup_candidate::ClientBackupCandidate>,
    storage_config: RemoteStorageConfig,
    cleanup_type: api_types::backup_config::CleanupType,
) -> AppResult<()> {
    // Backup each candidate file, skipping already completed ones
    let mut completed_files: usize = 0;
    let mut failed_files: usize = 0;
    let mut completed_bytes: u64 = 0;
    let mut total_uploaded_bytes: u64 = 0;
    let mut total_deduplicated_bytes: u64 = 0;

    for candidate in &candidates {
        let file_size = candidate.size as u64;

        let _ = tx
            .send(BackupResult::FileStarted {
                file_path: candidate.path.clone(),
                file_size,
                base_version: candidate.base_version,
            })
            .await;

        // Create backup job file entry (encrypted)
        let job_file_request =
            encrypt_create_job_file_request(job_id, &candidate.path, candidate.size, derived_keys)?;
        let job_file_response = env
            .backup_job_api()
            .create_job_file(job_file_request)
            .await?;
        let job_file_id = job_file_response.id;

        // Build the storage adaptor. If this fails (e.g. OAuth token refresh 500),
        // mark the job file as failed before propagating so it doesn't stay in 'uploading'.
        let storage_result: AppResult<Box<dyn StoragePort>> = match &storage_config {
            RemoteStorageConfig::Aws(s3_creds) => S3StorageAdaptor::new(s3_creds.clone())
                .await
                .map(|a| Box::new(a) as Box<dyn StoragePort>),
            RemoteStorageConfig::GoogleDrive(gdrive_creds) => {
                GoogleDriveStorageAdaptor::new(gdrive_creds.clone(), storage_id)
                    .await
                    .map(|a| Box::new(a) as Box<dyn StoragePort>)
            }
            RemoteStorageConfig::OneDrive(onedrive_creds) => {
                OneDriveStorageAdaptor::new(onedrive_creds.clone(), storage_id)
                    .await
                    .map(|a| Box::new(a) as Box<dyn StoragePort>)
            }
            RemoteStorageConfig::LocalFilesystem(cfg) => {
                crate::adapters::local_fs_storage_adaptor::LocalFsStorageAdaptor::new(
                    &cfg.root_path,
                )
                .map(|a| Box::new(a) as Box<dyn StoragePort>)
            }
            RemoteStorageConfig::Sftp(sftp_creds) => SftpStorageAdaptor::new(sftp_creds.clone())
                .map(|a| Box::new(a) as Box<dyn StoragePort>),
        };
        let storage = match storage_result {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Storage adaptor creation failed — marking job file as failed");
                let _ = env
                    .backup_job_api()
                    .complete_job_file(CompleteBackupJobFileRequest {
                        id: job_file_id,
                        uploaded_size: 0,
                        deduplicated_size: 0,
                        total_chunks: 0,
                        deduplicated_chunks: 0,
                        status: "failed".to_string(),
                        error_message: Some(e.to_string()),
                    })
                    .await;
                return Err(e);
            }
        };

        let mut chunk_rx = backup_file(
            env.clone_env(),
            storage,
            candidate.clone(),
            config_id,
            device_id,
            storage_id,
            user_id,
            dek.clone(),
        );

        let mut bytes_uploaded: u64 = 0;
        let mut total_chunks: u32 = 0;
        let mut file_uploaded_bytes: u64 = 0;
        let mut file_deduplicated_bytes: u64 = 0;
        let mut file_deduplicated_chunks: i32 = 0;
        let mut file_failed = false;
        // Captured from the leading `VersionCreated` event so the local index can
        // record the real server-assigned file version id/number below, instead of
        // a placeholder.
        let mut version_info: Option<(Uuid, u32)> = None;

        while let Some(result) = chunk_rx.recv().await {
            match result {
                Ok(BackupFileEvent::VersionCreated {
                    file_version_id,
                    version,
                }) => {
                    version_info = Some((file_version_id, version));
                }
                Ok(BackupFileEvent::Chunk(chunk_meta)) => {
                    bytes_uploaded += chunk_meta.size as u64;
                    total_chunks += 1;
                    file_uploaded_bytes += chunk_meta.uploaded_size as u64;
                    if chunk_meta.deduplicated {
                        file_deduplicated_bytes += chunk_meta.size as u64;
                        file_deduplicated_chunks += 1;
                    }

                    let _ = tx
                        .send(BackupResult::ChunkUploaded {
                            file_path: candidate.path.clone(),
                            chunk_index: chunk_meta.index,
                            chunk_size: chunk_meta.size,
                            uploaded_size: chunk_meta.uploaded_size,
                            deduplicated: chunk_meta.deduplicated,
                            bytes_uploaded,
                            file_size,
                        })
                        .await;
                }
                Err(e) => {
                    error!(error = %e, "File backup failed");

                    // Complete the job file as failed
                    if let Err(api_err) = env
                        .backup_job_api()
                        .complete_job_file(CompleteBackupJobFileRequest {
                            id: job_file_id,
                            uploaded_size: file_uploaded_bytes as i64,
                            deduplicated_size: file_deduplicated_bytes as i64,
                            total_chunks: total_chunks as i32,
                            deduplicated_chunks: file_deduplicated_chunks,
                            status: "failed".to_string(),
                            error_message: Some(e.to_string()),
                        })
                        .await
                    {
                        error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark job file as failed");
                    }

                    let _ = tx
                        .send(BackupResult::FileFailed {
                            file_path: candidate.path.clone(),
                            reason: e.to_string(),
                        })
                        .await;

                    // Continue to the next file instead of aborting the job
                    file_failed = true;
                    break;
                }
            }
        }

        if file_failed {
            failed_files += 1;
            continue;
        }

        // Complete the job file as succeeded
        if let Err(api_err) = env
            .backup_job_api()
            .complete_job_file(CompleteBackupJobFileRequest {
                id: job_file_id,
                uploaded_size: file_uploaded_bytes as i64,
                deduplicated_size: file_deduplicated_bytes as i64,
                total_chunks: total_chunks as i32,
                deduplicated_chunks: file_deduplicated_chunks,
                status: "completed".to_string(),
                error_message: None,
            })
            .await
        {
            error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark job file as completed");
        }

        // Mark the file as backed up in the local SQLite index so it won't be
        // picked as a candidate again on the next backup run. `version_info` is
        // guaranteed Some here: `backup_file` always sends `VersionCreated` as its
        // first event before any `Chunk`/error, and this point is only reached
        // when the stream ran to completion without taking the `Err` branch above.
        let Some((file_version_id, version)) = version_info else {
            error!("Backup stream completed without a VersionCreated event — treating file as failed");
            failed_files += 1;
            continue;
        };

        if let Err(e) = env
            .local_index()
            .mark_backed_up(config_id, &candidate.path, version as i32, file_version_id)
            .await
        {
            error!(error = %e, "Failed to mark file as backed up in local index");
        }

        completed_files += 1;
        completed_bytes += bytes_uploaded;
        total_uploaded_bytes += file_uploaded_bytes;
        total_deduplicated_bytes += file_deduplicated_bytes;

        let _ = tx
            .send(BackupResult::FileCompleted {
                file_path: candidate.path.clone(),
                total_chunks,
                uploaded_bytes: file_uploaded_bytes,
                deduplicated_bytes: file_deduplicated_bytes,
            })
            .await;
    }

    // Complete the backup job
    let (status, error_message) = if failed_files > 0 {
        (
            "completed_with_errors".to_string(),
            Some(format!("{failed_files} file(s) failed during backup")),
        )
    } else {
        ("completed".to_string(), None)
    };

    if let Err(api_err) = env
        .backup_job_api()
        .complete_job(CompleteBackupJobRequest {
            id: job_id,
            status,
            error_message,
        })
        .await
    {
        error!(job_id = %job_id, error = %api_err, "Failed to mark backup job as completed");
    }

    let _ = tx
        .send(BackupResult::Completed {
            total_files: completed_files,
            total_bytes: completed_bytes,
            uploaded_bytes: total_uploaded_bytes,
            deduplicated_bytes: total_deduplicated_bytes,
        })
        .await;

    let cleanup = run_local_cleanup(
        &cleanup_type,
        config_id,
        job_id,
        env.local_index(),
        env.backup_job_api(),
        derived_keys,
    )
    .await;

    let _ = tx
        .send(BackupResult::CleanupCompleted {
            files_deleted: cleanup.files_deleted,
            bytes_freed: cleanup.bytes_freed,
        })
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{
        auth::*,
        backup_config::*,
        backup_job::*,
        chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
        email_verification::{
            ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
            VerifyEmailResponse,
        },
        local_device::*,
        remote_file_version::*,
        remote_storage::*,
        restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
        user::*,
    };
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::model::local_index::LocalIndexEntry;
    use crate::ports::api::{
        ApiResult, backup_config_api_port::BackupConfigApiPort,
        backup_job_api_port::BackupJobApiPort, chunk_api_port::ChunkApiPort,
        local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    };

    // ---------------------------------------------------------------------------
    // Mocks — only the methods `run_backup_job` actually exercises do real work;
    // everything else is `unimplemented!()` per this codebase's mock convention.
    // ---------------------------------------------------------------------------

    #[derive(Clone)]
    struct MockFileVersionApi {
        /// The id/version handed back by the most recent `create()` call, so the
        /// test can assert the local index recorded the *real* server-assigned
        /// values rather than a placeholder.
        last_created: Arc<Mutex<Option<(Uuid, u32)>>>,
    }
    impl MockFileVersionApi {
        fn new() -> Self {
            Self {
                last_created: Arc::new(Mutex::new(None)),
            }
        }
    }
    #[async_trait]
    impl RemoteFileVersionApiPort for MockFileVersionApi {
        async fn create(
            &self,
            request: CreateFileVersionRequest,
        ) -> ApiResult<CreateFileVersionResponse> {
            let id = Uuid::now_v7();
            let version = request.base_version.map_or(1, |v| v + 1);
            *self.last_created.lock().unwrap() = Some((id, version));
            Ok(CreateFileVersionResponse { id, version })
        }
        async fn update_status(&self, _: UpdateFileVersionStatusRequest) -> ApiResult<()> {
            Ok(())
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
    struct MockChunkApi;
    #[async_trait]
    impl ChunkApiPort for MockChunkApi {
        async fn create(&self, _request: CreateChunkRequest) -> ApiResult<CreateChunkResponse> {
            Ok(CreateChunkResponse {
                id: Uuid::now_v7(),
                existed_in_db: false,
            })
        }
        async fn get_chunks_for_version(
            &self,
            _: GetFileVersionChunksRequest,
        ) -> ApiResult<GetFileVersionChunksResponse> {
            unimplemented!()
        }
        async fn update_storage_meta(&self, _: UpdateChunkStorageMetaRequest) -> ApiResult<()> {
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockBackupJobApi {
        completed_job_files: Arc<Mutex<Vec<CompleteBackupJobFileRequest>>>,
        completed_jobs: Arc<Mutex<Vec<CompleteBackupJobRequest>>>,
    }
    impl MockBackupJobApi {
        fn new() -> Self {
            Self {
                completed_job_files: Arc::new(Mutex::new(vec![])),
                completed_jobs: Arc::new(Mutex::new(vec![])),
            }
        }
    }
    #[async_trait]
    impl BackupJobApiPort for MockBackupJobApi {
        async fn create_job(&self, _: CreateBackupJobRequest) -> ApiResult<CreateBackupJobResponse> {
            unimplemented!()
        }
        async fn create_job_file(
            &self,
            _: CreateBackupJobFileRequest,
        ) -> ApiResult<CreateBackupJobFileResponse> {
            Ok(CreateBackupJobFileResponse { id: Uuid::now_v7() })
        }
        async fn complete_job_file(&self, request: CompleteBackupJobFileRequest) -> ApiResult<()> {
            self.completed_job_files.lock().unwrap().push(request);
            Ok(())
        }
        async fn complete_job(&self, request: CompleteBackupJobRequest) -> ApiResult<()> {
            self.completed_jobs.lock().unwrap().push(request);
            Ok(())
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
        async fn rename(&self, _: RenameBackupConfigRequest) -> ApiResult<RenameBackupConfigResponse> {
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
        async fn create(&self, _: CreateLocalDeviceRequest) -> ApiResult<CreateLocalDeviceResponse> {
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
        async fn create(&self, _: CreateRemoteStorageRequest) -> ApiResult<CreateRemoteStorageResponse> {
            unimplemented!()
        }
        async fn get_by_id(&self, _: GetRemoteStorageRequest) -> ApiResult<GetRemoteStorageResponse> {
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
        async fn reauth(&self, _: ReauthRemoteStorageRequest) -> ApiResult<ReauthRemoteStorageResponse> {
            unimplemented!()
        }
    }

    #[derive(Clone)]
    struct TestEnv {
        file_version_api: MockFileVersionApi,
        chunk_api: MockChunkApi,
        backup_job_api: MockBackupJobApi,
        local_index: SqliteLocalIndex,
    }

    impl BackupEnv for TestEnv {
        type UserApi = StubUserApi;
        type BackupConfigApi = StubBackupConfigApi;
        type LocalIndex = SqliteLocalIndex;
        type LocalDeviceApi = StubLocalDeviceApi;
        type RemoteStorageApi = StubRemoteStorageApi;
        type RemoteFileVersionApi = MockFileVersionApi;
        type ChunkApi = MockChunkApi;
        type BackupJobApi = MockBackupJobApi;

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
            &self.file_version_api
        }
        fn chunk_api(&self) -> &Self::ChunkApi {
            &self.chunk_api
        }
        fn backup_job_api(&self) -> &Self::BackupJobApi {
            &self.backup_job_api
        }
    }

    /// Regression test for an incidental bug fixed alongside server-assigned
    /// versioning: `run_backup_job` used to call `mark_backed_up` with a fresh
    /// `Uuid::now_v7()` instead of the real server-assigned `file_version_id`,
    /// which made the local index's `remote_file_id` column meaningless. This
    /// asserts the local index now records the exact `(id, version)` pair the
    /// mock file-version API returned via the `VersionCreated` event.
    #[tokio::test]
    async fn test_run_backup_job_marks_backed_up_with_server_assigned_version_and_id() {
        let storage_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let file_path = source_dir.path().join("file.txt");
        let data = b"backup job regression test content";
        std::fs::write(&file_path, data).unwrap();
        let path_str = file_path.to_str().unwrap().to_string();

        let config_id = Uuid::now_v7();
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let job_id = Uuid::now_v7();
        let dek = Dek::generate().unwrap();
        let derived_keys = DerivedKeys::derive(&dek).unwrap();

        let local_index = SqliteLocalIndex::open_in_memory().await.unwrap();
        // A pre-existing local index row is required — `mark_backed_up` is an
        // UPDATE, not an upsert, mirroring how a real backup run first records
        // the file via filesystem scan before ever attempting to back it up.
        local_index
            .upsert_file(&LocalIndexEntry {
                backup_config_id: config_id,
                path: path_str.clone(),
                size: data.len() as i64,
                mtime: Utc::now(),
                content_hash: None,
                encrypted_name: vec![1, 2, 3],
                encrypted_name_nonce: vec![4, 5, 6],
                blind_index: vec![7, 8, 9],
                remote_file_id: None,
                last_backed_up_version: None,
                synced_at: None,
            })
            .await
            .unwrap();

        let file_version_api = MockFileVersionApi::new();
        let env = TestEnv {
            file_version_api: file_version_api.clone(),
            chunk_api: MockChunkApi,
            backup_job_api: MockBackupJobApi::new(),
            local_index,
        };

        let candidate = crate::model::backup_candidate::ClientBackupCandidate {
            path: path_str.clone(),
            size: data.len() as i64,
            mtime: Utc::now(),
            base_version: Some(5),
            encrypted_name: vec![1, 2, 3],
            encrypted_name_nonce: vec![4, 5, 6],
            blind_index: vec![7, 8, 9],
            remote_file_id: None,
        };

        let storage_config = RemoteStorageConfig::LocalFilesystem(LocalFilesystemConfig {
            root_path: storage_dir.path().to_str().unwrap().to_string(),
        });

        let (tx, mut rx) = mpsc::channel(32);
        // Drain the progress channel concurrently so `run_backup_job` never
        // blocks on a full buffer.
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });

        let result = run_backup_job(
            &tx,
            &env,
            &dek,
            &derived_keys,
            device_id,
            config_id,
            job_id,
            storage_id,
            user_id,
            vec![candidate],
            storage_config,
            api_types::backup_config::CleanupType::NoCleanup,
        )
        .await;
        drop(tx);
        drain.await.unwrap();

        assert!(result.is_ok(), "run_backup_job failed: {:?}", result);

        let (expected_id, expected_version) = file_version_api
            .last_created
            .lock()
            .unwrap()
            .expect("MockFileVersionApi::create should have been called");
        assert_eq!(
            expected_version, 6,
            "server-assigned version should be base_version(5) + 1"
        );

        let entry = env
            .local_index
            .get_by_path(config_id, &path_str)
            .await
            .unwrap()
            .expect("local index entry should still exist");
        assert_eq!(
            entry.remote_file_id,
            Some(expected_id),
            "local index must record the real server-assigned file_version_id, not a placeholder"
        );
        assert_eq!(
            entry.last_backed_up_version,
            Some(expected_version as i32),
            "local index must record the real server-assigned version"
        );
        assert!(entry.synced_at.is_some());
    }
}
