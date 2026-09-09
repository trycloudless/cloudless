use std::path::PathBuf;

use tokio::sync::mpsc;

pub mod api;
pub mod local_index;

use crate::{
    domain::dek::Dek,
    model::base::{AppResult, Chunk, ChunkMeta, EncryptedData},
};

pub mod storage;

pub trait AsyncChunker {
    fn stream_chunks(&self, path: PathBuf) -> mpsc::Receiver<AppResult<Chunk>>;
    fn stream_chunks_meta(&self, path: PathBuf) -> mpsc::Receiver<AppResult<ChunkMeta>>;
}

pub trait Compressor: Send + Sync {
    fn compress(&self, data: &[u8]) -> AppResult<Vec<u8>>;
    fn decompress(&self, data: &[u8]) -> AppResult<Vec<u8>>;
}

//todo: try removing Send + Sync
pub trait Encryptor: Send + Sync {
    fn encrypt(&self, data: &[u8]) -> AppResult<EncryptedData>;
    fn decrypt(&self, data: &EncryptedData) -> AppResult<Vec<u8>>;
}

#[async_trait::async_trait]
pub trait KeyManager: Send + Sync + 'static {
    async fn load_or_create_dek(&self) -> AppResult<Dek>;
    async fn rotate_kek(&self, new_passphrase: &str) -> AppResult<()>;
    async fn rotate_dek(&self) -> AppResult<()>;
}

#[async_trait::async_trait]
pub trait FileManager: Send + Sync + 'static {
    async fn upload(&self, path: PathBuf) -> AppResult<()>;
    async fn download(&self, path: PathBuf) -> AppResult<()>;
}

#[async_trait::async_trait]
pub trait Api: Send + Sync + 'static {
    async fn get_chunk(&self, file_id: String, chunk_hash: [u8; 32]) -> AppResult<Vec<u8>>;
}
