/// Port for the client-side local file index (SQLite-backed).
///
/// Tracks file metadata, encrypted names, and backup state to enable
/// client-driven backup candidate detection without server knowledge
/// of plaintext file paths.
///
/// All operations are scoped by `backup_config_id` so multiple backup
/// configs can independently track the same file paths.
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::base::AppResult;
use crate::model::local_index::LocalIndexEntry;

#[async_trait]
pub trait LocalIndexPort: Send + Sync {
    /// Inserts or updates a file entry in the local index.
    async fn upsert_file(&self, entry: &LocalIndexEntry) -> AppResult<()>;

    /// Retrieves a file entry by its plaintext path within a backup config.
    async fn get_by_path(&self, config_id: Uuid, path: &str) -> AppResult<Option<LocalIndexEntry>>;

    /// Lists all tracked file entries for a backup config.
    async fn list_all(&self, config_id: Uuid) -> AppResult<Vec<LocalIndexEntry>>;

    /// Lists file entries whose path starts with the given prefix within a backup config.
    async fn list_by_prefix(
        &self,
        config_id: Uuid,
        prefix: &str,
    ) -> AppResult<Vec<LocalIndexEntry>>;

    /// Removes entries under `prefix` whose paths are not in `existing_paths`.
    /// Returns the number of entries removed.
    async fn remove_missing(
        &self,
        config_id: Uuid,
        prefix: &str,
        existing_paths: &[String],
    ) -> AppResult<u64>;

    /// Marks a file as backed up with the given version and remote file ID.
    async fn mark_backed_up(
        &self,
        config_id: Uuid,
        path: &str,
        version: i32,
        remote_file_id: Uuid,
    ) -> AppResult<()>;

    /// Removes all entries from the index for a given backup config.
    async fn clear_all(&self, config_id: Uuid) -> AppResult<()>;

    /// Returns entries whose `synced_at` is non-null and older than `backed_up_before`.
    /// These are candidates for local deletion after a successful backup.
    async fn find_cleanup_candidates(
        &self,
        config_id: Uuid,
        backed_up_before: DateTime<Utc>,
    ) -> AppResult<Vec<LocalIndexEntry>>;
}
