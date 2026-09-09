use std::collections::HashMap;

use api_types::{
    chunk::ChunkStorageMeta,
    gc::{
        ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectResponse,
        GcRunDetailResponse, GetGcRunDetailRequest, GetRetentionSettingsResponse,
        ListGcRunsRequest, ListGcRunsResponse, OrphanedChunkInfo, UpdateRetentionSettingsRequest,
    },
    remote_storage::{GetRemoteStorageRequest, RemoteStorageConfig},
};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    adapters::{
        aes_gcm_encryptor::AesGcmEncryptor,
        google_drive_storage_adaptor::GoogleDriveStorageAdaptor,
        onedrive_storage_adaptor::OneDriveStorageAdaptor, s3_storage_adaptor::S3StorageAdaptor,
        sftp_storage_adaptor::SftpStorageAdaptor,
    },
    applications::gc::env::GcEnv,
    domain::dek::Dek,
    model::{
        base::{AppResult, EncryptedData},
        file::ObjectKey,
    },
    ports::{
        Encryptor,
        api::{gc_api_port::GcApiPort, remote_storage_api_port::RemoteStorageApiPort},
        storage::StoragePort,
    },
};

/// Summary of a completed GC run, returned to the caller.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GcSummary {
    pub gc_run_id: Uuid,
    pub versions_deleted: u64,
    pub chunks_deleted: u64,
    pub storage_freed_bytes: i64,
}

/// Run garbage collection: server collect -> client S3 delete -> server confirm.
///
/// The server identifies expired bin versions and orphaned chunks. The client
/// then deletes orphaned chunks from S3 (since the server cannot access encrypted
/// storage credentials), and confirms the deletions back to the server.
pub async fn run_gc<E: GcEnv>(env: &E, dek: &Dek) -> AppResult<GcSummary> {
    // ── Phase A: Server marks expired versions as Deleted, returns orphaned chunks ──
    info!("Starting GC: calling server collect");
    let collect_response: GcCollectResponse = env.gc_api().collect().await?;
    let gc_run_id = collect_response.gc_run_id;
    let versions_deleted = collect_response.versions_deleted;

    info!(
        gc_run_id = %gc_run_id,
        versions_deleted = versions_deleted,
        orphaned_chunks = collect_response.orphaned_chunks.len(),
        "GC collect complete"
    );

    if collect_response.orphaned_chunks.is_empty() {
        return Ok(GcSummary {
            gc_run_id,
            versions_deleted,
            chunks_deleted: 0,
            storage_freed_bytes: 0,
        });
    }

    // ── Phase B: Client deletes orphaned chunks from storage ───────
    // Group chunks by storage_id so we build one storage adapter per remote storage.
    let mut chunks_by_storage: HashMap<Uuid, Vec<&OrphanedChunkInfo>> = HashMap::new();
    for chunk in &collect_response.orphaned_chunks {
        chunks_by_storage
            .entry(chunk.storage_id)
            .or_default()
            .push(chunk);
    }

    let encryptor = AesGcmEncryptor::new(dek)?;
    let mut successfully_deleted: Vec<Uuid> = Vec::new();

    for (storage_id, chunks) in &chunks_by_storage {
        // Fetch and decrypt storage config
        let storage_response = env
            .remote_storage_api()
            .get_by_id(GetRemoteStorageRequest { id: *storage_id })
            .await?;

        let encrypted_data: EncryptedData = storage_response.storage.config.try_into()?;
        let decrypted_config = encryptor.decrypt(&encrypted_data)?;
        let storage_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_config)?;

        let storage: Box<dyn StoragePort> = match &storage_config {
            RemoteStorageConfig::Aws(s3_creds) => {
                Box::new(S3StorageAdaptor::new(s3_creds.clone()).await?)
            }
            RemoteStorageConfig::GoogleDrive(gdrive_creds) => {
                Box::new(GoogleDriveStorageAdaptor::new(gdrive_creds.clone(), *storage_id).await?)
            }
            RemoteStorageConfig::OneDrive(onedrive_creds) => {
                Box::new(OneDriveStorageAdaptor::new(onedrive_creds.clone(), *storage_id).await?)
            }
            RemoteStorageConfig::LocalFilesystem(cfg) => Box::new(
                crate::adapters::local_fs_storage_adaptor::LocalFsStorageAdaptor::new(
                    &cfg.root_path,
                )?,
            ),
            RemoteStorageConfig::Sftp(sftp_creds) => {
                Box::new(SftpStorageAdaptor::new(sftp_creds.clone())?)
            }
        };

        for chunk in chunks {
            let object_key = match &chunk.storage_meta {
                ChunkStorageMeta::ObjectStore(s3_meta) => ObjectKey::new(s3_meta.key.clone()),
            };

            match storage.delete(&object_key).await {
                Ok(()) => {
                    info!(chunk_id = %chunk.chunk_id, "Deleted orphaned chunk from storage");
                    successfully_deleted.push(chunk.chunk_id);
                }
                Err(e) => {
                    // Log but continue — failed chunks will be retried on next GC run
                    warn!(
                        chunk_id = %chunk.chunk_id,
                        error = %e,
                        "Failed to delete orphaned chunk from storage, will retry next GC"
                    );
                }
            }
        }
    }

    // ── Phase C: Confirm deletions with server ────────────────────
    let confirm_response: ConfirmChunkDeletionsResponse = if !successfully_deleted.is_empty() {
        info!(
            count = successfully_deleted.len(),
            "Confirming chunk deletions with server"
        );
        env.gc_api()
            .confirm_chunk_deletions(ConfirmChunkDeletionsRequest {
                gc_run_id,
                chunk_ids: successfully_deleted,
            })
            .await?
    } else {
        ConfirmChunkDeletionsResponse {
            chunks_deleted: 0,
            storage_freed_bytes: 0,
        }
    };

    let failed_count =
        collect_response.orphaned_chunks.len() as u64 - confirm_response.chunks_deleted;
    if failed_count > 0 {
        error!(
            failed = failed_count,
            "Some orphaned chunks could not be deleted from storage"
        );
    }

    info!(
        gc_run_id = %gc_run_id,
        versions_deleted = versions_deleted,
        chunks_deleted = confirm_response.chunks_deleted,
        storage_freed_bytes = confirm_response.storage_freed_bytes,
        "GC complete"
    );

    Ok(GcSummary {
        gc_run_id,
        versions_deleted,
        chunks_deleted: confirm_response.chunks_deleted,
        storage_freed_bytes: confirm_response.storage_freed_bytes,
    })
}

/// List recent GC runs.
pub async fn list_gc_runs<E: GcEnv>(
    env: &E,
    request: ListGcRunsRequest,
) -> AppResult<ListGcRunsResponse> {
    Ok(env.gc_api().list_runs(request).await?)
}

/// Get detailed info for a specific GC run.
pub async fn get_gc_run_detail<E: GcEnv>(
    env: &E,
    request: GetGcRunDetailRequest,
) -> AppResult<GcRunDetailResponse> {
    Ok(env.gc_api().get_run_detail(request).await?)
}

/// Get the user's retention settings.
pub async fn get_retention_settings<E: GcEnv>(env: &E) -> AppResult<GetRetentionSettingsResponse> {
    Ok(env.gc_api().get_retention_settings().await?)
}

/// Update the user's bin retention period.
pub async fn update_retention_settings<E: GcEnv>(
    env: &E,
    request: UpdateRetentionSettingsRequest,
) -> AppResult<()> {
    Ok(env.gc_api().update_retention_settings(request).await?)
}
