use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::{debug, instrument};

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::file::ObjectKey;
use crate::ports::storage::StoragePort;

/// Storage adapter that reads and writes chunk data to the local filesystem.
///
/// Each [`ObjectKey`] maps to a file at `root_dir/{key}`. Parent directories
/// are created automatically on `put`. This adapter is used for local/USB/NAS
/// backup targets (desktop only).
#[derive(Debug)]
pub struct LocalFsStorageAdaptor {
    root_dir: PathBuf,
}

impl LocalFsStorageAdaptor {
    /// Creates a new adapter rooted at the given directory.
    ///
    /// Returns an error if `root_dir` does not exist or is not a directory.
    pub fn new(root_dir: &str) -> AppResult<Self> {
        let path = PathBuf::from(root_dir);
        if !path.exists() {
            return Err(AppError::NotFound {
                message: format!("storage directory does not exist: {}", root_dir),
                source: None,
            });
        }
        if !path.is_dir() {
            return Err(AppError::Internal {
                message: format!("storage path is not a directory: {}", root_dir),
                source: None,
            });
        }
        Ok(Self { root_dir: path })
    }

    /// Resolves an [`ObjectKey`] to an absolute file path under the root directory.
    fn resolve_path(&self, key: &ObjectKey) -> PathBuf {
        self.root_dir.join(key.as_str())
    }
}

#[async_trait]
impl StoragePort for LocalFsStorageAdaptor {
    #[instrument(skip(self, data), fields(key = %key.as_str(), size = data.len()))]
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        let path = self.resolve_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to create parent directories for {}", path.display()),
                    source: Some(e.into()),
                })?;
        }
        tokio::fs::write(&path, &data)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to write chunk to {}", path.display()),
                source: Some(e.into()),
            })?;
        debug!("stored {} bytes", data.len());
        Ok(())
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        let path = self.resolve_path(key);
        tokio::fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound {
                    message: format!("chunk not found: {}", path.display()),
                    source: Some(e.into()),
                }
            } else {
                AppError::Internal {
                    message: format!("failed to read chunk from {}", path.display()),
                    source: Some(e.into()),
                }
            }
        })
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        let path = self.resolve_path(key);
        tokio::fs::remove_file(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Deleting a non-existent key is not an error (idempotent).
                return AppError::NotFound {
                    message: format!("chunk not found for deletion: {}", path.display()),
                    source: Some(e.into()),
                };
            }
            AppError::Internal {
                message: format!("failed to delete chunk at {}", path.display()),
                source: Some(e.into()),
            }
        })
    }

    #[instrument(skip(self), fields(prefix = %prefix.as_str()))]
    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        let prefix_path = self.resolve_path(prefix);
        let search_dir = if prefix_path.is_dir() {
            prefix_path
        } else {
            prefix_path.parent().unwrap_or(&self.root_dir).to_path_buf()
        };

        if !search_dir.exists() {
            return Ok(vec![]);
        }

        let prefix_str = prefix.as_str();
        let mut results = Vec::new();
        collect_keys_recursive(&self.root_dir, &search_dir, prefix_str, &mut results).await?;
        Ok(results)
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        let path = self.resolve_path(key);
        Ok(path.exists())
    }
}

/// Recursively walks `dir` and collects all file keys matching `prefix`.
async fn collect_keys_recursive(
    root: &Path,
    dir: &Path,
    prefix: &str,
    results: &mut Vec<ObjectKey>,
) -> AppResult<()> {
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to read directory {}", dir.display()),
            source: Some(e.into()),
        })?;

    while let Some(entry) = entries.next_entry().await.map_err(|e| AppError::Internal {
        message: format!("failed to read directory entry in {}", dir.display()),
        source: Some(e.into()),
    })? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(collect_keys_recursive(root, &path, prefix, results)).await?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            let key = relative.to_string_lossy().to_string();
            if key.starts_with(prefix) {
                results.push(ObjectKey::new(key));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, LocalFsStorageAdaptor) {
        let dir = TempDir::new().unwrap();
        let adaptor = LocalFsStorageAdaptor::new(dir.path().to_str().unwrap()).unwrap();
        (dir, adaptor)
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("chunk-abc123".to_string());
        let data = b"hello world".to_vec();

        adaptor.put(&key, data.clone()).await.unwrap();
        let result = adaptor.get(&key).await.unwrap();

        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_exists_true_false() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("chunk-exists-test".to_string());

        assert!(!adaptor.exists(&key).await.unwrap());

        adaptor.put(&key, b"data".to_vec()).await.unwrap();

        assert!(adaptor.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("chunk-delete-test".to_string());

        adaptor.put(&key, b"data".to_vec()).await.unwrap();
        assert!(adaptor.exists(&key).await.unwrap());

        adaptor.delete(&key).await.unwrap();
        assert!(!adaptor.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_error() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("does-not-exist".to_string());

        let result = adaptor.get(&key).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_put_creates_parent_dirs() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("subdir/nested/chunk-nested".to_string());
        let data = b"nested data".to_vec();

        adaptor.put(&key, data.clone()).await.unwrap();
        let result = adaptor.get(&key).await.unwrap();

        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let (_dir, adaptor) = setup();

        // Put some keys with different prefixes
        adaptor
            .put(&ObjectKey::new("chunks/aaa".to_string()), b"a".to_vec())
            .await
            .unwrap();
        adaptor
            .put(&ObjectKey::new("chunks/bbb".to_string()), b"b".to_vec())
            .await
            .unwrap();
        adaptor
            .put(&ObjectKey::new("other/ccc".to_string()), b"c".to_vec())
            .await
            .unwrap();

        let listed = adaptor
            .list(&ObjectKey::new("chunks/".to_string()))
            .await
            .unwrap();

        assert_eq!(listed.len(), 2);
        let keys: Vec<&str> = listed.iter().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"chunks/aaa"));
        assert!(keys.contains(&"chunks/bbb"));
    }

    #[tokio::test]
    async fn test_new_nonexistent_dir_returns_error() {
        let result = LocalFsStorageAdaptor::new("/tmp/nonexistent-cloudless-test-dir-12345");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn test_large_data_roundtrip() {
        let (_dir, adaptor) = setup();
        let key = ObjectKey::new("large-chunk".to_string());
        // 4 MiB of data (matches CloudLess chunk size)
        let data: Vec<u8> = (0..4 * 1024 * 1024).map(|i| (i % 256) as u8).collect();

        adaptor.put(&key, data.clone()).await.unwrap();
        let result = adaptor.get(&key).await.unwrap();

        assert_eq!(result.len(), data.len());
        assert_eq!(result, data);
    }
}
