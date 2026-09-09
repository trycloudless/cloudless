/// SQLite-backed implementation of [`LocalIndexPort`] for client-side file tracking.
///
/// Uses sqlx with SQLite to maintain a local index of files, their encrypted
/// metadata, and backup state. The database is a rebuildable cache — it can
/// be reconstructed from encrypted metadata stored on the server.
use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::local_index::LocalIndexEntry;
use crate::ports::local_index::LocalIndexPort;

#[derive(Clone)]
pub struct SqliteLocalIndex {
    pool: SqlitePool,
}

impl SqliteLocalIndex {
    /// Opens (or creates) a SQLite database at the given path with WAL mode enabled.
    pub async fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Failed to open SQLite database: {e}"),
                source: Some(Box::new(e)),
            })?;

        sqlx::migrate!("./migrations/sqlite")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Failed to run SQLite migrations: {e}"),
                source: Some(Box::new(e)),
            })?;

        Ok(Self { pool })
    }

    /// Opens an in-memory SQLite database for testing.
    pub async fn open_in_memory() -> AppResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .journal_mode(SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Failed to open in-memory SQLite: {e}"),
                source: Some(Box::new(e)),
            })?;

        sqlx::migrate!("./migrations/sqlite")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Failed to run SQLite migrations: {e}"),
                source: Some(Box::new(e)),
            })?;

        Ok(Self { pool })
    }

    fn map_sqlx_error(e: sqlx::Error) -> AppError {
        AppError::Internal {
            message: format!("SQLite error: {e}"),
            source: Some(Box::new(e)),
        }
    }

    fn row_to_entry(row: &sqlx::sqlite::SqliteRow) -> AppResult<LocalIndexEntry> {
        let config_id_str: String = row
            .try_get("backup_config_id")
            .map_err(Self::map_sqlx_error)?;
        let backup_config_id = Uuid::parse_str(&config_id_str).map_err(|e| AppError::Internal {
            message: format!("Invalid UUID in backup_config_id: {e}"),
            source: None,
        })?;

        let remote_file_id_str: Option<String> = row
            .try_get("remote_file_id")
            .map_err(Self::map_sqlx_error)?;
        let remote_file_id = remote_file_id_str
            .map(|s| {
                Uuid::parse_str(&s).map_err(|e| AppError::Internal {
                    message: format!("Invalid UUID in remote_file_id: {e}"),
                    source: None,
                })
            })
            .transpose()?;

        let synced_at_str: Option<String> =
            row.try_get("synced_at").map_err(Self::map_sqlx_error)?;
        let synced_at = synced_at_str
            .map(|s| {
                s.parse::<DateTime<Utc>>().map_err(|e| AppError::Internal {
                    message: format!("Invalid datetime in synced_at: {e}"),
                    source: None,
                })
            })
            .transpose()?;

        let mtime_str: String = row.try_get("mtime").map_err(Self::map_sqlx_error)?;
        let mtime = mtime_str
            .parse::<DateTime<Utc>>()
            .map_err(|e| AppError::Internal {
                message: format!("Invalid datetime in mtime: {e}"),
                source: None,
            })?;

        Ok(LocalIndexEntry {
            backup_config_id,
            path: row.try_get("path").map_err(Self::map_sqlx_error)?,
            size: row.try_get("size").map_err(Self::map_sqlx_error)?,
            mtime,
            content_hash: row.try_get("content_hash").map_err(Self::map_sqlx_error)?,
            encrypted_name: row
                .try_get("encrypted_name")
                .map_err(Self::map_sqlx_error)?,
            encrypted_name_nonce: row
                .try_get("encrypted_name_nonce")
                .map_err(Self::map_sqlx_error)?,
            blind_index: row.try_get("blind_index").map_err(Self::map_sqlx_error)?,
            remote_file_id,
            last_backed_up_version: row
                .try_get("last_backed_up_version")
                .map_err(Self::map_sqlx_error)?,
            synced_at,
        })
    }
}

#[async_trait]
impl LocalIndexPort for SqliteLocalIndex {
    async fn upsert_file(&self, entry: &LocalIndexEntry) -> AppResult<()> {
        let config_id_str = entry.backup_config_id.to_string();
        let mtime_str = entry.mtime.to_rfc3339();
        let remote_file_id_str = entry.remote_file_id.map(|id| id.to_string());
        let synced_at_str = entry.synced_at.map(|dt| dt.to_rfc3339());

        sqlx::query(
            r#"
            INSERT INTO files (backup_config_id, path, size, mtime, content_hash, encrypted_name, encrypted_name_nonce, blind_index, remote_file_id, last_backed_up_version, synced_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(backup_config_id, path) DO UPDATE SET
                size = excluded.size,
                mtime = excluded.mtime,
                content_hash = excluded.content_hash,
                encrypted_name = excluded.encrypted_name,
                encrypted_name_nonce = excluded.encrypted_name_nonce,
                blind_index = excluded.blind_index,
                remote_file_id = COALESCE(excluded.remote_file_id, files.remote_file_id),
                last_backed_up_version = COALESCE(excluded.last_backed_up_version, files.last_backed_up_version),
                synced_at = excluded.synced_at
            "#,
        )
        .bind(&config_id_str)
        .bind(&entry.path)
        .bind(entry.size)
        .bind(&mtime_str)
        .bind(&entry.content_hash)
        .bind(&entry.encrypted_name)
        .bind(&entry.encrypted_name_nonce)
        .bind(&entry.blind_index)
        .bind(&remote_file_id_str)
        .bind(entry.last_backed_up_version)
        .bind(&synced_at_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_sqlx_error)?;

        Ok(())
    }

    async fn get_by_path(&self, config_id: Uuid, path: &str) -> AppResult<Option<LocalIndexEntry>> {
        let config_id_str = config_id.to_string();
        let row = sqlx::query("SELECT * FROM files WHERE backup_config_id = ?1 AND path = ?2")
            .bind(&config_id_str)
            .bind(path)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::map_sqlx_error)?;

        row.as_ref().map(Self::row_to_entry).transpose()
    }

    async fn list_all(&self, config_id: Uuid) -> AppResult<Vec<LocalIndexEntry>> {
        let config_id_str = config_id.to_string();
        let rows = sqlx::query("SELECT * FROM files WHERE backup_config_id = ?1 ORDER BY path")
            .bind(&config_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_sqlx_error)?;

        rows.iter().map(Self::row_to_entry).collect()
    }

    async fn list_by_prefix(
        &self,
        config_id: Uuid,
        prefix: &str,
    ) -> AppResult<Vec<LocalIndexEntry>> {
        let config_id_str = config_id.to_string();
        let pattern = format!("{prefix}%");
        let rows = sqlx::query(
            "SELECT * FROM files WHERE backup_config_id = ?1 AND path LIKE ?2 ORDER BY path",
        )
        .bind(&config_id_str)
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_sqlx_error)?;

        rows.iter().map(Self::row_to_entry).collect()
    }

    async fn remove_missing(
        &self,
        config_id: Uuid,
        prefix: &str,
        existing_paths: &[String],
    ) -> AppResult<u64> {
        let config_id_str = config_id.to_string();

        if existing_paths.is_empty() {
            // Remove all files under the prefix for this config
            let pattern = format!("{prefix}%");
            let result =
                sqlx::query("DELETE FROM files WHERE backup_config_id = ?1 AND path LIKE ?2")
                    .bind(&config_id_str)
                    .bind(&pattern)
                    .execute(&self.pool)
                    .await
                    .map_err(Self::map_sqlx_error)?;
            return Ok(result.rows_affected());
        }

        // Get all paths under prefix for this config
        let pattern = format!("{prefix}%");
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT path FROM files WHERE backup_config_id = ?1 AND path LIKE ?2")
                .bind(&config_id_str)
                .bind(&pattern)
                .fetch_all(&self.pool)
                .await
                .map_err(Self::map_sqlx_error)?;

        let mut removed = 0u64;
        for (path,) in &rows {
            if !existing_paths.contains(path) {
                sqlx::query("DELETE FROM files WHERE backup_config_id = ?1 AND path = ?2")
                    .bind(&config_id_str)
                    .bind(path)
                    .execute(&self.pool)
                    .await
                    .map_err(Self::map_sqlx_error)?;
                removed += 1;
            }
        }

        Ok(removed)
    }

    async fn mark_backed_up(
        &self,
        config_id: Uuid,
        path: &str,
        version: i32,
        remote_file_id: Uuid,
    ) -> AppResult<()> {
        let config_id_str = config_id.to_string();
        let now = Utc::now().to_rfc3339();
        let remote_id_str = remote_file_id.to_string();

        let result = sqlx::query(
            "UPDATE files SET last_backed_up_version = ?1, remote_file_id = ?2, synced_at = ?3 WHERE backup_config_id = ?4 AND path = ?5",
        )
        .bind(version)
        .bind(&remote_id_str)
        .bind(&now)
        .bind(&config_id_str)
        .bind(path)
        .execute(&self.pool)
        .await
        .map_err(Self::map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound {
                message: format!("File not found in local index: {path}"),
                source: None,
            });
        }

        Ok(())
    }

    async fn clear_all(&self, config_id: Uuid) -> AppResult<()> {
        let config_id_str = config_id.to_string();
        sqlx::query("DELETE FROM files WHERE backup_config_id = ?1")
            .bind(&config_id_str)
            .execute(&self.pool)
            .await
            .map_err(Self::map_sqlx_error)?;
        Ok(())
    }

    async fn find_cleanup_candidates(
        &self,
        config_id: Uuid,
        backed_up_before: DateTime<Utc>,
    ) -> AppResult<Vec<LocalIndexEntry>> {
        let config_id_str = config_id.to_string();
        let threshold_str = backed_up_before.to_rfc3339();
        let rows = sqlx::query(
            "SELECT * FROM files WHERE backup_config_id = ?1 AND synced_at IS NOT NULL AND synced_at < ?2 ORDER BY path",
        )
        .bind(&config_id_str)
        .bind(&threshold_str)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_sqlx_error)?;

        rows.iter().map(Self::row_to_entry).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::dek::Dek;
    use crate::domain::derived_keys::DerivedKeys;
    use crate::domain::metadata_crypto::encrypt_file_path;
    use zeroize::Zeroizing;

    fn test_keys() -> DerivedKeys {
        let dek = Dek {
            key: Zeroizing::new([42u8; 32]),
        };
        DerivedKeys::derive(&dek).unwrap()
    }

    fn test_config_id() -> Uuid {
        // Deterministic UUID for tests
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    fn make_entry(path: &str, keys: &DerivedKeys) -> LocalIndexEntry {
        let encrypted = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        LocalIndexEntry {
            backup_config_id: test_config_id(),
            path: path.to_string(),
            size: 1024,
            mtime: Utc::now(),
            content_hash: Some("abc123".to_string()),
            encrypted_name: encrypted.encrypted_name,
            encrypted_name_nonce: encrypted.nonce,
            blind_index: encrypted.blind_index,
            remote_file_id: None,
            last_backed_up_version: None,
            synced_at: None,
        }
    }

    #[tokio::test]
    async fn upsert_and_get_by_path() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();
        let entry = make_entry("/home/user/file.txt", &keys);

        index.upsert_file(&entry).await.unwrap();
        let result = index.get_by_path(cid, "/home/user/file.txt").await.unwrap();

        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.path, "/home/user/file.txt");
        assert_eq!(found.size, 1024);
        assert_eq!(found.content_hash, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn get_by_path_not_found() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let cid = test_config_id();
        let result = index.get_by_path(cid, "/nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn upsert_updates_existing() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();
        let mut entry = make_entry("/file.txt", &keys);

        index.upsert_file(&entry).await.unwrap();

        entry.size = 2048;
        entry.content_hash = Some("def456".to_string());
        index.upsert_file(&entry).await.unwrap();

        let found = index.get_by_path(cid, "/file.txt").await.unwrap().unwrap();
        assert_eq!(found.size, 2048);
        assert_eq!(found.content_hash, Some("def456".to_string()));
    }

    #[tokio::test]
    async fn list_all_returns_sorted() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/b.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/a.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/c.txt", &keys))
            .await
            .unwrap();

        let all = index.list_all(cid).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].path, "/a.txt");
        assert_eq!(all[1].path, "/b.txt");
        assert_eq!(all[2].path, "/c.txt");
    }

    #[tokio::test]
    async fn list_by_prefix_filters_correctly() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/home/docs/a.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/home/docs/b.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/other/c.txt", &keys))
            .await
            .unwrap();

        let docs = index.list_by_prefix(cid, "/home/docs/").await.unwrap();
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].path, "/home/docs/a.txt");
        assert_eq!(docs[1].path, "/home/docs/b.txt");
    }

    #[tokio::test]
    async fn remove_missing_deletes_absent_files() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/dir/a.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir/b.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir/c.txt", &keys))
            .await
            .unwrap();

        // Only a.txt still exists on disk
        let existing = vec!["/dir/a.txt".to_string()];
        let removed = index.remove_missing(cid, "/dir/", &existing).await.unwrap();

        assert_eq!(removed, 2);
        let remaining = index.list_all(cid).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, "/dir/a.txt");
    }

    #[tokio::test]
    async fn remove_missing_with_empty_existing_removes_all() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/dir/a.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/dir/b.txt", &keys))
            .await
            .unwrap();

        let removed = index.remove_missing(cid, "/dir/", &[]).await.unwrap();
        assert_eq!(removed, 2);
        assert!(index.list_all(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mark_backed_up_updates_fields() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/file.txt", &keys))
            .await
            .unwrap();

        let file_id = Uuid::now_v7();
        index
            .mark_backed_up(cid, "/file.txt", 1, file_id)
            .await
            .unwrap();

        let found = index.get_by_path(cid, "/file.txt").await.unwrap().unwrap();
        assert_eq!(found.last_backed_up_version, Some(1));
        assert_eq!(found.remote_file_id, Some(file_id));
        assert!(found.synced_at.is_some());
    }

    #[tokio::test]
    async fn mark_backed_up_nonexistent_returns_error() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let cid = test_config_id();
        let result = index
            .mark_backed_up(cid, "/nonexistent", 1, Uuid::now_v7())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn clear_all_removes_everything() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        index
            .upsert_file(&make_entry("/a.txt", &keys))
            .await
            .unwrap();
        index
            .upsert_file(&make_entry("/b.txt", &keys))
            .await
            .unwrap();

        index.clear_all(cid).await.unwrap();
        assert!(index.list_all(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn upsert_preserves_backup_state_on_re_scan() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        // Initial scan inserts a file
        index
            .upsert_file(&make_entry("/file.txt", &keys))
            .await
            .unwrap();

        // Mark it as backed up
        let file_id = Uuid::now_v7();
        index
            .mark_backed_up(cid, "/file.txt", 3, file_id)
            .await
            .unwrap();

        // Re-scan upserts with None for remote_file_id and version (simulating a modified file)
        let mut entry = make_entry("/file.txt", &keys);
        entry.size = 2048; // file changed
        entry.remote_file_id = None;
        entry.last_backed_up_version = None;
        entry.synced_at = None; // cleared → marks as needing backup
        index.upsert_file(&entry).await.unwrap();

        // remote_file_id and last_backed_up_version are preserved via COALESCE;
        // synced_at is cleared so the file is picked up as a backup candidate.
        let found = index.get_by_path(cid, "/file.txt").await.unwrap().unwrap();
        assert_eq!(found.size, 2048);
        assert_eq!(found.remote_file_id, Some(file_id));
        assert_eq!(found.last_backed_up_version, Some(3));
        assert!(found.synced_at.is_none());
    }

    #[tokio::test]
    async fn different_configs_are_isolated() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let config_a = Uuid::parse_str("00000000-0000-0000-0000-00000000000a").unwrap();
        let config_b = Uuid::parse_str("00000000-0000-0000-0000-00000000000b").unwrap();

        let mut entry_a = make_entry("/shared/file.txt", &keys);
        entry_a.backup_config_id = config_a;
        entry_a.size = 100;

        let mut entry_b = make_entry("/shared/file.txt", &keys);
        entry_b.backup_config_id = config_b;
        entry_b.size = 200;

        index.upsert_file(&entry_a).await.unwrap();
        index.upsert_file(&entry_b).await.unwrap();

        // Each config sees only its own entry
        let found_a = index
            .get_by_path(config_a, "/shared/file.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_a.size, 100);

        let found_b = index
            .get_by_path(config_b, "/shared/file.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_b.size, 200);

        // list_all is scoped
        assert_eq!(index.list_all(config_a).await.unwrap().len(), 1);
        assert_eq!(index.list_all(config_b).await.unwrap().len(), 1);

        // clear_all only affects one config
        index.clear_all(config_a).await.unwrap();
        assert!(index.list_all(config_a).await.unwrap().is_empty());
        assert_eq!(index.list_all(config_b).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn find_cleanup_candidates_returns_old_synced_files() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();
        let file_id = Uuid::now_v7();

        // Insert and mark a file as backed up
        index
            .upsert_file(&make_entry("/old.txt", &keys))
            .await
            .unwrap();
        index
            .mark_backed_up(cid, "/old.txt", 1, file_id)
            .await
            .unwrap();

        // Threshold in the future relative to synced_at, so the file qualifies
        let threshold = Utc::now() + chrono::Duration::seconds(10);
        let candidates = index.find_cleanup_candidates(cid, threshold).await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "/old.txt");
    }

    #[tokio::test]
    async fn find_cleanup_candidates_excludes_not_yet_synced() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();

        // File not yet marked as backed up (synced_at IS NULL)
        index
            .upsert_file(&make_entry("/pending.txt", &keys))
            .await
            .unwrap();

        let threshold = Utc::now() + chrono::Duration::seconds(10);
        let candidates = index.find_cleanup_candidates(cid, threshold).await.unwrap();
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn find_cleanup_candidates_excludes_recently_synced() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let cid = test_config_id();
        let file_id = Uuid::now_v7();

        index
            .upsert_file(&make_entry("/recent.txt", &keys))
            .await
            .unwrap();
        index
            .mark_backed_up(cid, "/recent.txt", 1, file_id)
            .await
            .unwrap();

        // Threshold in the past: synced_at (now) is NOT before the threshold (yesterday)
        let threshold = Utc::now() - chrono::Duration::days(1);
        let candidates = index.find_cleanup_candidates(cid, threshold).await.unwrap();
        assert!(candidates.is_empty());
    }

    #[tokio::test]
    async fn find_cleanup_candidates_scoped_to_config() {
        let index = SqliteLocalIndex::open_in_memory().await.unwrap();
        let keys = test_keys();
        let config_a = Uuid::parse_str("00000000-0000-0000-0000-00000000000a").unwrap();
        let config_b = Uuid::parse_str("00000000-0000-0000-0000-00000000000b").unwrap();
        let file_id = Uuid::now_v7();

        let mut entry_a = make_entry("/file.txt", &keys);
        entry_a.backup_config_id = config_a;
        index.upsert_file(&entry_a).await.unwrap();
        index
            .mark_backed_up(config_a, "/file.txt", 1, file_id)
            .await
            .unwrap();

        let threshold = Utc::now() + chrono::Duration::seconds(10);
        // config_b sees nothing even though config_a has a candidate
        let candidates = index
            .find_cleanup_candidates(config_b, threshold)
            .await
            .unwrap();
        assert!(candidates.is_empty());
        let candidates = index
            .find_cleanup_candidates(config_a, threshold)
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1);
    }
}
