use api_types::{
    backup_config::GetBackupConfigRequest,
    chunk::ChunkStorageMeta,
    remote_storage::{
        GetRemoteStorageRequest, RemoteStorageConfig, RemoteStorageStatus,
        UpdateRemoteStorageStatusRequest,
    },
    restore::RestoreResult,
    restore_file_info::{GetFileVersionChunksRequest, RestoreChunkInfo},
    restore_job::{
        CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
        CreateRestoreJobRequest, OverwriteBehavior, RestoreDestination,
    },
};
use chrono::Utc;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    adapters::{aes_gcm_encryptor::AesGcmEncryptor, zstd_compressor::ZstdCompressor},
    applications::{
        backup::backup_config::decrypt_backup_config,
        restore::{
            env::RestoreEnv,
            restore_file::{restore_file, restore_single_chunk},
            storage_factory::{StorageLayout, create_storage_port},
        },
    },
    domain::{dek::Dek, derived_keys::DerivedKeys, metadata_crypto::encrypt_file_path},
    model::app_error::AppError,
    model::base::{AppResult, EncryptedData},
    model::file::ObjectKey,
    ports::{
        Encryptor, api::backup_config_api_port::BackupConfigApiPort,
        api::chunk_api_port::ChunkApiPort, api::remote_storage_api_port::RemoteStorageApiPort,
        api::restore_job_api_port::RestoreJobApiPort, storage::StoragePort,
    },
};

pub use api_types::restore::RestoreResult as RestoreResultType;

/// A file selected for restore, provided by the caller (UI / Tauri command).
#[derive(Debug, Clone)]
pub struct RestoreFileSelection {
    pub version_id: Uuid,
    pub file_path: String,
    pub size: i64,
    pub version: u32,
}

enum RestoreTarget {
    Local { base_dest: PathBuf },
    ConfiguredStorage { storage: Box<dyn StoragePort> },
}

enum RemoteConflictDecision {
    Write(ObjectKey),
    Skip,
}

enum RemoteRestoreOutcome {
    Restored {
        bytes_restored: u64,
        chunks_completed: u32,
        destination_key: ObjectKey,
    },
    Skipped,
}

/// Starts a restore job that downloads, decrypts, decompresses, and reassembles
/// the selected file versions.
///
/// This is the top-level entry point for restore, mirroring `start_backup` from backup_job.rs.
/// It spawns a background tokio task and returns an mpsc receiver that streams `RestoreResult`
/// events for progress tracking.
///
/// High-level flow:
/// 1. Fetch and decrypt storage config (same as backup)
/// 2. Pre-restore integrity check — verify all required chunks exist in remote storage
/// 3. Create restore job in DB
/// 4. For each selected file: fetch chunks, download+decrypt+decompress, write atomically
/// 5. Complete restore job
pub fn start_restore<E: RestoreEnv + Send + Sync + 'static>(
    env: E,
    dek: Dek,
    config_id: Uuid,
    file_selections: Vec<RestoreFileSelection>,
    destination: RestoreDestination,
    overwrite_behavior: OverwriteBehavior,
) -> mpsc::Receiver<RestoreResult> {
    let (tx, rx) = mpsc::channel(32);

    tokio::spawn(async move {
        if let Err(reason) = run_restore(
            &tx,
            &env,
            &dek,
            config_id,
            &file_selections,
            &destination,
            &overwrite_behavior,
        )
        .await
        {
            error!(config_id = %config_id, error = %reason, "Restore failed");

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
                .send(RestoreResult::Failed {
                    reason: reason.to_string(),
                    file_path: None,
                })
                .await
                .is_err()
            {
                error!(config_id = %config_id, "Restore result channel closed — UI may not receive failure notification");
            }
        }
    });

    rx
}

#[tracing::instrument(skip(tx, env, dek, file_selections), fields(config_id = %config_id, num_files = file_selections.len()))]
async fn run_restore<E: RestoreEnv + Send + Sync + 'static>(
    tx: &mpsc::Sender<RestoreResult>,
    env: &E,
    dek: &Dek,
    config_id: Uuid,
    file_selections: &[RestoreFileSelection],
    destination: &RestoreDestination,
    overwrite_behavior: &OverwriteBehavior,
) -> AppResult<()> {
    // ── 1. Fetch backup config + remote storage config ────────────
    info!("Fetching backup config and storage config");
    let config_response = env
        .backup_config_api()
        .get_by_id(GetBackupConfigRequest { id: config_id })
        .await?;
    let derived_keys = DerivedKeys::derive(dek)?;
    let backup_config = decrypt_backup_config(config_response.config, &derived_keys.metadata_key)?;

    let storage_response = env
        .remote_storage_api()
        .get_by_id(GetRemoteStorageRequest {
            id: backup_config.storage_id,
        })
        .await?;
    let remote_storage = storage_response.storage;

    // Decrypt storage config using DEK
    let encrypted_data: EncryptedData = remote_storage.config.try_into()?;
    let encryptor = AesGcmEncryptor::new(dek)?;
    let decrypted_config = encryptor.decrypt(&encrypted_data)?;
    let storage_config: RemoteStorageConfig = serde_json::from_slice(&decrypted_config)?;

    let _ = tx
        .send(RestoreResult::ConfigLoaded {
            storage_id: backup_config.storage_id,
        })
        .await;

    let (destination_type, destination_storage_id, destination_prefix) =
        restore_destination_metadata(destination, backup_config.user_id);

    // ── 2. Create restore job in DB (early, so failures are recorded) ──
    let job_response = env
        .restore_job_api()
        .create_job(CreateRestoreJobRequest {
            backup_config_id: config_id,
            overwrite_behavior: overwrite_behavior.clone(),
            destination_type,
            destination_storage_id,
            destination_prefix,
            started_at: Utc::now(),
        })
        .await?;
    let job_id = job_response.id;

    // From here on, any failure must be recorded against this job.
    // We delegate to `run_restore_inner` and handle errors in the caller.
    match run_restore_inner(
        tx,
        env,
        dek,
        &backup_config,
        &storage_config,
        file_selections,
        destination,
        overwrite_behavior,
        &derived_keys,
        job_id,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) => {
            // Mark the job as failed in DB so it appears in reports
            if let Err(api_err) = env
                .restore_job_api()
                .complete_job(CompleteRestoreJobRequest {
                    id: job_id,
                    status: "failed".to_string(),
                    error_message: Some(e.to_string()),
                })
                .await
            {
                error!(job_id = %job_id, error = %api_err, "Failed to mark restore job as failed");
            }
            Err(e)
        }
    }
}

/// Inner restore logic, called after the job is created in the DB.
/// Any error returned will be recorded against the job by the caller.
#[tracing::instrument(skip_all, fields(job_id = %job_id))]
async fn run_restore_inner<E: RestoreEnv + Send + Sync + 'static>(
    tx: &mpsc::Sender<RestoreResult>,
    env: &E,
    dek: &Dek,
    backup_config: &api_types::backup_config::DecryptedBackupConfig,
    storage_config: &RemoteStorageConfig,
    file_selections: &[RestoreFileSelection],
    destination: &RestoreDestination,
    overwrite_behavior: &OverwriteBehavior,
    derived_keys: &DerivedKeys,
    job_id: Uuid,
) -> AppResult<()> {
    // ── Pre-restore integrity check ────────────────────────────
    info!("Starting pre-restore integrity check");
    // Fetch chunks for every selected file version and verify they exist in storage.
    let storage_type = match storage_config {
        RemoteStorageConfig::Aws(_) => "S3",
        RemoteStorageConfig::GoogleDrive(_) => "GoogleDrive",
        RemoteStorageConfig::OneDrive(_) => "OneDrive",
        RemoteStorageConfig::LocalFilesystem(_) => "LocalFilesystem",
        RemoteStorageConfig::Sftp(_) => "Sftp",
    };
    info!(storage_type, "Creating storage adaptor for integrity check");
    let storage = create_storage_port(
        storage_config,
        backup_config.storage_id,
        StorageLayout::BackupChunks,
    )
    .await?;
    info!("Storage adaptor created successfully");

    let mut file_chunks = Vec::with_capacity(file_selections.len());
    let mut total_chunks: usize = 0;

    for selection in file_selections {
        let chunks_response = env
            .chunk_api()
            .get_chunks_for_version(GetFileVersionChunksRequest {
                version_id: selection.version_id,
            })
            .await?;
        debug!(
            file_path = %selection.file_path,
            version_id = %selection.version_id,
            num_chunks = chunks_response.chunks.len(),
            "Fetched chunks for file version"
        );
        total_chunks += chunks_response.chunks.len();
        file_chunks.push((selection, chunks_response.chunks));
    }

    let _ = tx
        .send(RestoreResult::VerifyingChunks {
            total_files: file_selections.len(),
            total_chunks,
        })
        .await;

    let mut available: usize = 0;
    let mut missing: usize = 0;

    for (selection, chunks) in &file_chunks {
        for chunk_info in chunks {
            let key = match &chunk_info.storage_meta {
                ChunkStorageMeta::ObjectStore(s3) => ObjectKey::new(s3.key.clone()),
            };
            let exists = storage.exists(&key).await?;

            let _ = tx
                .send(RestoreResult::ChunkVerified {
                    file_path: selection.file_path.clone(),
                    chunk_index: chunk_info.chunk_index as u32,
                    available: exists,
                })
                .await;

            if exists {
                available += 1;
            } else {
                missing += 1;
            }
        }
    }

    let _ = tx
        .send(RestoreResult::IntegrityCheckComplete {
            total_chunks,
            available,
            missing,
        })
        .await;

    info!(total_chunks, available, missing, "Integrity check complete");

    if missing > 0 {
        return Err(crate::model::app_error::AppError::Internal {
            message: format!(
                "Pre-restore integrity check failed: {missing} of {total_chunks} chunks missing from storage"
            ),
            source: None,
        });
    }

    // ── Resolve destination writer ────────────────────────────
    let restore_target = match destination {
        RestoreDestination::RemoteStorage { storage_id } => {
            let destination_response = env
                .remote_storage_api()
                .get_by_id(GetRemoteStorageRequest { id: *storage_id })
                .await?;
            let encrypted_data: EncryptedData = destination_response.storage.config.try_into()?;
            let encryptor = AesGcmEncryptor::new(dek)?;
            let decrypted_config = encryptor.decrypt(&encrypted_data)?;
            let destination_config: RemoteStorageConfig =
                serde_json::from_slice(&decrypted_config)?;
            let destination_storage = create_storage_port(
                &destination_config,
                *storage_id,
                StorageLayout::RestoredFiles,
            )
            .await?;
            RestoreTarget::ConfiguredStorage {
                storage: destination_storage,
            }
        }
        _ => RestoreTarget::Local {
            base_dest: resolve_base_destination(destination),
        },
    };

    // ── Restore each file ──────────────────────────────────────
    info!(job_id = %job_id, "Starting file restores");
    let mut completed_files: usize = 0;
    let mut completed_bytes: u64 = 0;

    for (selection, chunks) in file_chunks {
        let file_size = selection.size as u64;
        let num_chunks = chunks.len() as u32;

        let _ = tx
            .send(RestoreResult::FileStarted {
                file_path: selection.file_path.clone(),
                file_size,
                version: selection.version,
            })
            .await;

        // Create restore job file entry (encrypted)
        let encrypted = encrypt_file_path(
            &selection.file_path,
            &derived_keys.metadata_key,
            &derived_keys.index_key,
        )?;
        let job_file_response = env
            .restore_job_api()
            .create_job_file(CreateRestoreJobFileRequest {
                job_id,
                remote_file_version_id: selection.version_id,
                encrypted_name: encrypted.encrypted_name,
                name_nonce: encrypted.nonce,
                blind_index: encrypted.blind_index,
                original_size: selection.size,
            })
            .await?;
        let job_file_id = job_file_response.id;

        if let RestoreTarget::ConfiguredStorage {
            storage: destination_storage,
        } = &restore_target
        {
            match restore_file_to_configured_storage(
                tx,
                storage.as_ref(),
                destination_storage.as_ref(),
                &chunks,
                selection,
                backup_config.user_id,
                &backup_config.source_directory,
                dek,
                overwrite_behavior,
            )
            .await
            {
                Ok(RemoteRestoreOutcome::Restored {
                    bytes_restored,
                    chunks_completed,
                    destination_key,
                }) => {
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job_file(CompleteRestoreJobFileRequest {
                            id: job_file_id,
                            restored_size: bytes_restored as i64,
                            total_chunks: chunks_completed as i32,
                            status: "completed".to_string(),
                            destination_path: Some(destination_key.as_str().to_string()),
                            error_message: None,
                        })
                        .await
                    {
                        error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark restore job file as completed");
                    }

                    completed_files += 1;
                    completed_bytes += bytes_restored;
                    let _ = tx
                        .send(RestoreResult::FileCompleted {
                            file_path: selection.file_path.clone(),
                            total_chunks: num_chunks,
                            restored_bytes: bytes_restored,
                        })
                        .await;
                }
                Ok(RemoteRestoreOutcome::Skipped) => {
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job_file(CompleteRestoreJobFileRequest {
                            id: job_file_id,
                            restored_size: 0,
                            total_chunks: 0,
                            status: "skipped".to_string(),
                            destination_path: remote_restore_key(
                                backup_config.user_id,
                                &selection.file_path,
                                &backup_config.source_directory,
                            )
                            .map(|key| key.as_str().to_string())
                            .ok(),
                            error_message: None,
                        })
                        .await
                    {
                        error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark restore job file as skipped");
                    }
                    completed_files += 1;
                    let _ = tx
                        .send(RestoreResult::FileSkipped {
                            file_path: selection.file_path.clone(),
                        })
                        .await;
                }
                Err(e) => {
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job_file(CompleteRestoreJobFileRequest {
                            id: job_file_id,
                            restored_size: 0,
                            total_chunks: 0,
                            status: "failed".to_string(),
                            destination_path: None,
                            error_message: Some(e.to_string()),
                        })
                        .await
                    {
                        error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark restore job file as failed");
                    }
                    let _ = tx
                        .send(RestoreResult::Failed {
                            reason: e.to_string(),
                            file_path: Some(selection.file_path.clone()),
                        })
                        .await;
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job(CompleteRestoreJobRequest {
                            id: job_id,
                            status: "failed".to_string(),
                            error_message: Some(e.to_string()),
                        })
                        .await
                    {
                        error!(job_id = %job_id, error = %api_err, "Failed to mark restore job as failed");
                    }
                    return Ok(());
                }
            }
            continue;
        }

        let RestoreTarget::Local { base_dest } = &restore_target else {
            unreachable!("configured storage restore handled above");
        };

        // Determine destination path for this file
        let dest_path = resolve_file_destination(
            base_dest,
            &selection.file_path,
            &backup_config.source_directory,
            destination,
            overwrite_behavior,
        );
        let dest_path_string = dest_path.to_string_lossy().to_string();

        // Build a new source storage client for this file so the spawned local
        // restore task can own it.
        let file_storage = create_storage_port(
            storage_config,
            backup_config.storage_id,
            StorageLayout::BackupChunks,
        )
        .await?;

        let mut chunk_rx = restore_file(file_storage, chunks, dest_path, dek.clone());

        let mut bytes_restored: u64 = 0;
        let mut chunks_completed: u32 = 0;

        let mut file_failed = false;
        while let Some(result) = chunk_rx.recv().await {
            match result {
                Ok(chunk_meta) => {
                    bytes_restored += chunk_meta.size as u64;
                    chunks_completed += 1;

                    let _ = tx
                        .send(RestoreResult::ChunkDownloaded {
                            file_path: selection.file_path.clone(),
                            chunk_index: chunk_meta.index as u32,
                            chunk_size: chunk_meta.size,
                            bytes_restored,
                            file_size,
                        })
                        .await;
                }
                Err(e) => {
                    // Complete the job file as failed
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job_file(CompleteRestoreJobFileRequest {
                            id: job_file_id,
                            restored_size: bytes_restored as i64,
                            total_chunks: chunks_completed as i32,
                            status: "failed".to_string(),
                            destination_path: Some(dest_path_string.clone()),
                            error_message: Some(e.to_string()),
                        })
                        .await
                    {
                        error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark restore job file as failed");
                    }

                    if tx
                        .send(RestoreResult::Failed {
                            reason: e.to_string(),
                            file_path: Some(selection.file_path.clone()),
                        })
                        .await
                        .is_err()
                    {
                        error!(file_path = %selection.file_path, error = %e, "Restore file failed and result channel closed");
                    }

                    // Complete the entire job as failed
                    if let Err(api_err) = env
                        .restore_job_api()
                        .complete_job(CompleteRestoreJobRequest {
                            id: job_id,
                            status: "failed".to_string(),
                            error_message: Some(e.to_string()),
                        })
                        .await
                    {
                        error!(job_id = %job_id, error = %api_err, "Failed to mark restore job as failed");
                    }

                    file_failed = true;
                    break;
                }
            }
        }

        if file_failed {
            return Ok(());
        }

        // Complete the job file as succeeded
        if let Err(api_err) = env
            .restore_job_api()
            .complete_job_file(CompleteRestoreJobFileRequest {
                id: job_file_id,
                restored_size: bytes_restored as i64,
                total_chunks: num_chunks as i32,
                status: "completed".to_string(),
                destination_path: Some(dest_path_string),
                error_message: None,
            })
            .await
        {
            error!(job_file_id = %job_file_id, error = %api_err, "Failed to mark restore job file as completed");
        }

        completed_files += 1;
        completed_bytes += bytes_restored;
        info!(file_path = %selection.file_path, bytes_restored, "File restore completed");

        let _ = tx
            .send(RestoreResult::FileCompleted {
                file_path: selection.file_path.clone(),
                total_chunks: num_chunks,
                restored_bytes: bytes_restored,
            })
            .await;
    }

    // ── 6. Complete restore job ───────────────────────────────────
    info!(job_id = %job_id, completed_files, completed_bytes, "Restore job completed");
    if let Err(api_err) = env
        .restore_job_api()
        .complete_job(CompleteRestoreJobRequest {
            id: job_id,
            status: "completed".to_string(),
            error_message: None,
        })
        .await
    {
        error!(job_id = %job_id, error = %api_err, "Failed to mark restore job as completed");
    }

    let _ = tx
        .send(RestoreResult::Completed {
            total_files: completed_files,
            total_bytes: completed_bytes,
        })
        .await;

    Ok(())
}

/// Converts a restore destination into metadata persisted with the restore job.
fn restore_destination_metadata(
    destination: &RestoreDestination,
    user_id: Uuid,
) -> (String, Option<Uuid>, Option<String>) {
    match destination {
        RestoreDestination::OriginalPath => ("original_path".to_string(), None, None),
        RestoreDestination::DownloadFolder => ("download_folder".to_string(), None, None),
        RestoreDestination::CustomPath(path) => {
            ("custom_path".to_string(), None, Some(path.clone()))
        }
        RestoreDestination::RemoteStorage { storage_id } => (
            "configured_storage".to_string(),
            Some(*storage_id),
            Some(format!("cloudless-restored/{user_id}")),
        ),
    }
}

/// Restores one file directly into configured storage after resolving remote
/// conflict behavior against the destination key.
async fn restore_file_to_configured_storage(
    tx: &mpsc::Sender<RestoreResult>,
    source_storage: &dyn StoragePort,
    destination_storage: &dyn StoragePort,
    chunks: &[RestoreChunkInfo],
    selection: &RestoreFileSelection,
    user_id: Uuid,
    source_directory: &str,
    dek: &Dek,
    overwrite_behavior: &OverwriteBehavior,
) -> AppResult<RemoteRestoreOutcome> {
    let desired_key = remote_restore_key(user_id, &selection.file_path, source_directory)?;
    let decision =
        resolve_remote_conflict_key(destination_storage, &desired_key, overwrite_behavior).await?;

    let RemoteConflictDecision::Write(destination_key) = decision else {
        return Ok(RemoteRestoreOutcome::Skipped);
    };

    let encryptor = AesGcmEncryptor::new(dek)?;
    let compressor = ZstdCompressor::new(4);
    let file_size = selection.size as u64;
    let mut file_data = Vec::new();
    let mut bytes_restored = 0_u64;
    let mut chunks_completed = 0_u32;

    for chunk_info in chunks {
        let (decompressed_data, chunk_meta) =
            restore_single_chunk(source_storage, &encryptor, &compressor, chunk_info).await?;
        bytes_restored += chunk_meta.size as u64;
        chunks_completed += 1;
        file_data.extend_from_slice(&decompressed_data);

        let _ = tx
            .send(RestoreResult::ChunkDownloaded {
                file_path: selection.file_path.clone(),
                chunk_index: chunk_meta.index as u32,
                chunk_size: chunk_meta.size,
                bytes_restored,
                file_size,
            })
            .await;
    }

    destination_storage.put(&destination_key, file_data).await?;

    Ok(RemoteRestoreOutcome::Restored {
        bytes_restored,
        chunks_completed,
        destination_key,
    })
}

/// Builds the configured-storage restore key under
/// `cloudless-restored/{user_id}/` while preserving the source-relative path.
fn remote_restore_key(
    user_id: Uuid,
    original_file_path: &str,
    source_directory: &str,
) -> AppResult<ObjectKey> {
    let relative = original_file_path
        .strip_prefix(source_directory)
        .unwrap_or(original_file_path)
        .trim_start_matches(['/', '\\']);
    let normalized = relative.replace('\\', "/");
    validate_relative_restore_path(&normalized)?;
    Ok(ObjectKey::new(format!(
        "cloudless-restored/{user_id}/{normalized}"
    )))
}

/// Rejects ambiguous or unsafe destination paths before they reach storage.
fn validate_relative_restore_path(path: &str) -> AppResult<()> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err(AppError::Validation {
            message: "restore destination path must be relative".to_string(),
            source: None,
        });
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\0') {
            return Err(AppError::Validation {
                message: format!("unsafe restore destination path segment: {segment}"),
                source: None,
            });
        }
    }
    Ok(())
}

/// Applies remote overwrite behavior using the destination storage `exists`
/// operation as the only conflict check.
async fn resolve_remote_conflict_key(
    destination_storage: &dyn StoragePort,
    desired_key: &ObjectKey,
    overwrite_behavior: &OverwriteBehavior,
) -> AppResult<RemoteConflictDecision> {
    match overwrite_behavior {
        OverwriteBehavior::Overwrite => Ok(RemoteConflictDecision::Write(desired_key.clone())),
        OverwriteBehavior::SkipIfExists => {
            if destination_storage.exists(desired_key).await? {
                Ok(RemoteConflictDecision::Skip)
            } else {
                Ok(RemoteConflictDecision::Write(desired_key.clone()))
            }
        }
        OverwriteBehavior::KeepBoth => {
            if !destination_storage.exists(desired_key).await? {
                return Ok(RemoteConflictDecision::Write(desired_key.clone()));
            }

            const MAX_KEEP_BOTH_ATTEMPTS: u32 = 10_000;
            for counter in 1..=MAX_KEEP_BOTH_ATTEMPTS {
                let candidate = with_keep_both_suffix(desired_key, counter);
                if !destination_storage.exists(&candidate).await? {
                    return Ok(RemoteConflictDecision::Write(candidate));
                }
            }

            Ok(RemoteConflictDecision::Write(ObjectKey::new(format!(
                "{} ({})",
                desired_key.as_str(),
                Uuid::now_v7()
            ))))
        }
    }
}

/// Adds a ` (counter)` suffix before the file extension for keep-both restore.
fn with_keep_both_suffix(key: &ObjectKey, counter: u32) -> ObjectKey {
    let raw = key.as_str();
    let (parent, name) = raw.rsplit_once('/').unwrap_or(("", raw));
    let (stem, ext) = name
        .rsplit_once('.')
        .map(|(s, e)| (s.to_string(), format!(".{e}")))
        .unwrap_or_else(|| (name.to_string(), String::new()));
    let renamed = format!("{stem} ({counter}){ext}");
    if parent.is_empty() {
        ObjectKey::new(renamed)
    } else {
        ObjectKey::new(format!("{parent}/{renamed}"))
    }
}

/// Resolves the base destination directory from the user's chosen `RestoreDestination`.
fn resolve_base_destination(destination: &RestoreDestination) -> PathBuf {
    match destination {
        RestoreDestination::OriginalPath => PathBuf::from("/"),
        RestoreDestination::DownloadFolder => dirs::download_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("Downloads")
        }),
        RestoreDestination::CustomPath(path) => PathBuf::from(path),
        RestoreDestination::RemoteStorage { .. } => PathBuf::from("/"),
    }
}

/// Determines the final file path, preserving folder structure relative to
/// the backup source directory, and applying overwrite behavior.
fn resolve_file_destination(
    base_dest: &PathBuf,
    original_file_path: &str,
    source_directory: &str,
    destination: &RestoreDestination,
    overwrite_behavior: &OverwriteBehavior,
) -> PathBuf {
    let dest_path = match destination {
        // For OriginalPath, write back to the exact original location.
        RestoreDestination::OriginalPath => PathBuf::from(original_file_path),
        // For DownloadFolder or CustomPath, preserve the relative path from the source directory.
        _ => {
            let relative = original_file_path
                .strip_prefix(source_directory)
                .unwrap_or(original_file_path)
                .trim_start_matches('/');
            base_dest.join(relative)
        }
    };

    match overwrite_behavior {
        OverwriteBehavior::Overwrite | OverwriteBehavior::SkipIfExists => dest_path,
        OverwriteBehavior::KeepBoth => {
            if dest_path.exists() {
                // Generate a unique name by appending a counter: file (1).ext, file (2).ext, ...
                let stem = dest_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let ext = dest_path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                let parent = dest_path.parent().unwrap_or(base_dest);

                const MAX_KEEP_BOTH_ATTEMPTS: u32 = 10_000;
                for counter in 1..=MAX_KEEP_BOTH_ATTEMPTS {
                    let candidate = parent.join(format!("{stem} ({counter}){ext}"));
                    if !candidate.exists() {
                        return candidate;
                    }
                }
                // Exhausted all attempts — fall back to a UUID-suffixed name
                let fallback = parent.join(format!("{stem} ({}){ext}", Uuid::now_v7()));
                fallback
            } else {
                dest_path
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::storage::StoragePort;
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::Mutex;

    /// Minimal destination storage mock for testing remote restore conflict
    /// behavior without touching a real provider.
    struct MockDestinationStorage {
        existing: Mutex<HashSet<String>>,
    }

    impl MockDestinationStorage {
        /// Creates a mock storage with the provided keys marked as existing.
        fn with_existing(keys: &[&str]) -> Self {
            Self {
                existing: Mutex::new(keys.iter().map(|k| k.to_string()).collect()),
            }
        }
    }

    #[async_trait]
    impl StoragePort for MockDestinationStorage {
        async fn put(&self, key: &ObjectKey, _data: Vec<u8>) -> AppResult<()> {
            self.existing
                .lock()
                .unwrap()
                .insert(key.as_str().to_string());
            Ok(())
        }

        async fn get(&self, _key: &ObjectKey) -> AppResult<Vec<u8>> {
            unimplemented!()
        }

        async fn delete(&self, _key: &ObjectKey) -> AppResult<()> {
            unimplemented!()
        }

        async fn list(&self, _prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
            unimplemented!()
        }

        async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
            Ok(self.existing.lock().unwrap().contains(key.as_str()))
        }
    }

    // --- Tests: resolve_base_destination ---

    /// Tests that OriginalPath resolves to root ("/").
    #[test]
    fn test_resolve_base_destination_original_path() {
        let dest = resolve_base_destination(&RestoreDestination::OriginalPath);
        assert_eq!(dest, PathBuf::from("/"));
    }

    /// Tests that DownloadFolder resolves to a Downloads directory.
    #[test]
    fn test_resolve_base_destination_download_folder() {
        let dest = resolve_base_destination(&RestoreDestination::DownloadFolder);
        // Should end with "Downloads" regardless of platform home dir
        let path_str = dest.to_string_lossy();
        assert!(
            path_str.ends_with("Downloads"),
            "Expected path ending with 'Downloads', got: {path_str}"
        );
    }

    /// Tests that CustomPath resolves to the exact path provided.
    #[test]
    fn test_resolve_base_destination_custom_path() {
        let dest = resolve_base_destination(&RestoreDestination::CustomPath("/tmp/restore".into()));
        assert_eq!(dest, PathBuf::from("/tmp/restore"));
    }

    // --- Tests: resolve_file_destination ---

    /// Tests that OriginalPath restores to the exact original file path.
    #[test]
    fn test_resolve_file_destination_original_path() {
        let base = PathBuf::from("/");
        let dest = resolve_file_destination(
            &base,
            "/home/user/docs/file.txt",
            "/home/user/docs",
            &RestoreDestination::OriginalPath,
            &OverwriteBehavior::Overwrite,
        );
        assert_eq!(dest, PathBuf::from("/home/user/docs/file.txt"));
    }

    /// Tests that CustomPath preserves relative structure from source directory.
    #[test]
    fn test_resolve_file_destination_custom_path_preserves_relative() {
        let base = PathBuf::from("/tmp/restore");
        let dest = resolve_file_destination(
            &base,
            "/home/user/docs/subdir/file.txt",
            "/home/user/docs",
            &RestoreDestination::CustomPath("/tmp/restore".into()),
            &OverwriteBehavior::Overwrite,
        );
        assert_eq!(dest, PathBuf::from("/tmp/restore/subdir/file.txt"));
    }

    /// Tests that DownloadFolder preserves relative structure from source directory.
    #[test]
    fn test_resolve_file_destination_download_folder_relative() {
        let base = PathBuf::from("/Users/test/Downloads");
        let dest = resolve_file_destination(
            &base,
            "/backup/source/deep/nested/file.rs",
            "/backup/source",
            &RestoreDestination::DownloadFolder,
            &OverwriteBehavior::SkipIfExists,
        );
        assert_eq!(
            dest,
            PathBuf::from("/Users/test/Downloads/deep/nested/file.rs")
        );
    }

    /// Tests that OverwriteBehavior::Overwrite returns the path as-is (no renaming).
    #[test]
    fn test_resolve_file_destination_overwrite_returns_same_path() {
        let base = PathBuf::from("/tmp/restore");
        let dest = resolve_file_destination(
            &base,
            "/src/main.rs",
            "/src",
            &RestoreDestination::CustomPath("/tmp/restore".into()),
            &OverwriteBehavior::Overwrite,
        );
        assert_eq!(dest, PathBuf::from("/tmp/restore/main.rs"));
    }

    /// Tests that SkipIfExists returns the path as-is (caller decides whether to skip).
    #[test]
    fn test_resolve_file_destination_skip_if_exists_returns_same_path() {
        let base = PathBuf::from("/tmp/restore");
        let dest = resolve_file_destination(
            &base,
            "/src/lib.rs",
            "/src",
            &RestoreDestination::CustomPath("/tmp/restore".into()),
            &OverwriteBehavior::SkipIfExists,
        );
        assert_eq!(dest, PathBuf::from("/tmp/restore/lib.rs"));
    }

    /// Tests that KeepBoth returns the original path when the file does not exist.
    #[test]
    fn test_resolve_file_destination_keep_both_no_conflict() {
        let base = PathBuf::from("/tmp/restore-nonexistent-dir-xyz");
        let dest = resolve_file_destination(
            &base,
            "/src/file.txt",
            "/src",
            &RestoreDestination::CustomPath("/tmp/restore-nonexistent-dir-xyz".into()),
            &OverwriteBehavior::KeepBoth,
        );
        // File doesn't exist, so path is returned as-is
        assert_eq!(
            dest,
            PathBuf::from("/tmp/restore-nonexistent-dir-xyz/file.txt")
        );
    }

    /// Tests that files outside the source directory still get a reasonable path.
    #[test]
    fn test_resolve_file_destination_file_outside_source() {
        let base = PathBuf::from("/tmp/restore");
        let dest = resolve_file_destination(
            &base,
            "/other/path/file.txt",
            "/home/user/docs",
            &RestoreDestination::CustomPath("/tmp/restore".into()),
            &OverwriteBehavior::Overwrite,
        );
        // File path doesn't start with source_directory, so full path is used as relative
        assert_eq!(dest, PathBuf::from("/tmp/restore/other/path/file.txt"));
    }

    /// Tests RestoreFileSelection struct construction and field access.
    #[test]
    fn test_restore_file_selection_construction() {
        let selection = RestoreFileSelection {
            version_id: Uuid::now_v7(),
            file_path: "/home/user/photo.jpg".into(),
            size: 5_000_000,
            version: 3,
        };
        assert_eq!(selection.file_path, "/home/user/photo.jpg");
        assert_eq!(selection.size, 5_000_000);
        assert_eq!(selection.version, 3);
    }

    /// Tests that configured-storage restore keys are user-scoped and preserve
    /// the source-relative path under `cloudless-restored`.
    #[test]
    fn remote_restore_key_uses_cloudless_restored_user_prefix() {
        let user_id = Uuid::now_v7();

        let key = remote_restore_key(
            user_id,
            "/Users/test/Documents/tax/2025.pdf",
            "/Users/test/Documents",
        )
        .unwrap();

        assert_eq!(
            key.as_str(),
            format!("cloudless-restored/{user_id}/tax/2025.pdf")
        );
    }

    /// Tests that paths outside the configured source directory still become a
    /// safe relative key under the user-scoped restore prefix.
    #[test]
    fn remote_restore_key_handles_paths_outside_source_directory() {
        let user_id = Uuid::now_v7();

        let key =
            remote_restore_key(user_id, "/other/path/file.txt", "/Users/test/Documents").unwrap();

        assert_eq!(
            key.as_str(),
            format!("cloudless-restored/{user_id}/other/path/file.txt")
        );
    }

    /// Tests that traversal-like relative paths are rejected before storage
    /// adapters see them.
    #[test]
    fn remote_restore_key_rejects_parent_segments() {
        let user_id = Uuid::now_v7();

        let result = remote_restore_key(
            user_id,
            "/Users/test/Documents/../secret.txt",
            "/Users/test/Documents",
        );

        assert!(result.is_err());
    }

    /// Tests that keep-both conflict handling appends the counter before the
    /// extension using destination `exists` checks.
    #[tokio::test]
    async fn resolve_remote_conflict_key_keep_both_renames_before_extension() {
        let storage = MockDestinationStorage::with_existing(&[
            "cloudless-restored/user/report.pdf",
            "cloudless-restored/user/report (1).pdf",
        ]);
        let desired = ObjectKey::new("cloudless-restored/user/report.pdf".to_string());

        let decision =
            resolve_remote_conflict_key(&storage, &desired, &OverwriteBehavior::KeepBoth)
                .await
                .unwrap();

        match decision {
            RemoteConflictDecision::Write(key) => {
                assert_eq!(key.as_str(), "cloudless-restored/user/report (2).pdf");
            }
            RemoteConflictDecision::Skip => panic!("keep-both should not skip"),
        }
    }

    /// Tests that skip-if-exists returns a skip decision without choosing a new
    /// key when the destination object already exists.
    #[tokio::test]
    async fn resolve_remote_conflict_key_skip_existing_returns_skip() {
        let storage = MockDestinationStorage::with_existing(&["cloudless-restored/user/file.txt"]);
        let desired = ObjectKey::new("cloudless-restored/user/file.txt".to_string());

        let decision =
            resolve_remote_conflict_key(&storage, &desired, &OverwriteBehavior::SkipIfExists)
                .await
                .unwrap();

        assert!(matches!(decision, RemoteConflictDecision::Skip));
    }
}
