use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Enums ──────────────────────────────────────────────────────────

/// How to handle existing files at the restore destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverwriteBehavior {
    Overwrite,
    KeepBoth,
    SkipIfExists,
}

/// Where to write restored files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreDestination {
    OriginalPath,
    DownloadFolder,
    CustomPath(String),
    /// Write normal restored files to a configured storage target under
    /// `cloudless-restored/{user_id}/`. This is restore output only; it does not
    /// create or mutate backup versions.
    RemoteStorage {
        storage_id: Uuid,
    },
}

// ── Create / mutate types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRestoreJobRequest {
    pub backup_config_id: Uuid,
    pub overwrite_behavior: OverwriteBehavior,
    pub destination_type: String,
    pub destination_storage_id: Option<Uuid>,
    pub destination_prefix: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRestoreJobResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRestoreJobFileRequest {
    pub job_id: Uuid,
    pub remote_file_version_id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub original_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRestoreJobFileResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRestoreJobFileRequest {
    pub id: Uuid,
    pub restored_size: i64,
    pub total_chunks: i32,
    pub status: String,
    pub destination_path: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRestoreJobRequest {
    pub id: Uuid,
    pub status: String,
    pub error_message: Option<String>,
}

// ── Query types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreJobSummary {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub overwrite_behavior: String,
    pub destination_type: String,
    pub destination_storage_id: Option<Uuid>,
    pub destination_prefix: Option<String>,
    pub status: String,
    pub total_files: i32,
    pub original_bytes: i64,
    pub restored_bytes: i64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreJobFileSummary {
    pub id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub destination_path: Option<String>,
    pub original_size: i64,
    pub restored_size: i64,
    pub total_chunks: i32,
    pub chunks_completed: i32,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreJobDetail {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub overwrite_behavior: String,
    pub destination_type: String,
    pub destination_storage_id: Option<Uuid>,
    pub destination_prefix: Option<String>,
    pub status: String,
    pub total_files: i32,
    pub total_files_succeeded: i32,
    pub total_files_failed: i32,
    pub original_bytes: i64,
    pub restored_bytes: i64,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub files: Vec<RestoreJobFileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRestoreJobsRequest {
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRestoreJobsResponse {
    pub jobs: Vec<RestoreJobSummary>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRestoreJobDetailRequest {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRestoreJobDetailResponse {
    pub job: RestoreJobDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLatestRestoreJobResponse {
    pub job: Option<RestoreJobSummary>,
}

/// A restore job that was interrupted and can be resumed.
/// Includes the job metadata plus its incomplete file entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumableRestoreJob {
    pub job: RestoreJobSummary,
    pub pending_files: Vec<RestoreJobFileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResumableRestoreJobResponse {
    pub job: Option<ResumableRestoreJob>,
}

// ── Decrypted types (used by core → Tauri → UI) ───────────────────

/// Decrypted restore job file with plaintext file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedRestoreJobFileSummary {
    pub id: Uuid,
    pub file_path: String,
    pub destination_path: Option<String>,
    pub original_size: i64,
    pub restored_size: i64,
    pub total_chunks: i32,
    pub chunks_completed: i32,
    pub status: String,
    pub error_message: Option<String>,
}

/// Decrypted restore job detail with plaintext file paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedRestoreJobDetail {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub overwrite_behavior: String,
    pub destination_type: String,
    pub destination_storage_id: Option<Uuid>,
    pub destination_prefix: Option<String>,
    pub status: String,
    pub total_files: i32,
    pub total_files_succeeded: i32,
    pub total_files_failed: i32,
    pub original_bytes: i64,
    pub restored_bytes: i64,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub files: Vec<DecryptedRestoreJobFileSummary>,
}

/// Decrypted response wrapper for restore job detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedRestoreJobDetailResponse {
    pub job: DecryptedRestoreJobDetail,
}
