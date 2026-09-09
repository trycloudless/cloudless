use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkStatus {
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkStatusWithTime {
    pub timestamp: DateTime<Utc>,
    pub status: ChunkStatus,
}

/// Storage-agnostic metadata stored per chunk to locate it in any storage backend.
/// Previously named `S3` — the `alias` ensures backward compatibility with existing DB rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkStorageMeta {
    #[serde(alias = "S3")]
    ObjectStore(ChunkObjectStoreMeta),
}

/// Encryption metadata stored alongside each chunk so we can decrypt it during restore.
/// The ciphertext itself lives in object storage; this struct captures the nonce and
/// algorithm needed to decrypt it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkEncryptionMeta {
    pub nonce: Vec<u8>,
    pub algorithm: crate::common::EncryptionAlgorithm,
}

/// Generic object-store metadata for a chunk. The `key` is the storage path/key
/// used by any backend (S3, Google Drive, local filesystem).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkObjectStoreMeta {
    pub key: String,
    pub hash: String,
    /// Encryption metadata (nonce + algorithm) used when this chunk was encrypted.
    /// None for chunks created before encryption tracking was added, or for deduplicated
    /// chunks that reuse an existing chunk's encryption metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<ChunkEncryptionMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkStatusHistory(Vec<ChunkStatusWithTime>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkEntity {
    pub id: Uuid,
    // this is the original raw chunk hash
    pub hash: Vec<u8>,
    pub size: u64,
    pub user_id: Uuid,
    pub storage_id: Uuid,
    pub storage_meta: ChunkStorageMeta,
    pub status_history: Vec<ChunkStatusWithTime>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChunkRequest {
    // pub id: Uuid,
    // this is the original raw chunk hash
    pub hash: Vec<u8>,
    pub size: u64,
    // pub user_id: Uuid,
    pub storage_id: Uuid,
    pub remote_file_version_id: Uuid,
    pub chunk_index: i32,
    pub storage_meta: ChunkStorageMeta,
    pub status_history: Vec<ChunkStatusWithTime>,
    // pub created_at: DateTime<Utc>,
    /// Deterministically derived by the client so a repeated attempt reuses
    /// the same key instead of creating a duplicate request.
    pub idempotency_key: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateChunkResponse {
    pub id: Uuid,
    /// True if the chunk already existed in the DB (same hash+user+storage).
    /// When true, the existing row's encryption metadata was preserved and the
    /// caller can safely skip re-uploading.
    pub existed_in_db: bool,
}

/// Request to update a chunk's storage metadata (e.g. after re-encryption).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateChunkStorageMetaRequest {
    pub chunk_id: Uuid,
    pub storage_meta: ChunkStorageMeta,
}

pub struct ManifestChunkEntity {
    pub manifest_id: Uuid,
    pub chunk_hash: Vec<u8>,
    pub chunk_index: u32,
    pub created_at: DateTime<Utc>,
}

// todo: Add Request response type for chunks
