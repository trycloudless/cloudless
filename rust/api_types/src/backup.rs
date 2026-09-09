use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupResult {
    ConfigLoaded {
        config_id: Uuid,
        storage_id: Uuid,
    },
    IndexUpdated {
        files_updated: usize,
        files_failed: usize,
    },
    CandidatesFound {
        total_files: usize,
        total_bytes: u64,
    },
    FileStarted {
        file_path: String,
        file_size: u64,
        /// Last version the client observed for this file, `None` if it has
        /// never been backed up before. The server assigns the actual version
        /// once the file version is created — see `ChunkUploaded`/`FileCompleted`
        /// for events that happen after that point.
        base_version: Option<u32>,
    },
    ChunkUploaded {
        file_path: String,
        chunk_index: u32,
        chunk_size: u32,
        uploaded_size: u32,
        deduplicated: bool,
        bytes_uploaded: u64,
        file_size: u64,
    },
    FileCompleted {
        file_path: String,
        total_chunks: u32,
        uploaded_bytes: u64,
        deduplicated_bytes: u64,
    },
    /// A single file failed but the backup job continues with the next file.
    FileFailed {
        file_path: String,
        reason: String,
    },
    Completed {
        total_files: usize,
        total_bytes: u64,
        uploaded_bytes: u64,
        deduplicated_bytes: u64,
    },
    CleanupCompleted {
        files_deleted: usize,
        bytes_freed: u64,
    },
    Failed {
        reason: String,
        file_path: Option<String>,
    },
}
