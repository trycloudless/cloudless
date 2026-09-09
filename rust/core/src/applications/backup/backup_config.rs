/// Encrypt/decrypt boundary for backup config and job types.
///
/// Converts between encrypted `api_types` server wire format types and
/// `Decrypted*` types (used by core logic, Tauri, and UI).
use api_types::backup_config::{
    BackupConfig, BackupConfigWithRemoteStorage, BackupExclusionConfig, CleanupType,
    CreateBackupConfigRequest, DecryptedBackupConfig, DecryptedBackupConfigWithRemoteStorage,
    UpdateExclusionConfigRequest,
};
use api_types::backup_job::{
    BackupJobFileSummary, CreateBackupJobFileRequest, DecryptedBackupJobDetail,
    DecryptedBackupJobDetailResponse, DecryptedBackupJobFileSummary, GetBackupJobDetailResponse,
};
use api_types::restore_job::{
    DecryptedRestoreJobDetail, DecryptedRestoreJobDetailResponse, DecryptedRestoreJobFileSummary,
    GetRestoreJobDetailResponse, RestoreJobFileSummary,
};
use uuid::Uuid;

use crate::{
    domain::{
        derived_keys::DerivedKeys,
        metadata_crypto::{
            EncryptedMetadata, decrypt_bytes, decrypt_file_path, encrypt_bytes, encrypt_file_path,
        },
    },
    model::base::AppResult,
};

// ── Encrypt (before server write) ──────────────────────────────────

/// Serialises and encrypts a `BackupExclusionConfig` with the metadata key.
fn encrypt_exclusion_config(
    config: &BackupExclusionConfig,
    metadata_key: &[u8; 32],
) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let json = serde_json::to_vec(config)?;
    encrypt_bytes(&json, metadata_key)
}

/// Decrypts and deserialises a `BackupExclusionConfig` from the server fields.
/// Returns `BackupExclusionConfig::default()` when either field is `None` (backwards compat).
fn decrypt_exclusion_config(
    encrypted: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
    metadata_key: &[u8; 32],
) -> AppResult<BackupExclusionConfig> {
    match (encrypted, nonce) {
        (Some(enc), Some(n)) => {
            let plaintext = decrypt_bytes(&enc, &n, metadata_key)?;
            Ok(serde_json::from_slice(&plaintext)?)
        }
        _ => Ok(BackupExclusionConfig::default()),
    }
}

/// Encrypts source directory and exclusion config, then builds a
/// `CreateBackupConfigRequest` for the server.
pub fn encrypt_create_config_request(
    source_directory: &str,
    storage_id: Uuid,
    display_name: String,
    local_device_id: Uuid,
    cleanup_type: CleanupType,
    exclusion_config: &BackupExclusionConfig,
    derived_keys: &DerivedKeys,
) -> AppResult<CreateBackupConfigRequest> {
    let encrypted_source = encrypt_file_path(
        source_directory,
        &derived_keys.metadata_key,
        &derived_keys.index_key,
    )?;
    let (encrypted_exclusion, exclusion_nonce) =
        encrypt_exclusion_config(exclusion_config, &derived_keys.metadata_key)?;

    Ok(CreateBackupConfigRequest {
        storage_id,
        display_name,
        local_device_id,
        encrypted_source_dir: encrypted_source.encrypted_name,
        source_dir_nonce: encrypted_source.nonce,
        source_dir_blind_index: encrypted_source.blind_index,
        cleanup_type,
        encrypted_exclusion_config: Some(encrypted_exclusion),
        exclusion_config_nonce: Some(exclusion_nonce),
    })
}

/// Builds an `UpdateExclusionConfigRequest` with the exclusion config encrypted.
pub fn encrypt_update_exclusion_config_request(
    id: Uuid,
    exclusion_config: &BackupExclusionConfig,
    derived_keys: &DerivedKeys,
) -> AppResult<UpdateExclusionConfigRequest> {
    let (encrypted, nonce) =
        encrypt_exclusion_config(exclusion_config, &derived_keys.metadata_key)?;
    Ok(UpdateExclusionConfigRequest {
        id,
        encrypted_exclusion_config: encrypted,
        exclusion_config_nonce: nonce,
    })
}

// ── Decrypt (after server read) ────────────────────────────────────

/// Decrypts a `BackupConfig` from the server into a `DecryptedBackupConfig`.
pub fn decrypt_backup_config(
    config: BackupConfig,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedBackupConfig> {
    let encrypted = EncryptedMetadata {
        encrypted_name: config.encrypted_source_dir,
        nonce: config.source_dir_nonce,
        blind_index: vec![],
    };
    let source_directory = decrypt_file_path(&encrypted, metadata_key)?;
    let exclusion_config = decrypt_exclusion_config(
        config.encrypted_exclusion_config,
        config.exclusion_config_nonce,
        metadata_key,
    )?;
    Ok(DecryptedBackupConfig {
        id: config.id,
        user_id: config.user_id,
        storage_id: config.storage_id,
        local_device_id: config.local_device_id,
        source_directory,
        created_at: config.created_at,
        cleanup_type: config.cleanup_type,
        exclusion_config,
    })
}

/// Decrypts a `BackupConfigWithRemoteStorage` from the server.
pub fn decrypt_backup_config_with_storage(
    config: BackupConfigWithRemoteStorage,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedBackupConfigWithRemoteStorage> {
    let encrypted = EncryptedMetadata {
        encrypted_name: config.encrypted_source_dir,
        nonce: config.source_dir_nonce,
        blind_index: vec![],
    };
    let source_directory = decrypt_file_path(&encrypted, metadata_key)?;
    let exclusion_config = decrypt_exclusion_config(
        config.encrypted_exclusion_config,
        config.exclusion_config_nonce,
        metadata_key,
    )?;
    Ok(DecryptedBackupConfigWithRemoteStorage {
        storage_id: config.storage_id,
        source_directory,
        config_id: config.config_id,
        display_name: config.display_name,
        storage_type: config.storage_type,
        config: config.config,
        local_device_id: config.local_device_id,
        physical_device_id: config.physical_device_id,
        is_active: config.is_active,
        cleanup_type: config.cleanup_type,
        exclusion_config,
    })
}

// ── Backup job file encrypt/decrypt ─────────────────────────────────

/// Encrypts a file path and builds a `CreateBackupJobFileRequest` for the server.
pub fn encrypt_create_job_file_request(
    job_id: Uuid,
    file_path: &str,
    original_size: i64,
    derived_keys: &DerivedKeys,
) -> AppResult<CreateBackupJobFileRequest> {
    let encrypted = encrypt_file_path(
        file_path,
        &derived_keys.metadata_key,
        &derived_keys.index_key,
    )?;
    Ok(CreateBackupJobFileRequest {
        job_id,
        encrypted_name: encrypted.encrypted_name,
        name_nonce: encrypted.nonce,
        blind_index: encrypted.blind_index,
        original_size,
    })
}

/// Decrypts a single `BackupJobFileSummary` into a `DecryptedBackupJobFileSummary`.
fn decrypt_job_file_summary(
    file: BackupJobFileSummary,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedBackupJobFileSummary> {
    let encrypted = EncryptedMetadata {
        encrypted_name: file.encrypted_name,
        nonce: file.name_nonce,
        blind_index: vec![],
    };
    let file_path = decrypt_file_path(&encrypted, metadata_key)?;
    Ok(DecryptedBackupJobFileSummary {
        id: file.id,
        file_path,
        original_size: file.original_size,
        uploaded_size: file.uploaded_size,
        deduplicated_size: file.deduplicated_size,
        total_chunks: file.total_chunks,
        deduplicated_chunks: file.deduplicated_chunks,
        status: file.status,
        error_message: file.error_message,
    })
}

/// Decrypts a `GetBackupJobDetailResponse` into a `DecryptedBackupJobDetailResponse`.
pub fn decrypt_backup_job_detail(
    response: GetBackupJobDetailResponse,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedBackupJobDetailResponse> {
    let job = response.job;
    let files = job
        .files
        .into_iter()
        .map(|f| decrypt_job_file_summary(f, metadata_key))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(DecryptedBackupJobDetailResponse {
        job: DecryptedBackupJobDetail {
            id: job.id,
            backup_config_id: job.backup_config_id,
            status: job.status,
            total_files: job.total_files,
            total_files_succeeded: job.total_files_succeeded,
            total_files_failed: job.total_files_failed,
            original_bytes: job.original_bytes,
            uploaded_bytes: job.uploaded_bytes,
            deduplicated_bytes: job.deduplicated_bytes,
            deduplicated_chunks: job.deduplicated_chunks,
            error_message: job.error_message,
            started_at: job.started_at,
            completed_at: job.completed_at,
            files,
        },
    })
}

// ── Restore job file decrypt ───────────────────────────────────────

/// Decrypts a single `RestoreJobFileSummary` into a `DecryptedRestoreJobFileSummary`.
fn decrypt_restore_job_file_summary(
    file: RestoreJobFileSummary,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedRestoreJobFileSummary> {
    let encrypted = EncryptedMetadata {
        encrypted_name: file.encrypted_name,
        nonce: file.name_nonce,
        blind_index: vec![],
    };
    let file_path = decrypt_file_path(&encrypted, metadata_key)?;
    Ok(DecryptedRestoreJobFileSummary {
        id: file.id,
        file_path,
        destination_path: file.destination_path,
        original_size: file.original_size,
        restored_size: file.restored_size,
        total_chunks: file.total_chunks,
        chunks_completed: file.chunks_completed,
        status: file.status,
        error_message: file.error_message,
    })
}

/// Decrypts a `GetRestoreJobDetailResponse` into a `DecryptedRestoreJobDetailResponse`.
pub fn decrypt_restore_job_detail(
    response: GetRestoreJobDetailResponse,
    metadata_key: &[u8; 32],
) -> AppResult<DecryptedRestoreJobDetailResponse> {
    let job = response.job;
    let files = job
        .files
        .into_iter()
        .map(|f| decrypt_restore_job_file_summary(f, metadata_key))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(DecryptedRestoreJobDetailResponse {
        job: DecryptedRestoreJobDetail {
            id: job.id,
            backup_config_id: job.backup_config_id,
            overwrite_behavior: job.overwrite_behavior,
            destination_type: job.destination_type,
            destination_storage_id: job.destination_storage_id,
            destination_prefix: job.destination_prefix,
            status: job.status,
            total_files: job.total_files,
            total_files_succeeded: job.total_files_succeeded,
            total_files_failed: job.total_files_failed,
            original_bytes: job.original_bytes,
            restored_bytes: job.restored_bytes,
            error_message: job.error_message,
            started_at: job.started_at,
            completed_at: job.completed_at,
            files,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::backup_config::{
        BackupExclusionEntry, BackupExclusionPreset, BackupExclusionRule, BackupExclusionRuleKind,
    };
    use zeroize::Zeroizing;

    use crate::domain::dek::Dek;

    fn test_keys() -> DerivedKeys {
        let dek = Dek {
            key: Zeroizing::new([42u8; 32]),
        };
        DerivedKeys::derive(&dek).unwrap()
    }

    fn non_empty_exclusion_config() -> BackupExclusionConfig {
        BackupExclusionConfig {
            entries: vec![
                BackupExclusionEntry::Preset(BackupExclusionPreset::OsMetadata),
                BackupExclusionEntry::Custom(BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: "dmg".to_string(),
                }),
                BackupExclusionEntry::Glob("**/node_modules/**".to_string()),
            ],
        }
    }

    #[test]
    fn encrypt_create_config_request_round_trip() {
        let keys = test_keys();
        let source_dir = "/Users/test/Documents";
        let storage_id = Uuid::now_v7();
        let device_id = Uuid::now_v7();

        let request = encrypt_create_config_request(
            source_dir,
            storage_id,
            "Test Config".to_string(),
            device_id,
            Default::default(),
            &BackupExclusionConfig::default(),
            &keys,
        )
        .unwrap();

        assert_eq!(request.storage_id, storage_id);
        assert_eq!(request.local_device_id, device_id);
        assert_eq!(request.display_name, "Test Config");
        assert!(!request.encrypted_source_dir.is_empty());
        assert_eq!(request.source_dir_nonce.len(), 12);
        assert_eq!(request.source_dir_blind_index.len(), 32);
        assert!(request.encrypted_exclusion_config.is_some());
        assert!(request.exclusion_config_nonce.is_some());
    }

    #[test]
    fn decrypt_backup_config_round_trip() {
        let keys = test_keys();
        let source_dir = "/Users/test/Documents";
        let storage_id = Uuid::now_v7();
        let device_id = Uuid::now_v7();
        let config_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let exclusion_config = non_empty_exclusion_config();

        let request = encrypt_create_config_request(
            source_dir,
            storage_id,
            "Test Config".to_string(),
            device_id,
            Default::default(),
            &exclusion_config,
            &keys,
        )
        .unwrap();

        let server_config = BackupConfig {
            id: config_id,
            user_id,
            storage_id: request.storage_id,
            local_device_id: request.local_device_id,
            encrypted_source_dir: request.encrypted_source_dir,
            source_dir_nonce: request.source_dir_nonce,
            source_dir_blind_index: request.source_dir_blind_index,
            created_at: None,
            cleanup_type: Default::default(),
            encrypted_exclusion_config: request.encrypted_exclusion_config,
            exclusion_config_nonce: request.exclusion_config_nonce,
        };

        let decrypted = decrypt_backup_config(server_config, &keys.metadata_key).unwrap();
        assert_eq!(decrypted.source_directory, source_dir);
        assert_eq!(decrypted.id, config_id);
        assert_eq!(decrypted.storage_id, storage_id);
        assert_eq!(decrypted.exclusion_config, exclusion_config);
    }

    #[test]
    fn encrypt_decrypt_exclusion_config_round_trip() {
        let keys = test_keys();
        let config = non_empty_exclusion_config();

        let (enc, nonce) = encrypt_exclusion_config(&config, &keys.metadata_key).unwrap();
        let decoded = decrypt_exclusion_config(Some(enc), Some(nonce), &keys.metadata_key).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn decrypt_exclusion_config_missing_fields_defaults() {
        let keys = test_keys();
        let result = decrypt_exclusion_config(None, None, &keys.metadata_key).unwrap();
        assert_eq!(result, BackupExclusionConfig::default());
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let keys = test_keys();
        let wrong_key = [0u8; 32];

        let request = encrypt_create_config_request(
            "/test/path",
            Uuid::now_v7(),
            "Config".to_string(),
            Uuid::now_v7(),
            Default::default(),
            &BackupExclusionConfig::default(),
            &keys,
        )
        .unwrap();

        let server_config = BackupConfig {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            storage_id: request.storage_id,
            local_device_id: request.local_device_id,
            encrypted_source_dir: request.encrypted_source_dir,
            source_dir_nonce: request.source_dir_nonce,
            source_dir_blind_index: request.source_dir_blind_index,
            cleanup_type: Default::default(),
            created_at: None,
            encrypted_exclusion_config: request.encrypted_exclusion_config,
            exclusion_config_nonce: request.exclusion_config_nonce,
        };

        let result = decrypt_backup_config(server_config, &wrong_key);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_update_exclusion_config_request_round_trip() {
        let keys = test_keys();
        let id = Uuid::now_v7();
        let config = non_empty_exclusion_config();

        let req = encrypt_update_exclusion_config_request(id, &config, &keys).unwrap();
        assert_eq!(req.id, id);

        let decoded = decrypt_exclusion_config(
            Some(req.encrypted_exclusion_config),
            Some(req.exclusion_config_nonce),
            &keys.metadata_key,
        )
        .unwrap();
        assert_eq!(config, decoded);
    }
}
