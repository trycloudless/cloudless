use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tracing::error;

use crate::model::base::AppResult;
use crate::model::base::Chunk;
use crate::model::base::ChunkMeta;
use crate::ports::AsyncChunker;

pub struct FixedSizeChunker {
    chunk_size: usize,
}

impl FixedSizeChunker {
    pub fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }
}

impl AsyncChunker for FixedSizeChunker {
    fn stream_chunks(&self, path: PathBuf) -> mpsc::Receiver<AppResult<Chunk>> {
        let chunk_size = self.chunk_size;
        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let result = async {
                let mut file = File::open(&path).await?;
                let mut index: u32 = 0;

                loop {
                    let mut buffer = vec![0u8; chunk_size];
                    let bytes_read = file.read(&mut buffer).await?;

                    if bytes_read == 0 {
                        break;
                    }

                    buffer.truncate(bytes_read);
                    let hash: [u8; 32] = Sha256::digest(&buffer).into();

                    let chunk = Chunk {
                        index,
                        hash,
                        size: bytes_read as u32,
                        data: buffer,
                    };

                    if tx.send(Ok(chunk)).await.is_err() {
                        break;
                    }

                    index += 1;
                }

                Ok::<_, std::io::Error>(())
            }
            .await;

            if let Err(e) = result {
                error!(path = %path.display(), error = %e, "Failed to read file for chunking");
                let _ = tx.send(Err(e.into())).await;
            }
        });

        rx
    }

    fn stream_chunks_meta(&self, path: PathBuf) -> mpsc::Receiver<AppResult<ChunkMeta>> {
        let chunk_size = self.chunk_size;
        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let result = async {
                let mut file = File::open(&path).await?;
                let mut index: u32 = 0;

                loop {
                    let mut buffer = vec![0u8; chunk_size];
                    let bytes_read = file.read(&mut buffer).await?;

                    if bytes_read == 0 {
                        break;
                    }

                    buffer.truncate(bytes_read);
                    let hash: [u8; 32] = Sha256::digest(&buffer).into();

                    let chunk_meta = ChunkMeta {
                        index,
                        hash,
                        size: bytes_read as u32,
                        uploaded_size: 0,
                        deduplicated: false,
                    };

                    if tx.send(Ok(chunk_meta)).await.is_err() {
                        break;
                    }

                    index += 1;
                }

                Ok::<_, std::io::Error>(())
            }
            .await;

            if let Err(e) = result {
                error!(path = %path.display(), error = %e, "Failed to read file for chunk metadata");
                let _ = tx.send(Err(e.into())).await;
            }
        });

        rx
    }
}
