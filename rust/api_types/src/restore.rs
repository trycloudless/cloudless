use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tracks each state of the restore process, emitted through a tokio mpsc channel.
/// Mirrors `BackupResult` but for the reverse pipeline (download, decrypt, decompress, verify).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestoreResult {
    /// Storage configuration loaded and decrypted successfully.
    ConfigLoaded { storage_id: Uuid },
    /// Starting pre-restore integrity verification of chunks.
    VerifyingChunks {
        total_files: usize,
        total_chunks: usize,
    },
    /// A single chunk's availability was verified in remote storage.
    ChunkVerified {
        file_path: String,
        chunk_index: u32,
        available: bool,
    },
    /// Pre-restore integrity check completed. If `missing > 0`, restore will abort.
    IntegrityCheckComplete {
        total_chunks: usize,
        available: usize,
        missing: usize,
    },
    /// A file restore has started.
    FileStarted {
        file_path: String,
        file_size: u64,
        version: u32,
    },
    /// A single chunk was downloaded, decrypted, decompressed, and hash-verified.
    ChunkDownloaded {
        file_path: String,
        chunk_index: u32,
        chunk_size: u32,
        bytes_restored: u64,
        file_size: u64,
    },
    /// A file was fully restored and its final hash verified.
    FileCompleted {
        file_path: String,
        total_chunks: u32,
        restored_bytes: u64,
    },
    /// A file was skipped because it already exists at the destination and the
    /// user selected `Skip existing`.
    FileSkipped { file_path: String },
    /// All selected files have been restored successfully.
    Completed {
        total_files: usize,
        total_bytes: u64,
    },
    /// An error occurred during restore.
    Failed {
        reason: String,
        file_path: Option<String>,
    },
}
