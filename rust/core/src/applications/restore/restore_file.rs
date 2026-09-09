use std::path::PathBuf;

use api_types::{chunk::ChunkStorageMeta, restore_file_info::RestoreChunkInfo};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{
    adapters::{aes_gcm_encryptor::AesGcmEncryptor, zstd_compressor::ZstdCompressor},
    domain::dek::Dek,
    model::base::AppResult,
    model::file::ObjectKey,
    ports::{Compressor, Encryptor, storage::StoragePort},
};

/// Metadata about a restored chunk, sent through the mpsc channel for progress tracking.
#[derive(Debug)]
pub struct RestoreChunkMeta {
    pub index: i32,
    pub size: u32,
}

/// Restores a single file by downloading, decrypting, decompressing, and reassembling its chunks.
///
/// This is the reverse of `backup_file`. For each chunk (ordered by chunk_index):
/// 1. Downloads the encrypted ciphertext from remote storage using the chunk's storage key.
/// 2. Decrypts with AES-256-GCM using the provided DEK.
/// 3. Decompresses with zstd.
/// 4. Verifies the SHA-256 hash of the decompressed data matches the original chunk hash.
///    If the hash doesn't match, the restore is aborted for this file.
/// 5. Streams `RestoreChunkMeta` back through an mpsc channel for progress tracking.
///
/// After all chunks are processed, the file is reassembled and written atomically
/// (write to a temp file then rename) to prevent partial files on failure.
///
/// Note: integrity is verified per-chunk (SHA-256). A full-file hash is not stored
/// in the current metadata schema, so no file-level verification is performed.
///
/// The function spawns a background tokio task and returns a receiver immediately.
#[tracing::instrument(skip(storage, dek, chunks_info), fields(dest = %dest_path.display(), num_chunks = chunks_info.len()))]
pub fn restore_file<S>(
    storage: S,
    chunks_info: Vec<RestoreChunkInfo>,
    dest_path: PathBuf,
    dek: Dek,
) -> mpsc::Receiver<AppResult<RestoreChunkMeta>>
where
    S: StoragePort + 'static,
{
    let (tx, rx) = mpsc::channel(16);

    tokio::spawn(async move {
        let encryptor = match AesGcmEncryptor::new(&dek) {
            Ok(enc) => enc,
            Err(e) => {
                error!(error = %e, "Failed to create encryptor for restore");
                let _ = tx.send(Err(e)).await;
                return;
            }
        };
        let compressor = ZstdCompressor::new(4);

        // Collect all decompressed chunk data in order for final reassembly and hash verification.
        let mut all_chunk_data: Vec<Vec<u8>> = Vec::with_capacity(chunks_info.len());
        info!(num_chunks = chunks_info.len(), dest = %dest_path.display(), "Starting file restore");

        for chunk_info in &chunks_info {
            debug!(chunk_index = chunk_info.chunk_index, "Restoring chunk");
            let result = restore_single_chunk(&storage, &encryptor, &compressor, chunk_info).await;

            match result {
                Ok((decompressed_data, meta)) => {
                    debug!(
                        chunk_index = meta.index,
                        size = meta.size,
                        "Chunk restored successfully"
                    );
                    all_chunk_data.push(decompressed_data);
                    if tx.send(Ok(meta)).await.is_err() {
                        return;
                    }
                }
                Err(e) => {
                    error!(chunk_index = chunk_info.chunk_index, error = %e, "Chunk restore failed");
                    let _ = tx.send(Err(e)).await;
                    return;
                }
            }
        }

        // Reassemble the complete file from chunks and write atomically.
        info!(dest = %dest_path.display(), num_chunks = all_chunk_data.len(), "Writing restored file");
        if let Err(e) = write_file_atomically(&dest_path, &all_chunk_data).await {
            error!(error = %e, "Failed to write restored file");
            let _ = tx.send(Err(e)).await;
        }
    });

    rx
}

/// Downloads, decrypts, decompresses, and hash-verifies a single chunk.
///
/// Returns the decompressed data (for reassembly) and a RestoreChunkMeta for progress tracking.
/// Errors if the download fails, decryption fails, decompression fails, or the hash doesn't match.
pub(crate) async fn restore_single_chunk(
    storage: &dyn StoragePort,
    encryptor: &AesGcmEncryptor,
    compressor: &ZstdCompressor,
    chunk_info: &RestoreChunkInfo,
) -> AppResult<(Vec<u8>, RestoreChunkMeta)> {
    // Get the storage key from the chunk's storage metadata
    let storage_key = match &chunk_info.storage_meta {
        ChunkStorageMeta::ObjectStore(s3_meta) => ObjectKey::new(s3_meta.key.clone()),
    };

    // Download encrypted ciphertext from remote storage.
    let ciphertext = storage.get(&storage_key).await?;

    // Retrieve the nonce and algorithm from the chunk's stored encryption metadata.
    let encryption_meta = match &chunk_info.storage_meta {
        ChunkStorageMeta::ObjectStore(s3_meta) => s3_meta.encryption.as_ref().ok_or_else(|| {
            crate::model::app_error::AppError::Internal {
                message: format!(
                    "Chunk {} has no encryption metadata — cannot decrypt",
                    chunk_info.chunk_index
                ),
                source: None,
            }
        })?,
    };

    let encrypted_data = crate::model::base::EncryptedData {
        nonce: encryption_meta.nonce.clone(),
        ciphertext,
        algorithm: encryption_meta.algorithm.clone(),
    };

    // Decrypt
    let compressed_data = encryptor.decrypt(&encrypted_data)?;

    // Decompress
    let decompressed_data = compressor.decompress(&compressed_data)?;

    // Verify SHA-256 hash matches the original chunk hash.
    // This ensures data integrity: the chunk we downloaded is exactly what was originally backed up.
    let mut hasher = Sha256::new();
    hasher.update(&decompressed_data);
    let computed_hash: [u8; 32] = hasher.finalize().into();

    if computed_hash[..] != chunk_info.hash[..] {
        return Err(crate::model::app_error::AppError::Internal {
            message: format!(
                "Chunk {} hash mismatch: expected {}, got {}",
                chunk_info.chunk_index,
                hex::encode(&chunk_info.hash),
                hex::encode(computed_hash)
            ),
            source: None,
        });
    }

    let meta = RestoreChunkMeta {
        index: chunk_info.chunk_index,
        size: decompressed_data.len() as u32,
    };

    Ok((decompressed_data, meta))
}

/// Writes reassembled chunk data to the destination path atomically.
///
/// First writes to a temporary file (`<dest>.cloudless.tmp`), then renames to the
/// final path. This prevents partial files if the process is interrupted during write.
/// Also creates parent directories if they don't exist. Cleans up the temp file on
/// failure.
async fn write_file_atomically(dest_path: &PathBuf, chunks: &[Vec<u8>]) -> AppResult<()> {
    // Create parent directories if needed
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Append `.cloudless.tmp` instead of replacing the extension, so
    // `file.tar.gz` becomes `file.tar.gz.cloudless.tmp` (not `file.tar.tmp`).
    let mut tmp_name = dest_path.as_os_str().to_os_string();
    tmp_name.push(".cloudless.tmp");
    let tmp_path = PathBuf::from(tmp_name);

    // Write all chunk data to the temporary file
    let mut file_data = Vec::new();
    for chunk in chunks {
        file_data.extend_from_slice(chunk);
    }

    tokio::fs::write(&tmp_path, &file_data).await?;

    // Atomic rename from tmp to final destination
    if let Err(e) = tokio::fs::rename(&tmp_path, dest_path).await {
        // Clean up temp file on rename failure
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(e.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::{aes_gcm_encryptor::AesGcmEncryptor, zstd_compressor::ZstdCompressor},
        model::file::ObjectKey,
        ports::{Compressor, Encryptor, storage::StoragePort},
    };
    use api_types::{
        chunk::{ChunkEncryptionMeta, ChunkObjectStoreMeta, ChunkStorageMeta},
        common::EncryptionAlgorithm,
        restore_file_info::RestoreChunkInfo,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory storage mock that returns pre-loaded data by key.
    struct MockStorage {
        data: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, key: &str, value: Vec<u8>) {
            self.data.lock().unwrap().insert(key.to_string(), value);
        }
    }

    #[async_trait]
    impl StoragePort for MockStorage {
        async fn put(&self, _key: &ObjectKey, _data: Vec<u8>) -> AppResult<()> {
            unimplemented!()
        }
        async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
            let data = self.data.lock().unwrap();
            data.get(key.as_str()).cloned().ok_or_else(|| {
                crate::model::app_error::AppError::Internal {
                    message: format!("key not found: {}", key.as_str()),
                    source: None,
                }
            })
        }
        async fn delete(&self, _key: &ObjectKey) -> AppResult<()> {
            unimplemented!()
        }
        async fn list(&self, _prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
            unimplemented!()
        }
        async fn exists(&self, _key: &ObjectKey) -> AppResult<bool> {
            unimplemented!()
        }
    }

    /// Helper: creates a DEK, encrypts and compresses raw data, returns
    /// (ciphertext, nonce, dek) suitable for building RestoreChunkInfo.
    fn prepare_chunk_data(raw_data: &[u8]) -> (Vec<u8>, Vec<u8>, Dek) {
        let dek = Dek::generate().expect("DEK generation should succeed");
        let encryptor = AesGcmEncryptor::new(&dek).expect("encryptor creation should succeed");
        let compressor = ZstdCompressor::new(4);

        let compressed = compressor
            .compress(raw_data)
            .expect("compression should succeed");
        let encrypted = encryptor
            .encrypt(&compressed)
            .expect("encryption should succeed");

        let nonce = encrypted.nonce.clone();
        let ciphertext = encrypted.ciphertext;

        (ciphertext, nonce, dek)
    }

    /// Tests that restore_file successfully restores a single-chunk file.
    #[tokio::test]
    async fn test_restore_file_single_chunk_success() {
        let raw_data = b"Hello, Cloudless! This is test data for restore.";
        let (ciphertext, nonce, dek) = prepare_chunk_data(raw_data);

        // Compute expected hash of raw data
        let mut hasher = Sha256::new();
        hasher.update(raw_data);
        let hash: Vec<u8> = hasher.finalize().to_vec();

        let storage = MockStorage::new();
        storage.insert("chunks/test-chunk-1", ciphertext);

        let chunk_info = RestoreChunkInfo {
            chunk_id: uuid::Uuid::now_v7(),
            chunk_index: 0,
            hash,
            size: raw_data.len() as i64,
            storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                key: "chunks/test-chunk-1".into(),
                hash: "unused".into(),
                encryption: Some(ChunkEncryptionMeta {
                    nonce,
                    algorithm: EncryptionAlgorithm::Aes256Gcm,
                }),
            }),
        };

        let dest_dir = tempfile::tempdir().expect("failed to create temp dir");
        let dest_path = dest_dir.path().join("restored_file.txt");

        let mut rx = restore_file(storage, vec![chunk_info], dest_path.clone(), dek);

        let mut received_metas = Vec::new();
        while let Some(result) = rx.recv().await {
            match result {
                Ok(meta) => received_metas.push(meta),
                Err(e) => panic!("restore_file returned error: {e}"),
            }
        }

        assert_eq!(received_metas.len(), 1);
        assert_eq!(received_metas[0].index, 0);
        assert_eq!(received_metas[0].size, raw_data.len() as u32);

        // Verify file was written correctly
        let restored = tokio::fs::read(&dest_path)
            .await
            .expect("failed to read restored file");
        assert_eq!(restored, raw_data);
    }

    /// Tests that restore_file correctly reassembles a multi-chunk file.
    #[tokio::test]
    async fn test_restore_file_multi_chunk_success() {
        let chunk1_data = b"First chunk of data. ";
        let chunk2_data = b"Second chunk of data.";
        let dek = Dek::generate().expect("DEK generation should succeed");
        let encryptor = AesGcmEncryptor::new(&dek).expect("encryptor creation should succeed");
        let compressor = ZstdCompressor::new(4);

        let storage = MockStorage::new();
        let mut chunks_info = Vec::new();

        for (i, raw) in [chunk1_data.as_slice(), chunk2_data.as_slice()]
            .iter()
            .enumerate()
        {
            let compressed = compressor.compress(raw).expect("compress");
            let encrypted = encryptor.encrypt(&compressed).expect("encrypt");

            let mut hasher = Sha256::new();
            hasher.update(raw);
            let hash: Vec<u8> = hasher.finalize().to_vec();

            let key = format!("chunks/multi-{i}");
            storage.insert(&key, encrypted.ciphertext.clone());

            chunks_info.push(RestoreChunkInfo {
                chunk_id: uuid::Uuid::now_v7(),
                chunk_index: i as i32,
                hash,
                size: raw.len() as i64,
                storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                    key,
                    hash: "unused".into(),
                    encryption: Some(ChunkEncryptionMeta {
                        nonce: encrypted.nonce,
                        algorithm: EncryptionAlgorithm::Aes256Gcm,
                    }),
                }),
            });
        }

        let dest_dir = tempfile::tempdir().expect("failed to create temp dir");
        let dest_path = dest_dir.path().join("multi_chunk_restored.txt");

        let mut rx = restore_file(storage, chunks_info, dest_path.clone(), dek);

        let mut count = 0;
        while let Some(result) = rx.recv().await {
            result.expect("chunk restore should succeed");
            count += 1;
        }
        assert_eq!(count, 2);

        let restored = tokio::fs::read(&dest_path).await.expect("failed to read");
        let mut expected = Vec::new();
        expected.extend_from_slice(chunk1_data);
        expected.extend_from_slice(chunk2_data);
        assert_eq!(restored, expected);
    }

    /// Tests that restore_file returns an error when storage download fails.
    #[tokio::test]
    async fn test_restore_file_storage_error() {
        let dek = Dek::generate().expect("DEK generation should succeed");
        let storage = MockStorage::new(); // Empty — no data stored

        let chunk_info = RestoreChunkInfo {
            chunk_id: uuid::Uuid::now_v7(),
            chunk_index: 0,
            hash: vec![0u8; 32],
            size: 100,
            storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                key: "chunks/nonexistent".into(),
                hash: "unused".into(),
                encryption: Some(ChunkEncryptionMeta {
                    nonce: vec![0u8; 12],
                    algorithm: EncryptionAlgorithm::Aes256Gcm,
                }),
            }),
        };

        let dest_dir = tempfile::tempdir().expect("failed to create temp dir");
        let dest_path = dest_dir.path().join("should_not_exist.txt");

        let mut rx = restore_file(storage, vec![chunk_info], dest_path.clone(), dek);

        let result = rx
            .recv()
            .await
            .expect("should receive at least one message");
        assert!(result.is_err(), "Expected error for missing storage key");
        assert!(!dest_path.exists(), "File should not be written on error");
    }

    /// Tests that restore_file detects hash mismatch after decryption.
    #[tokio::test]
    async fn test_restore_file_hash_mismatch() {
        let raw_data = b"Correct data";
        let (ciphertext, nonce, dek) = prepare_chunk_data(raw_data);

        let storage = MockStorage::new();
        storage.insert("chunks/hash-mismatch", ciphertext);

        // Provide wrong hash (all zeros instead of actual SHA-256)
        let chunk_info = RestoreChunkInfo {
            chunk_id: uuid::Uuid::now_v7(),
            chunk_index: 0,
            hash: vec![0u8; 32], // Wrong hash
            size: raw_data.len() as i64,
            storage_meta: ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                key: "chunks/hash-mismatch".into(),
                hash: "unused".into(),
                encryption: Some(ChunkEncryptionMeta {
                    nonce,
                    algorithm: EncryptionAlgorithm::Aes256Gcm,
                }),
            }),
        };

        let dest_dir = tempfile::tempdir().expect("failed to create temp dir");
        let dest_path = dest_dir.path().join("hash_mismatch.txt");

        let mut rx = restore_file(storage, vec![chunk_info], dest_path.clone(), dek);

        let result = rx.recv().await.expect("should receive a message");
        assert!(result.is_err(), "Expected hash mismatch error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("hash mismatch"),
            "Error should mention hash mismatch, got: {err_msg}"
        );
    }

    /// Tests that RestoreChunkMeta fields are correctly populated.
    #[test]
    fn test_restore_chunk_meta_construction() {
        let meta = RestoreChunkMeta {
            index: 5,
            size: 4096,
        };
        assert_eq!(meta.index, 5);
        assert_eq!(meta.size, 4096);
    }
}
