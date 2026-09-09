use std::path::PathBuf;
use std::sync::Arc;

use crate::model::base::{AppResult, ProcessedChunk, ProcessedData};
use crate::ports::{AsyncChunker, Compressor, Encryptor};

pub struct FileService<C: AsyncChunker, Co: Compressor, E: Encryptor> {
    chunker: Arc<C>,
    compressor: Option<Arc<Co>>,
    encryptor: Option<Arc<E>>,
}

impl<C: AsyncChunker, Co: Compressor, E: Encryptor> FileService<C, Co, E> {
    pub fn new(chunker: C, compressor: Option<Co>, encryptor: Option<E>) -> Self {
        Self {
            chunker: Arc::new(chunker),
            compressor: compressor.map(Arc::new),
            encryptor: encryptor.map(Arc::new),
        }
    }

    pub async fn process_chunks(&self, path: PathBuf) -> AppResult<Vec<ProcessedChunk>> {
        let mut rx = self.chunker.stream_chunks(path);

        let mut processed_chunks = Vec::new();

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            let mut data = ProcessedData::Raw(chunk.data);

            if let Some(compressor) = &self.compressor {
                let compressed = compressor.compress(data.bytes())?;
                data = ProcessedData::Compressed(compressed);
            }

            if let Some(encryptor) = &self.encryptor {
                let encrypted = encryptor.encrypt(data.bytes())?;
                data = ProcessedData::Encrypted(encrypted);
            }

            processed_chunks.push(ProcessedChunk {
                index: chunk.index,
                hash: chunk.hash,
                size: chunk.size,
                data,
            });
        }

        Ok(processed_chunks)
    }

    pub async fn upload(&self, path: std::path::PathBuf) -> AppResult<()> {
        // 1. Check if file is already uploaded
        // 2. Read file
        // 3. Chunk file
        // 4. Compress chunks
        // 5. Encrypt chunks
        // 6. Upload chunks
        // 7. Update file metadata
        let _chunks = self.process_chunks(path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use api_types::common::EncryptionAlgorithm;
    use tokio::sync::mpsc;

    use crate::model::base::{Chunk, ChunkMeta, EncryptedData};

    use super::*;

    struct MockChunker;
    impl AsyncChunker for MockChunker {
        fn stream_chunks(&self, _path: PathBuf) -> mpsc::Receiver<AppResult<Chunk>> {
            let (tx, rx) = mpsc::channel(1);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok(Chunk {
                        index: 0,
                        hash: [0; 32],
                        size: 5,
                        data: b"hello".to_vec(),
                    }))
                    .await;
            });
            rx
        }
        fn stream_chunks_meta(&self, _path: PathBuf) -> mpsc::Receiver<AppResult<ChunkMeta>> {
            let (_tx, rx) = mpsc::channel(1);
            rx
        }
    }

    struct MockCompressor;
    impl Compressor for MockCompressor {
        fn compress(&self, data: &[u8]) -> AppResult<Vec<u8>> {
            let mut compressed = data.to_vec();
            compressed.push(b'C');
            Ok(compressed)
        }
        fn decompress(&self, data: &[u8]) -> AppResult<Vec<u8>> {
            Ok(data[..data.len() - 1].to_vec())
        }
    }

    struct MockEncryptor;
    impl Encryptor for MockEncryptor {
        fn encrypt(&self, data: &[u8]) -> AppResult<EncryptedData> {
            let mut ciphertext = data.to_vec();
            ciphertext.push(b'E');
            Ok(EncryptedData {
                nonce: vec![0; 12],
                ciphertext,
                algorithm: EncryptionAlgorithm::Aes256Gcm,
            })
        }
        fn decrypt(&self, _data: &EncryptedData) -> AppResult<Vec<u8>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_process_chunks_logic() -> AppResult<()> {
        let chunker = MockChunker;
        let compressor = MockCompressor;
        let encryptor = MockEncryptor;

        let service = FileService::new(chunker, Some(compressor), Some(encryptor));
        let chunks = service.process_chunks(PathBuf::from("test")).await?;

        assert_eq!(chunks.len(), 1);
        let chunk = &chunks[0];
        assert_eq!(chunk.index, 0);

        if let ProcessedData::Encrypted(enc) = &chunk.data {
            // "hello" + 'C' + 'E'
            assert_eq!(enc.ciphertext, b"helloCE");
        } else {
            panic!("Expected encrypted data");
        }

        Ok(())
    }
}
