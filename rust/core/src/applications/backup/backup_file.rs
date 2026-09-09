use std::path::PathBuf;

use api_types::{
    chunk::{
        ChunkEncryptionMeta, ChunkObjectStoreMeta, ChunkStatus, ChunkStatusWithTime,
        ChunkStorageMeta, CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest,
    },
    remote_file_version::{
        CreateFileVersionRequest, FileVersionStatus, UpdateFileVersionStatusRequest,
    },
};
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    adapters::{
        aes_gcm_encryptor::AesGcmEncryptor, fixed_size_chunker::FixedSizeChunker,
        zstd_compressor::ZstdCompressor,
    },
    applications::backup::env::BackupEnv,
    domain::dek::Dek,
    model::backup_candidate::ClientBackupCandidate,
    model::base::{AppResult, BackupFileEvent, ChunkMeta},
    model::file::ObjectKey,
    ports::{
        AsyncChunker, Compressor, Encryptor, api::chunk_api_port::ChunkApiPort,
        api::remote_file_version_api_port::RemoteFileVersionApiPort, storage::StoragePort,
    },
};

/// Deterministically derives the idempotency key for a file-version creation request.
///
/// Must be deterministic (not random-per-attempt): a "retry" here isn't an in-process
/// loop, it's the *next scheduled backup run* picking the same unsynced candidate back
/// up from a fresh process with no memory of the previous attempt. The same file
/// identity + observed base_version always yields the same key, so a crashed/restarted
/// attempt is recognized server-side as a retry of the same intent rather than a new one.
fn file_version_idempotency_key(
    backup_config_id: Uuid,
    name_blind_index: &[u8],
    base_version: Option<u32>,
) -> Uuid {
    let name = format!(
        "fv:{}:{}:{:?}",
        backup_config_id,
        hex::encode(name_blind_index),
        base_version
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

/// Deterministically derives the idempotency key for a chunk registration request.
/// See `file_version_idempotency_key` for why this must be deterministic, not random.
fn chunk_idempotency_key(remote_file_version_id: Uuid, chunk_index: i32, hash: &[u8; 32]) -> Uuid {
    let name = format!(
        "chunk:{}:{}:{}",
        remote_file_version_id,
        chunk_index,
        hex::encode(hash)
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

/// Creates a remote file version entry with `Uploading` status to track an in-progress backup.
///
/// This is the first step of backing up a file — it registers the intent to upload a new
/// version so the server can track the upload lifecycle (Uploading → Completed/Failed). The
/// server assigns the actual version number based on `base_version` (optimistic concurrency);
/// returns both the new version id and the server-assigned version number.
async fn create_file_version<E: BackupEnv>(
    env: &E,
    candidate: &ClientBackupCandidate,
    backup_config_id: Uuid,
    device_id: Uuid,
    storage_id: Uuid,
) -> AppResult<(Uuid, u32)> {
    let idempotency_key = file_version_idempotency_key(
        backup_config_id,
        &candidate.blind_index,
        candidate.base_version,
    );
    let create_request = CreateFileVersionRequest {
        backup_config_id,
        device_id,
        storage_id,
        size: candidate.size,
        status: FileVersionStatus::Uploading,
        local_file_updated_at: candidate.mtime,
        base_version: candidate.base_version,
        idempotency_key,
        encrypted_name: candidate.encrypted_name.clone(),
        name_nonce: candidate.encrypted_name_nonce.clone(),
        name_blind_index: candidate.blind_index.clone(),
    };
    let response = env.remote_file_version_api().create(create_request).await?;
    debug!(
        version = response.version,
        version_id = %response.id,
        "File version created"
    );
    Ok((response.id, response.version))
}

/// Checks if a chunk with the same SHA-256 hash already exists in storage. If it does,
/// the upload is skipped (deduplicated), saving bandwidth and storage costs. Otherwise,
/// the chunk data is compressed with zstd, encrypted with AES-256-GCM, and uploaded.
///
/// Returns `(uploaded_size, deduplicated, encryption_meta)`:
/// - `uploaded_size`: bytes written to storage (0 if deduplicated)
/// - `deduplicated`: true if the chunk was already present in storage
/// - `encryption_meta`: nonce + algorithm used to encrypt (None if deduplicated)
async fn upload_or_deduplicate_chunk(
    storage: &dyn StoragePort,
    compressor: &ZstdCompressor,
    encryptor: &AesGcmEncryptor,
    key: &ObjectKey,
    data: &[u8],
) -> AppResult<(u32, bool, Option<ChunkEncryptionMeta>)> {
    if storage.exists(key).await? {
        debug!(key = %key.as_str(), "Chunk deduplicated (exists in storage)");
        return Ok((0u32, true, None));
    }
    let compressed_data = compressor.compress(data)?;
    let encrypted_data = encryptor.encrypt(&compressed_data)?;
    let size = encrypted_data.ciphertext.len() as u32;
    storage.put(key, encrypted_data.ciphertext).await?;
    debug!(key = %key.as_str(), uploaded_size = size, original_size = data.len(), "Chunk uploaded");
    Ok((
        size,
        false,
        Some(ChunkEncryptionMeta {
            nonce: encrypted_data.nonce,
            algorithm: encrypted_data.algorithm,
        }),
    ))
}

/// Re-encrypts and uploads a chunk that exists in storage but not in the DB.
///
/// This handles the edge case where S3 has stale chunks from a previous DB lifetime
/// (e.g. DB was reset but S3 was not cleaned). The chunk must be re-encrypted and
/// re-uploaded so the nonce in the new DB row matches the ciphertext in storage.
async fn reupload_chunk(
    storage: &dyn StoragePort,
    compressor: &ZstdCompressor,
    encryptor: &AesGcmEncryptor,
    key: &ObjectKey,
    data: &[u8],
) -> AppResult<Option<ChunkEncryptionMeta>> {
    let compressed_data = compressor.compress(data)?;
    let encrypted_data = encryptor.encrypt(&compressed_data)?;
    storage.put(key, encrypted_data.ciphertext).await?;
    Ok(Some(ChunkEncryptionMeta {
        nonce: encrypted_data.nonce,
        algorithm: encrypted_data.algorithm,
    }))
}

/// Registers a processed chunk in the API server and returns the server response plus chunk metadata.
///
/// After a chunk has been uploaded (or deduplicated), this function creates the chunk
/// record on the server with its storage location (S3 key), hash, size, and status.
/// Returns both the server response (which includes `existed_in_db`) and the `ChunkMeta`
/// for progress tracking.
async fn register_chunk<E: BackupEnv>(
    env: &E,
    chunk: &crate::model::base::Chunk,
    storage_key: &ObjectKey,
    storage_id: Uuid,
    file_version_id: Uuid,
    chunk_index: i32,
    uploaded_size: u32,
    deduplicated: bool,
    encryption: Option<ChunkEncryptionMeta>,
) -> AppResult<(CreateChunkResponse, ChunkMeta)> {
    let original_size = chunk.data.len() as u32;

    let create_chunk_request = CreateChunkRequest {
        hash: chunk.hash.to_vec(),
        size: chunk.data.len() as u64,
        storage_id,
        remote_file_version_id: file_version_id,
        chunk_index,
        storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
            key: storage_key.as_str().to_string(),
            hash: chunk.hash_string(),
            encryption,
        }),
        status_history: vec![ChunkStatusWithTime {
            timestamp: Utc::now(),
            status: ChunkStatus::Verified,
        }],
        idempotency_key: chunk_idempotency_key(file_version_id, chunk_index, &chunk.hash),
    };
    let response = env.chunk_api().create(create_chunk_request).await?;

    debug!(
        chunk_index,
        storage_key = %storage_key.as_str(),
        deduplicated,
        existed_in_db = response.existed_in_db,
        "Chunk registered"
    );

    let meta = ChunkMeta {
        index: chunk.index,
        hash: chunk.hash,
        size: original_size,
        uploaded_size,
        deduplicated,
    };

    Ok((response, meta))
}

/// Backs up a single file by chunking, compressing, encrypting, and uploading it to remote storage.
///
/// This is the core file-level backup routine. It works as follows:
///
/// 1. Creates a new `RemoteFileVersion` entry with status `Uploading` to track this upload.
/// 2. Splits the file into fixed-size chunks (4 MiB each) using `FixedSizeChunker`.
/// 3. For each chunk, checks if the chunk already exists in remote storage (content-addressable
///    deduplication via SHA-256 hash). If the chunk exists, it is marked as deduplicated and
///    the upload is skipped, saving bandwidth and storage costs.
/// 4. For new chunks: compresses with zstd (level 4), encrypts with AES-256-GCM using the
///    provided DEK, then uploads the ciphertext to remote storage.
/// 5. Registers each chunk in the API server with its storage metadata and status.
/// 6. Streams `ChunkMeta` results back through an `mpsc` channel so the caller can track
///    progress in real time (bytes uploaded, deduplication stats, etc.).
///
/// The function spawns a background tokio task and returns a receiver immediately, making it
/// non-blocking for the caller. If the receiver is dropped, the background task stops gracefully.
#[tracing::instrument(skip(env, storage, dek, candidate), fields(base_version = ?candidate.base_version, size = candidate.size))]
pub fn backup_file<E, S>(
    env: E,
    storage: S,
    candidate: ClientBackupCandidate,
    backup_config_id: Uuid,
    device_id: Uuid,
    storage_id: Uuid,
    user_id: Uuid,
    dek: Dek,
) -> mpsc::Receiver<AppResult<BackupFileEvent>>
where
    E: BackupEnv + Send + Sync + 'static,
    S: StoragePort + 'static,
{
    let (tx, rx) = mpsc::channel(16);

    tokio::spawn(async move {
        let (file_version_id, version) =
            match create_file_version(&env, &candidate, backup_config_id, device_id, storage_id)
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to create file version");
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            };

        if tx
            .send(Ok(BackupFileEvent::VersionCreated {
                file_version_id,
                version,
            }))
            .await
            .is_err()
        {
            // Receiver dropped before chunk work started.
            return;
        }

        info!(file_size = candidate.size, version, "Backing up file");

        let path = PathBuf::from(&candidate.path);
        let chunker = FixedSizeChunker::new(4 * 1024 * 1024);
        let mut chunk_rx = chunker.stream_chunks(path);
        let compressor = ZstdCompressor::new(4);

        let encryptor = match AesGcmEncryptor::new(&dek) {
            Ok(enc) => enc,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create encryptor for backup");
                let _ = tx.send(Err(e)).await;
                return;
            }
        };

        let mut chunk_index: i32 = 0;
        let mut chunk_error = false;
        while let Some(chunk_result) = chunk_rx.recv().await {
            let result = async {
                let chunk = chunk_result?;
                let hash_key = ObjectKey::new(format!("{}/{}", user_id, chunk.hash_string()));

                let (uploaded_size, deduplicated, encryption) = upload_or_deduplicate_chunk(
                    &storage,
                    &compressor,
                    &encryptor,
                    &hash_key,
                    &chunk.data,
                )
                .await?;

                let (response, mut meta) = register_chunk(
                    &env,
                    &chunk,
                    &hash_key,
                    storage_id,
                    file_version_id,
                    chunk_index,
                    uploaded_size,
                    deduplicated,
                    encryption,
                )
                .await?;

                // Edge case: S3 has a stale chunk (from a previous DB lifetime) but the DB
                // didn't have it. The dedup path skipped encryption, but the DB row was freshly
                // inserted with encryption: None. Re-encrypt, re-upload, and patch the DB row.
                if deduplicated && !response.existed_in_db {
                    tracing::warn!(
                        chunk_index,
                        hash = %hash_key.as_str(),
                        "Stale S3 chunk detected (exists in S3 but not in DB). Re-uploading."
                    );
                    let fresh_encryption =
                        reupload_chunk(&storage, &compressor, &encryptor, &hash_key, &chunk.data)
                            .await?;
                    let updated_meta = ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                        key: hash_key.as_str().to_string(),
                        hash: chunk.hash_string(),
                        encryption: fresh_encryption,
                    });
                    env.chunk_api()
                        .update_storage_meta(UpdateChunkStorageMetaRequest {
                            chunk_id: response.id,
                            storage_meta: updated_meta,
                        })
                        .await?;
                    meta.deduplicated = false;
                }

                Ok(meta)
            }
            .await;

            if result.is_err() {
                chunk_error = true;
            }
            chunk_index += 1;

            if tx.send(result.map(BackupFileEvent::Chunk)).await.is_err() {
                // Receiver dropped, stop processing
                break;
            }
        }
        if chunk_error {
            // Leave status as Uploading — the caller (backup_job) will mark
            // the job file as failed. The next backup run will reset via ON CONFLICT.
            return;
        }

        info!(total_chunks = chunk_index, "File backup completed");

        // Mark the file version as UploadCompleted, then VerifiedOnRemoteStorage.
        if let Err(e) = env
            .remote_file_version_api()
            .update_status(UpdateFileVersionStatusRequest {
                id: file_version_id,
                status: FileVersionStatus::UploadCompleted,
            })
            .await
        {
            tracing::error!(version_id = %file_version_id, error = %e, "Failed to mark file version as UploadCompleted");
        }

        if let Err(e) = env
            .remote_file_version_api()
            .update_status(UpdateFileVersionStatusRequest {
                id: file_version_id,
                status: FileVersionStatus::VerifiedOnRemoteStorage,
            })
            .await
        {
            tracing::error!(version_id = %file_version_id, error = %e, "Failed to mark file version as VerifiedOnRemoteStorage");
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{
        auth::*,
        backup_config::*,
        backup_job::*,
        chunk::{CreateChunkResponse, UpdateChunkStorageMetaRequest},
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use crate::adapters::sqlite_local_index::SqliteLocalIndex;
    use crate::ports::api::{
        ApiClientError, ApiResult, backup_config_api_port::BackupConfigApiPort,
        backup_job_api_port::BackupJobApiPort, chunk_api_port::ChunkApiPort,
        local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    };
    use api_types::error::ApiError;
    use crate::model::app_error::AppError;

    #[derive(Clone)]
    struct MockStorage {
        existing_keys: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }
    impl MockStorage {
        fn new() -> Self {
            Self {
                existing_keys: Arc::new(Mutex::new(HashMap::new())),
            }
        }
        fn with_existing_key(key: &str) -> Self {
            let mut map = HashMap::new();
            map.insert(key.to_string(), vec![]);
            Self {
                existing_keys: Arc::new(Mutex::new(map)),
            }
        }
        fn stored_keys(&self) -> Vec<String> {
            self.existing_keys.lock().unwrap().keys().cloned().collect()
        }
    }
    #[async_trait]
    impl StoragePort for MockStorage {
        async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
            self.existing_keys
                .lock()
                .unwrap()
                .insert(key.as_str().to_string(), data);
            Ok(())
        }
        async fn get(&self, _key: &ObjectKey) -> AppResult<Vec<u8>> {
            Ok(vec![])
        }
        async fn delete(&self, _key: &ObjectKey) -> AppResult<()> {
            Ok(())
        }
        async fn list(&self, _prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
            Ok(vec![])
        }
        async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
            Ok(self
                .existing_keys
                .lock()
                .unwrap()
                .contains_key(key.as_str()))
        }
    }

    #[derive(Clone)]
    struct MockFileVersionApi {
        created: Arc<Mutex<Vec<CreateFileVersionRequest>>>,
        /// When set, `create` always returns a 409 Conflict instead of succeeding —
        /// used to test that `backup_file` surfaces the conflict without touching chunks.
        fail_with_conflict: Arc<Mutex<bool>>,
    }
    impl MockFileVersionApi {
        fn new() -> Self {
            Self {
                created: Arc::new(Mutex::new(vec![])),
                fail_with_conflict: Arc::new(Mutex::new(false)),
            }
        }
        fn new_always_conflicting() -> Self {
            Self {
                created: Arc::new(Mutex::new(vec![])),
                fail_with_conflict: Arc::new(Mutex::new(true)),
            }
        }
    }
    #[async_trait]
    impl RemoteFileVersionApiPort for MockFileVersionApi {
        async fn create(
            &self,
            request: CreateFileVersionRequest,
        ) -> ApiResult<CreateFileVersionResponse> {
            if *self.fail_with_conflict.lock().unwrap() {
                return Err(ApiClientError::Api(ApiError::Conflict {
                    message: "stale base_version".to_string(),
                }));
            }
            let version = request.base_version.map_or(1, |v| v + 1);
            self.created.lock().unwrap().push(request);
            Ok(CreateFileVersionResponse {
                id: Uuid::now_v7(),
                version,
            })
        }
        async fn update_status(&self, _: UpdateFileVersionStatusRequest) -> ApiResult<()> {
            Ok(())
        }
        async fn list_backed_up_files(
            &self,
            _: ListBackedUpFilesRequest,
        ) -> ApiResult<ListBackedUpFilesResponse> {
            Ok(ListBackedUpFilesResponse {
                files: vec![],
                has_more: false,
                next_cursor: None,
                total_files: Some(0),
            })
        }
        async fn list_all_versions(
            &self,
            _: ListAllVersionsRequest,
        ) -> ApiResult<ListAllVersionsResponse> {
            Ok(ListAllVersionsResponse { versions: vec![] })
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
    struct MockChunkApi {
        created: Arc<Mutex<Vec<CreateChunkRequest>>>,
        known_hashes: Arc<Mutex<std::collections::HashSet<Vec<u8>>>>,
    }
    impl MockChunkApi {
        fn new() -> Self {
            Self {
                created: Arc::new(Mutex::new(vec![])),
                known_hashes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            }
        }
    }
    #[async_trait]
    impl ChunkApiPort for MockChunkApi {
        async fn create(&self, request: CreateChunkRequest) -> ApiResult<CreateChunkResponse> {
            let existed_in_db = !self
                .known_hashes
                .lock()
                .unwrap()
                .insert(request.hash.clone());
            self.created.lock().unwrap().push(request);
            Ok(CreateChunkResponse {
                id: Uuid::now_v7(),
                existed_in_db,
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
        file_version_api: MockFileVersionApi,
        chunk_api: MockChunkApi,
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
            &self.file_version_api
        }
        fn chunk_api(&self) -> &Self::ChunkApi {
            &self.chunk_api
        }
        fn backup_job_api(&self) -> &Self::BackupJobApi {
            &StubBackupJobApi
        }
    }

    fn make_candidate(file_path: &str, size: i64) -> ClientBackupCandidate {
        ClientBackupCandidate {
            path: file_path.to_string(),
            size,
            mtime: Utc::now(),
            base_version: None,
            encrypted_name: vec![1, 2, 3],
            encrypted_name_nonce: vec![4, 5, 6],
            blind_index: vec![7, 8, 9],
            remote_file_id: None,
        }
    }

    async fn make_env() -> TestEnv {
        TestEnv {
            file_version_api: MockFileVersionApi::new(),
            chunk_api: MockChunkApi::new(),
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        }
    }

    /// Drains a `backup_file` event stream for the happy path: asserts `VersionCreated`
    /// is the first event (before any chunk work), then collects the remaining `Chunk`
    /// events in order. Panics on any error or a second `VersionCreated` — tests that
    /// expect an error read the raw receiver directly instead of using this helper.
    async fn drain_events(
        rx: &mut mpsc::Receiver<AppResult<BackupFileEvent>>,
    ) -> (Uuid, u32, Vec<ChunkMeta>) {
        let first = rx
            .recv()
            .await
            .expect("channel closed before any event")
            .expect("first event should not be an error");
        let (file_version_id, version) = match first {
            BackupFileEvent::VersionCreated {
                file_version_id,
                version,
            } => (file_version_id, version),
            BackupFileEvent::Chunk(_) => panic!("expected VersionCreated as the first event"),
        };

        let mut chunks = vec![];
        while let Some(result) = rx.recv().await {
            match result.expect("chunk should succeed") {
                BackupFileEvent::VersionCreated { .. } => {
                    panic!("VersionCreated must only be sent once")
                }
                BackupFileEvent::Chunk(meta) => chunks.push(meta),
            }
        }
        (file_version_id, version, chunks)
    }

    #[tokio::test]
    async fn test_backup_small_file_single_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("small.txt");
        let data = b"hello world backup test";
        std::fs::write(&file_path, data).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage.clone(),
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );

        let (_, _, chunks) = drain_events(&mut rx).await;

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].size, data.len() as u32);
        assert!(!chunks[0].deduplicated);
        assert!(chunks[0].uploaded_size > 0);
        assert_eq!(env.file_version_api.created.lock().unwrap().len(), 1);
        assert_eq!(env.chunk_api.created.lock().unwrap().len(), 1);
        assert_eq!(storage.stored_keys().len(), 1);
        let key = storage.stored_keys().into_iter().next().unwrap();
        assert!(
            key.starts_with(&format!("{}/", user_id)),
            "storage key should be namespaced under user_id"
        );
    }

    #[tokio::test]
    async fn test_backup_large_file_multiple_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("large.bin");
        let data = vec![0xABu8; 5 * 1024 * 1024];
        std::fs::write(&file_path, &data).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage.clone(),
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );

        let (_, _, chunks) = drain_events(&mut rx).await;

        assert!(chunks.len() >= 2);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i as u32);
        }
        let total_size: u32 = chunks.iter().map(|c| c.size).sum();
        assert_eq!(total_size, data.len() as u32);
        assert!(!chunks[0].deduplicated);
        assert_eq!(env.chunk_api.created.lock().unwrap().len(), chunks.len());
        assert!(storage.stored_keys().len() >= 1);
    }

    #[tokio::test]
    async fn test_backup_deduplicates_existing_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("dedup.txt");
        let data = b"dedup test content";
        std::fs::write(&file_path, data).unwrap();

        let env = make_env().await;
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();

        let storage = MockStorage::new();
        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage.clone(),
            candidate.clone(),
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek.clone(),
        );
        while let Some(result) = rx.recv().await {
            result.unwrap();
        }

        let keys = storage.stored_keys();
        assert_eq!(keys.len(), 1);
        let storage_with_existing = MockStorage::with_existing_key(&keys[0]);

        let mut rx2 = backup_file(
            env.clone(),
            storage_with_existing,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );
        let (_, _, second_chunks) = drain_events(&mut rx2).await;

        assert_eq!(second_chunks.len(), 1);
        assert!(second_chunks[0].deduplicated);
        assert_eq!(second_chunks[0].uploaded_size, 0);
        assert_eq!(env.chunk_api.created.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_backup_reuploads_stale_s3_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("stale.txt");
        let data = b"stale chunk test content";
        std::fs::write(&file_path, data).unwrap();

        let env1 = make_env().await;
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let storage = MockStorage::new();
        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env1.clone(),
            storage.clone(),
            candidate.clone(),
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek.clone(),
        );
        while let Some(result) = rx.recv().await {
            result.unwrap();
        }
        let keys = storage.stored_keys();
        assert_eq!(keys.len(), 1);

        let env2 = make_env().await;
        let storage_with_existing = MockStorage::with_existing_key(&keys[0]);
        let mut rx2 = backup_file(
            env2.clone(),
            storage_with_existing,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );
        let (_, _, chunks) = drain_events(&mut rx2).await;

        assert_eq!(chunks.len(), 1);
        assert!(!chunks[0].deduplicated);
        assert_eq!(env2.chunk_api.created.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_backup_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        std::fs::File::create(&file_path).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), 0);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage.clone(),
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );
        let (_, _, chunks) = drain_events(&mut rx).await;

        assert_eq!(chunks.len(), 0);
        assert_eq!(env.file_version_api.created.lock().unwrap().len(), 1);
        assert_eq!(env.chunk_api.created.lock().unwrap().len(), 0);
        assert_eq!(storage.stored_keys().len(), 0);
    }

    #[tokio::test]
    async fn test_backup_nonexistent_file_returns_error() {
        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate("/tmp/nonexistent_backup_test_file_xyz.bin", 100);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env,
            storage,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );

        let mut got_error = false;
        while let Some(result) = rx.recv().await {
            if result.is_err() {
                got_error = true;
                break;
            }
        }
        assert!(got_error);
    }

    #[tokio::test]
    async fn test_file_version_created_with_correct_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("meta.txt");
        let data = b"metadata test";
        std::fs::write(&file_path, data).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let candidate = ClientBackupCandidate {
            path: file_path.to_str().unwrap().to_string(),
            size: data.len() as i64,
            mtime: Utc::now(),
            base_version: Some(3),
            encrypted_name: vec![10, 20],
            encrypted_name_nonce: vec![30, 40],
            blind_index: vec![50, 60],
            remote_file_id: None,
        };

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );
        while rx.recv().await.is_some() {}

        let created = env.file_version_api.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].device_id, device_id);
        assert_eq!(created[0].storage_id, storage_id);
        assert_eq!(created[0].base_version, Some(3));
        assert_eq!(created[0].size, data.len() as i64);
        assert_eq!(created[0].status, FileVersionStatus::Uploading);
        assert_eq!(created[0].encrypted_name, vec![10, 20]);
        assert_eq!(created[0].name_nonce, vec![30, 40]);
        assert_eq!(created[0].name_blind_index, vec![50, 60]);
    }

    #[tokio::test]
    async fn test_chunk_entries_have_correct_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("chunks_meta.bin");
        let data = vec![0x42u8; 5 * 1024 * 1024];
        std::fs::write(&file_path, &data).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);

        let user_id = Uuid::now_v7();
        let mut rx = backup_file(
            env.clone(),
            storage,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );
        while let Some(r) = rx.recv().await {
            r.unwrap();
        }

        let created_chunks = env.chunk_api.created.lock().unwrap();
        assert!(created_chunks.len() >= 2);
        for (i, chunk) in created_chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as i32);
            assert_eq!(chunk.storage_id, storage_id);
        }
        let total_size: u64 = created_chunks.iter().map(|c| c.size).sum();
        assert_eq!(total_size, data.len() as u64);
    }

    #[tokio::test]
    async fn test_backup_file_emits_version_created_before_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("order.txt");
        let data = b"order test content";
        std::fs::write(&file_path, data).unwrap();

        let env = make_env().await;
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();

        let mut rx = backup_file(
            env.clone(),
            storage,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );

        // The very first event on the channel must be VersionCreated, before any
        // chunk work has happened — callers rely on this to record the real
        // file_version_id/version before any chunk data hits the wire.
        let first = rx
            .recv()
            .await
            .expect("expected an event")
            .expect("first event should not be an error");
        match first {
            BackupFileEvent::VersionCreated { version, .. } => assert_eq!(version, 1),
            BackupFileEvent::Chunk(_) => {
                panic!("VersionCreated must be sent before any Chunk event")
            }
        }

        let mut saw_chunk = false;
        while let Some(result) = rx.recv().await {
            if let BackupFileEvent::Chunk(_) = result.expect("chunk should succeed") {
                saw_chunk = true;
            }
        }
        assert!(
            saw_chunk,
            "expected at least one Chunk event after VersionCreated"
        );
    }

    #[tokio::test]
    async fn test_backup_file_surfaces_conflict_error_without_chunk_processing() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("conflict.txt");
        let data = b"conflict test content";
        std::fs::write(&file_path, data).unwrap();

        // The mock file-version API always returns 409 Conflict — simulating a
        // stale base_version. No chunk work should ever happen in this case.
        let env = TestEnv {
            file_version_api: MockFileVersionApi::new_always_conflicting(),
            chunk_api: MockChunkApi::new(),
            local_index: SqliteLocalIndex::open_in_memory().await.unwrap(),
        };
        let storage = MockStorage::new();
        let dek = Dek::generate().unwrap();
        let candidate = make_candidate(file_path.to_str().unwrap(), data.len() as i64);
        let device_id = Uuid::now_v7();
        let storage_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();

        let mut rx = backup_file(
            env.clone(),
            storage,
            candidate,
            Uuid::now_v7(),
            device_id,
            storage_id,
            user_id,
            dek,
        );

        let first = rx.recv().await.expect("expected a conflict event");
        assert!(
            matches!(first, Err(AppError::Conflict { .. })),
            "expected a Conflict error, got {:?}",
            first
        );
        assert!(
            rx.recv().await.is_none(),
            "no chunk events should be sent after a version-creation conflict"
        );
        assert_eq!(env.chunk_api.created.lock().unwrap().len(), 0);
    }

    /// Same inputs must always derive the same key (so a crashed/restarted backup
    /// run is recognized as a retry of the same intent), and any changed input
    /// must derive a different key (so genuinely different requests never collide).
    #[test]
    fn test_create_file_version_idempotency_key_is_deterministic() {
        let backup_config_id = Uuid::now_v7();
        let blind_index = vec![1, 2, 3, 4];

        let key_a = file_version_idempotency_key(backup_config_id, &blind_index, Some(2));
        let key_b = file_version_idempotency_key(backup_config_id, &blind_index, Some(2));
        assert_eq!(key_a, key_b, "identical inputs must derive identical keys");

        let key_different_base_version =
            file_version_idempotency_key(backup_config_id, &blind_index, Some(3));
        assert_ne!(key_a, key_different_base_version);

        let key_different_config =
            file_version_idempotency_key(Uuid::now_v7(), &blind_index, Some(2));
        assert_ne!(key_a, key_different_config);

        let key_none_base_version = file_version_idempotency_key(backup_config_id, &blind_index, None);
        assert_ne!(key_a, key_none_base_version);
    }

    /// See `test_create_file_version_idempotency_key_is_deterministic` for why
    /// determinism (not randomness) matters here.
    #[test]
    fn test_register_chunk_idempotency_key_is_deterministic() {
        let file_version_id = Uuid::now_v7();
        let hash = [7u8; 32];

        let key_a = chunk_idempotency_key(file_version_id, 0, &hash);
        let key_b = chunk_idempotency_key(file_version_id, 0, &hash);
        assert_eq!(key_a, key_b, "identical inputs must derive identical keys");

        let key_different_index = chunk_idempotency_key(file_version_id, 1, &hash);
        assert_ne!(key_a, key_different_index);

        let mut different_hash = hash;
        different_hash[0] ^= 0xFF;
        let key_different_hash = chunk_idempotency_key(file_version_id, 0, &different_hash);
        assert_ne!(key_a, key_different_hash);
    }
}
