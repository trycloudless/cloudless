/// Post-backup local file cleanup.
///
/// After a successful backup job, deletes local files that have been safely
/// backed up for at least N days according to their `synced_at` timestamp.
/// File names are encrypted before reporting to the server so plaintext paths
/// never leave the device.
use api_types::backup_config::CleanupType;
use api_types::backup_job::{CleanupFileEntry, LogCleanupFilesRequest};
use chrono::Utc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    domain::derived_keys::DerivedKeys,
    domain::metadata_crypto::encrypt_file_path,
    ports::{api::backup_job_api_port::BackupJobApiPort, local_index::LocalIndexPort},
};

pub struct CleanupResult {
    pub files_deleted: usize,
    pub bytes_freed: u64,
}

/// Deletes locally-present files that have been backed up for at least the
/// configured threshold, then logs the deletions to the server.
///
/// Errors deleting individual files are logged but do not abort the run —
/// the cleanup result reflects only successfully deleted files.
pub async fn run_local_cleanup<L: LocalIndexPort, J: BackupJobApiPort>(
    cleanup_type: &CleanupType,
    config_id: Uuid,
    job_id: Uuid,
    local_index: &L,
    backup_job_api: &J,
    derived_keys: &DerivedKeys,
) -> CleanupResult {
    let days = match cleanup_type {
        CleanupType::NoCleanup => {
            return CleanupResult {
                files_deleted: 0,
                bytes_freed: 0,
            };
        }
        CleanupType::DaysAfterLastBackup(d) => *d,
    };

    let threshold = Utc::now() - chrono::Duration::days(days as i64);

    let candidates = match local_index
        .find_cleanup_candidates(config_id, threshold)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            error!(config_id = %config_id, error = %e, "Failed to query cleanup candidates");
            return CleanupResult {
                files_deleted: 0,
                bytes_freed: 0,
            };
        }
    };

    if candidates.is_empty() {
        return CleanupResult {
            files_deleted: 0,
            bytes_freed: 0,
        };
    }

    let mut deleted_entries: Vec<CleanupFileEntry> = Vec::new();
    let mut bytes_freed: u64 = 0;

    for entry in &candidates {
        match tokio::fs::remove_file(&entry.path).await {
            Ok(()) => {
                info!(
                    size = entry.size,
                    "Deleted local file after backup threshold"
                );

                let encrypted = match encrypt_file_path(
                    &entry.path,
                    &derived_keys.metadata_key,
                    &derived_keys.index_key,
                ) {
                    Ok(e) => e,
                    Err(err) => {
                        warn!(error = %err, "Failed to encrypt path for cleanup log; skipping server record");
                        bytes_freed += entry.size.unsigned_abs();
                        continue;
                    }
                };

                bytes_freed += entry.size.unsigned_abs();
                deleted_entries.push(CleanupFileEntry {
                    encrypted_name: encrypted.encrypted_name,
                    name_nonce: encrypted.nonce,
                    blind_index: encrypted.blind_index,
                    size: entry.size,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File was already removed externally; count it as freed.
                warn!("File already absent during cleanup");
                bytes_freed += entry.size.unsigned_abs();
            }
            Err(e) => {
                error!(error = %e, "Failed to delete local file during cleanup");
            }
        }
    }

    let files_deleted = deleted_entries.len();

    if !deleted_entries.is_empty() {
        if let Err(e) = backup_job_api
            .log_cleanup_files(LogCleanupFilesRequest {
                job_id,
                files: deleted_entries,
            })
            .await
        {
            error!(job_id = %job_id, error = %e, "Failed to log cleanup files to server");
        }
    }

    CleanupResult {
        files_deleted,
        bytes_freed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dek::Dek;
    use crate::model::base::AppResult;
    use crate::model::local_index::LocalIndexEntry;
    use crate::ports::api::{ApiResult, backup_job_api_port::BackupJobApiPort};
    use api_types::backup_job::{
        AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
        CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
        CreateBackupJobResponse, GetBackupJobDetailRequest, GetBackupJobDetailResponse,
        GetLatestBackupJobResponse, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
        ListBackupJobsRequest, ListBackupJobsResponse, LogCleanupFilesRequest,
    };
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use std::sync::{Arc, Mutex};
    use zeroize::Zeroizing;

    struct StubIndex {
        candidates: Vec<LocalIndexEntry>,
    }

    #[async_trait]
    impl crate::ports::local_index::LocalIndexPort for StubIndex {
        async fn upsert_file(&self, _: &LocalIndexEntry) -> AppResult<()> {
            unimplemented!()
        }
        async fn get_by_path(&self, _: Uuid, _: &str) -> AppResult<Option<LocalIndexEntry>> {
            unimplemented!()
        }
        async fn list_all(&self, _: Uuid) -> AppResult<Vec<LocalIndexEntry>> {
            unimplemented!()
        }
        async fn list_by_prefix(&self, _: Uuid, _: &str) -> AppResult<Vec<LocalIndexEntry>> {
            unimplemented!()
        }
        async fn remove_missing(&self, _: Uuid, _: &str, _: &[String]) -> AppResult<u64> {
            unimplemented!()
        }
        async fn mark_backed_up(&self, _: Uuid, _: &str, _: i32, _: Uuid) -> AppResult<()> {
            unimplemented!()
        }
        async fn clear_all(&self, _: Uuid) -> AppResult<()> {
            unimplemented!()
        }
        async fn find_cleanup_candidates(
            &self,
            _: Uuid,
            _: chrono::DateTime<Utc>,
        ) -> AppResult<Vec<LocalIndexEntry>> {
            Ok(self.candidates.clone())
        }
    }

    #[derive(Default)]
    struct SpyJobApi {
        logged: Arc<Mutex<Vec<LogCleanupFilesRequest>>>,
    }

    #[async_trait]
    impl BackupJobApiPort for SpyJobApi {
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
        async fn log_cleanup_files(&self, req: LogCleanupFilesRequest) -> ApiResult<()> {
            self.logged.lock().unwrap().push(req);
            Ok(())
        }
        async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse> {
            unimplemented!()
        }
    }

    fn test_keys() -> DerivedKeys {
        let dek = Dek {
            key: Zeroizing::new([42u8; 32]),
        };
        DerivedKeys::derive(&dek).unwrap()
    }

    #[tokio::test]
    async fn no_cleanup_returns_zero() {
        let index = StubIndex { candidates: vec![] };
        let api = SpyJobApi::default();
        let keys = test_keys();
        let result = run_local_cleanup(
            &CleanupType::NoCleanup,
            Uuid::now_v7(),
            Uuid::now_v7(),
            &index,
            &api,
            &keys,
        )
        .await;
        assert_eq!(result.files_deleted, 0);
        assert_eq!(result.bytes_freed, 0);
        assert!(api.logged.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_candidates_skips_log_call() {
        let index = StubIndex { candidates: vec![] };
        let api = SpyJobApi::default();
        let keys = test_keys();
        let result = run_local_cleanup(
            &CleanupType::DaysAfterLastBackup(7),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &index,
            &api,
            &keys,
        )
        .await;
        assert_eq!(result.files_deleted, 0);
        assert!(api.logged.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn deletes_file_and_logs_to_server() {
        use tempfile::NamedTempFile;
        let keys = test_keys();
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        // Keep the path but drop the handle so we own the file
        let path_owned = path.clone();
        drop(tmp);
        // Write something so the file actually exists
        std::fs::write(&path_owned, b"hello").unwrap();

        let encrypted = crate::domain::metadata_crypto::encrypt_file_path(
            &path_owned,
            &keys.metadata_key,
            &keys.index_key,
        )
        .unwrap();
        let entry = LocalIndexEntry {
            backup_config_id: Uuid::now_v7(),
            path: path_owned.clone(),
            size: 5,
            mtime: Utc::now(),
            content_hash: None,
            encrypted_name: encrypted.encrypted_name,
            encrypted_name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            remote_file_id: Some(Uuid::now_v7()),
            last_backed_up_version: Some(1),
            synced_at: Some(Utc::now() - Duration::days(10)),
        };

        let index = StubIndex {
            candidates: vec![entry],
        };
        let api = SpyJobApi::default();
        let result = run_local_cleanup(
            &CleanupType::DaysAfterLastBackup(7),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &index,
            &api,
            &keys,
        )
        .await;

        assert_eq!(result.files_deleted, 1);
        assert_eq!(result.bytes_freed, 5);
        assert!(
            !std::path::Path::new(&path_owned).exists(),
            "file should be deleted"
        );
        let logged = api.logged.lock().unwrap();
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].files.len(), 1);
        assert_eq!(logged[0].files[0].size, 5);
    }

    #[tokio::test]
    async fn server_log_failure_does_not_abort_cleanup() {
        use tempfile::NamedTempFile;
        let keys = test_keys();
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        drop(tmp);
        std::fs::write(&path, b"data").unwrap();

        let encrypted = crate::domain::metadata_crypto::encrypt_file_path(
            &path,
            &keys.metadata_key,
            &keys.index_key,
        )
        .unwrap();
        let entry = LocalIndexEntry {
            backup_config_id: Uuid::now_v7(),
            path: path.clone(),
            size: 4,
            mtime: Utc::now(),
            content_hash: None,
            encrypted_name: encrypted.encrypted_name,
            encrypted_name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            remote_file_id: Some(Uuid::now_v7()),
            last_backed_up_version: Some(1),
            synced_at: Some(Utc::now() - Duration::days(10)),
        };

        struct FailingJobApi;
        #[async_trait]
        impl BackupJobApiPort for FailingJobApi {
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
            async fn list_jobs(
                &self,
                _: ListBackupJobsRequest,
            ) -> ApiResult<ListBackupJobsResponse> {
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
                Err(crate::ports::api::ApiClientError::InvalidUrl(
                    "server down".into(),
                ))
            }
            async fn abandon_stale_jobs(&self) -> ApiResult<AbandonStaleJobsResponse> {
                unimplemented!()
            }
        }

        let index = StubIndex {
            candidates: vec![entry],
        };
        let result = run_local_cleanup(
            &CleanupType::DaysAfterLastBackup(7),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &index,
            &FailingJobApi,
            &keys,
        )
        .await;

        // Cleanup result is still reported even though server log failed
        assert_eq!(result.files_deleted, 1);
        assert_eq!(result.bytes_freed, 4);
        assert!(!std::path::Path::new(&path).exists());
    }

    #[tokio::test]
    async fn nonexistent_file_counts_as_already_freed() {
        use crate::domain::metadata_crypto::encrypt_file_path;
        let keys = test_keys();
        let encrypted = encrypt_file_path(
            "/nonexistent/ghost.txt",
            &keys.metadata_key,
            &keys.index_key,
        )
        .unwrap();

        let entry = LocalIndexEntry {
            backup_config_id: Uuid::now_v7(),
            path: "/nonexistent/ghost.txt".into(),
            size: 512,
            mtime: Utc::now(),
            content_hash: None,
            encrypted_name: encrypted.encrypted_name,
            encrypted_name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            remote_file_id: Some(Uuid::now_v7()),
            last_backed_up_version: Some(1),
            synced_at: Some(Utc::now() - Duration::days(10)),
        };

        let index = StubIndex {
            candidates: vec![entry],
        };
        let api = SpyJobApi::default();
        let result = run_local_cleanup(
            &CleanupType::DaysAfterLastBackup(7),
            Uuid::now_v7(),
            Uuid::now_v7(),
            &index,
            &api,
            &keys,
        )
        .await;
        // NotFound counts as freed bytes but not as a deleted file for the server log
        assert_eq!(result.bytes_freed, 512);
    }
}
