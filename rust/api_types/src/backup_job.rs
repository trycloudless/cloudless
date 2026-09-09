use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackupJobRequest {
    pub backup_config_id: Uuid,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackupJobResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackupJobFileRequest {
    pub job_id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub original_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackupJobFileResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteBackupJobFileRequest {
    pub id: Uuid,
    pub uploaded_size: i64,
    pub deduplicated_size: i64,
    pub total_chunks: i32,
    pub deduplicated_chunks: i32,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteBackupJobRequest {
    pub id: Uuid,
    pub status: String,
    pub error_message: Option<String>,
}

/// One file deleted from the local device during post-backup cleanup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupFileEntry {
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub size: i64,
}

/// Sent by the client after local cleanup to record deleted files on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogCleanupFilesRequest {
    pub job_id: Uuid,
    pub files: Vec<CleanupFileEntry>,
}

/// Returned after marking stale running jobs as interrupted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbandonStaleJobsResponse {
    /// Number of jobs that were transitioned to "interrupted".
    pub count: u32,
}

// ── Query types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJobSummary {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub status: String,
    pub total_files: i32,
    pub original_bytes: i64,
    pub uploaded_bytes: i64,
    pub deduplicated_bytes: i64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJobFileSummary {
    pub id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub original_size: i64,
    pub uploaded_size: i64,
    pub deduplicated_size: i64,
    pub total_chunks: i32,
    pub deduplicated_chunks: i32,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJobDetail {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub status: String,
    pub total_files: i32,
    pub total_files_succeeded: i32,
    pub total_files_failed: i32,
    pub original_bytes: i64,
    pub uploaded_bytes: i64,
    pub deduplicated_bytes: i64,
    pub deduplicated_chunks: i32,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub files: Vec<BackupJobFileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBackupJobsRequest {
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBackupJobsResponse {
    pub jobs: Vec<BackupJobSummary>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBackupJobDetailRequest {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBackupJobDetailResponse {
    pub job: BackupJobDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLatestBackupJobResponse {
    pub job: Option<BackupJobSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumableBackupJob {
    pub job: BackupJobSummary,
    pub completed_blind_indexes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResumableBackupJobRequest {
    pub backup_config_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResumableBackupJobResponse {
    pub job: Option<ResumableBackupJob>,
}

// ── Decrypted types (used by core → Tauri → UI) ───────────────────

/// Decrypted backup job file with plaintext file path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedBackupJobFileSummary {
    pub id: Uuid,
    pub file_path: String,
    pub original_size: i64,
    pub uploaded_size: i64,
    pub deduplicated_size: i64,
    pub total_chunks: i32,
    pub deduplicated_chunks: i32,
    pub status: String,
    pub error_message: Option<String>,
}

/// Decrypted backup job detail with plaintext file paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedBackupJobDetail {
    pub id: Uuid,
    pub backup_config_id: Uuid,
    pub status: String,
    pub total_files: i32,
    pub total_files_succeeded: i32,
    pub total_files_failed: i32,
    pub original_bytes: i64,
    pub uploaded_bytes: i64,
    pub deduplicated_bytes: i64,
    pub deduplicated_chunks: i32,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub files: Vec<DecryptedBackupJobFileSummary>,
}

/// Decrypted response wrapper for backup job detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptedBackupJobDetailResponse {
    pub job: DecryptedBackupJobDetail,
}

// ── Display helpers ───────────────────────────────────────────────

/// Maps a raw backend job `status` string to a user-facing display label.
///
/// The backend uses snake_case enum values (`"in_progress"`,
/// `"completed_with_errors"`, etc.). This function converts them to
/// title-case strings suitable for rendering in the UI, so the raw
/// backend token never appears to users.
pub fn display_job_status(status: &str) -> &'static str {
    match status {
        "completed" => "Completed",
        "completed_with_errors" => "Completed with errors",
        "failed" => "Failed",
        "interrupted" => "Interrupted",
        "running" | "in_progress" => "Running",
        _ => "Unknown",
    }
}

/// Maps per-file status strings used in backup and restore job detail views.
pub fn display_file_status(status: &str) -> &'static str {
    match status {
        "completed" => "Completed",
        "failed" => "Failed",
        "uploading" => "Uploading",
        "pending" => "Pending",
        "restoring" => "Restoring",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_display_labels() {
        assert_eq!(display_job_status("completed"), "Completed");
        assert_eq!(
            display_job_status("completed_with_errors"),
            "Completed with errors"
        );
        assert_eq!(display_job_status("failed"), "Failed");
        assert_eq!(display_job_status("running"), "Running");
        assert_eq!(display_job_status("in_progress"), "Running");
        assert_eq!(display_job_status("interrupted"), "Interrupted");
        // Unknown values return "Unknown" rather than leaking internal tokens.
        assert_eq!(display_job_status("some_future_state"), "Unknown");
    }

    #[test]
    fn file_status_display_labels() {
        assert_eq!(display_file_status("completed"), "Completed");
        assert_eq!(display_file_status("failed"), "Failed");
        assert_eq!(display_file_status("uploading"), "Uploading");
        assert_eq!(display_file_status("pending"), "Pending");
        assert_eq!(display_file_status("restoring"), "Restoring");
    }
}
