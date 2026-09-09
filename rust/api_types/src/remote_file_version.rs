use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileVersionStatus {
    Uploading,
    UploadCompleted,
    UploadFailed(String),
    VerifiedOnRemoteStorage,
    VerifiedOnRemoteStorageFailed(String),
    /// Soft-deleted — version is hidden from file browser but recoverable
    /// during the retention period. Timestamp tracked via `status_history`.
    MovedToBin,
    /// Restored from bin — treated as active (shown in file browser, excluded from GC).
    Restored,
    /// Permanently deleted — chunks cleaned up, version is a tombstone.
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVersionStatusHistory(pub Vec<(DateTime<Utc>, FileVersionStatus)>);

pub struct FileVersionEntity {
    pub id: Uuid,
    pub file_id: Uuid,
    pub version: u32,
    pub storage_id: Uuid,
    pub size: u64,
    pub status: FileVersionStatus,
    pub status_history: FileVersionStatusHistory,
    // pub mtime: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateFileVersionRequest {
    pub backup_config_id: Uuid,
    pub device_id: Uuid,
    pub storage_id: Uuid,
    pub size: i64,
    pub status: FileVersionStatus,
    pub local_file_updated_at: DateTime<Utc>,
    /// Last version the client observed for this file. `None` means the client
    /// believes no version exists yet. The server assigns the actual next
    /// version and rejects the request with a conflict if this is stale.
    pub base_version: Option<u32>,
    /// Deterministically derived by the client so that a repeated attempt
    /// (e.g. the next scheduled backup run re-picking up the same candidate)
    /// reuses the same key instead of creating a duplicate request.
    pub idempotency_key: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub name_blind_index: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateFileVersionResponse {
    pub id: Uuid,
    /// The version number assigned by the server.
    pub version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateFileVersionStatusRequest {
    pub id: Uuid,
    pub status: FileVersionStatus,
}

// ── File browsing types ────────────────────────────────────────────

/// Default page size for file listing pagination.
pub const DEFAULT_PAGE_SIZE: i64 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBackedUpFilesRequest {
    pub backup_config_id: Uuid,
    /// Cursor for keyset pagination: the `blind_index` of the last file
    /// from the previous page. `None` for the first page.
    #[serde(default)]
    pub cursor: Option<Vec<u8>>,
    /// Maximum number of *files* (not versions) to return. Defaults to 50.
    #[serde(default = "default_page_size")]
    pub limit: i64,
}

fn default_page_size() -> i64 {
    DEFAULT_PAGE_SIZE
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBackedUpFilesResponse {
    pub files: Vec<BackedUpFile>,
    /// Whether more files are available after this page.
    pub has_more: bool,
    /// The cursor to pass in the next request. `None` if no more pages.
    pub next_cursor: Option<Vec<u8>>,
    /// Total number of distinct files (for display, e.g. "showing 50 of 1,234").
    /// Only populated on the first page (when `cursor` is `None`).
    #[serde(default)]
    pub total_files: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedUpFile {
    pub file_id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    /// Decrypted file path. Empty when received from the server;
    /// populated by the client after decrypting `encrypted_name`.
    #[serde(default)]
    pub path: String,
    pub size: i64,
    pub file_updated_at: DateTime<Utc>,
    pub versions: Vec<FileVersionSummary>,
}

// ── Recovery types ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAllVersionsRequest {
    pub backup_config_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAllVersionsResponse {
    pub versions: Vec<RemoteFileVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileVersion {
    pub id: Uuid,
    pub device_id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub name_blind_index: Vec<u8>,
    pub version: u32,
    pub size: i64,
    pub status: FileVersionStatus,
    pub local_file_updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersionSummary {
    pub version_id: Uuid,
    pub version: u32,
    pub size: i64,
    pub status: FileVersionStatus,
    pub created_at: DateTime<Utc>,
}

// ── Bin (soft-delete) types ────────────────────────────────────────

/// Move a single file version to bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveVersionToBinRequest {
    pub version_id: Uuid,
}

/// Move ALL versions of a file (identified by blind index) to bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveAllVersionsToBinRequest {
    pub backup_config_id: Uuid,
    pub name_blind_index: Vec<u8>,
}

/// Restore a single file version from bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreVersionFromBinRequest {
    pub version_id: Uuid,
}

/// List versions currently in bin for a backup config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBinVersionsRequest {
    pub backup_config_id: Uuid,
}

/// Response containing binned files (reuses BackedUpFile).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBinVersionsResponse {
    pub files: Vec<BackedUpFile>,
}
