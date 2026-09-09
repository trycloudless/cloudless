use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::chunk::ChunkStorageMeta;

/// Request to fetch the ordered list of chunks belonging to a file version.
/// Used during restore to know which chunks to download from remote storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFileVersionChunksRequest {
    pub version_id: Uuid,
}

/// A single chunk's metadata needed for restore: its storage location, hash for
/// integrity verification, and position in the file's chunk sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreChunkInfo {
    pub chunk_id: Uuid,
    pub chunk_index: i32,
    /// The original raw chunk SHA-256 hash (32 bytes). Used to verify integrity
    /// after downloading, decrypting, and decompressing.
    pub hash: Vec<u8>,
    pub size: i64,
    pub storage_meta: ChunkStorageMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFileVersionChunksResponse {
    pub chunks: Vec<RestoreChunkInfo>,
}
