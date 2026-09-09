use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::chunk::ChunkStorageMeta;

// ── GC collect (Phase A) ───────────────────────────────────────────

/// Request to trigger garbage collection for the authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcCollectRequest {}

/// Response from GC collection: expired versions marked Deleted, orphaned chunks identified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcCollectResponse {
    pub gc_run_id: Uuid,
    pub versions_deleted: u64,
    pub orphaned_chunks: Vec<OrphanedChunkInfo>,
}

/// Info about a chunk that is no longer referenced by any active file version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedChunkInfo {
    pub chunk_id: Uuid,
    pub storage_id: Uuid,
    pub size: i32,
    pub storage_meta: ChunkStorageMeta,
}

// ── GC confirm chunk deletions (Phase C) ───────────────────────────

/// Request to confirm that orphaned chunks have been deleted from storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmChunkDeletionsRequest {
    pub gc_run_id: Uuid,
    pub chunk_ids: Vec<Uuid>,
}

/// Response after confirming chunk deletions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmChunkDeletionsResponse {
    pub chunks_deleted: u64,
    pub storage_freed_bytes: i64,
}

// ── GC run history ─────────────────────────────────────────────────

/// Status of a GC run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GcRunStatus {
    Running,
    Completed,
    Failed,
}

/// Summary of a single GC run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcRunSummary {
    pub id: Uuid,
    pub status: GcRunStatus,
    pub versions_deleted: i32,
    pub chunks_deleted: i32,
    pub storage_freed_bytes: i64,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Request to list recent GC runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGcRunsRequest {
    /// Maximum number of runs to return (default 10).
    pub limit: Option<i32>,
}

/// Response containing recent GC runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGcRunsResponse {
    pub runs: Vec<GcRunSummary>,
}

/// Request to get details of a specific GC run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetGcRunDetailRequest {
    pub gc_run_id: Uuid,
}

/// Detailed info for a specific GC run, including individual items processed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcRunDetailResponse {
    pub run: GcRunSummary,
    pub versions: Vec<GcRunVersionInfo>,
    pub chunks: Vec<GcRunChunkInfo>,
}

/// Info about a file version that was garbage collected in a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcRunVersionInfo {
    pub file_version_id: Uuid,
    pub encrypted_name: Vec<u8>,
    pub name_nonce: Vec<u8>,
    pub version: i32,
    pub size: i64,
}

/// Info about a chunk that was processed in a GC run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcRunChunkInfo {
    pub chunk_id: Uuid,
    pub storage_id: Uuid,
    pub size: i32,
    pub deleted_from_storage: bool,
}

// ── Retention settings ─────────────────────────────────────────────

/// Response with the user's retention settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRetentionSettingsResponse {
    pub bin_retention_days: i32,
}

/// Request to update the user's bin retention period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRetentionSettingsRequest {
    /// Number of days to keep versions in bin before GC (7-30).
    pub bin_retention_days: i32,
}
