use api_types::common::{Base64EncryptedData, EncryptionAlgorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::app_error::AppError;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: u32,
    pub hash: [u8; 32],
    pub size: u32,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn hash_string(&self) -> String {
        hex::encode(self.hash)
    }

    pub fn decode_hash_string(hash_string: &str) -> AppResult<[u8; 32]> {
        let decoded_hash: Vec<u8> = hex::decode(hash_string).map_err(|e| AppError::Internal {
            message: "failed to decode hash string".into(),
            source: Some(e.into()),
        })?;
        decoded_hash.try_into().map_err(|_| AppError::Internal {
            message: "failed to convert decoded hash to array".into(),
            source: None,
        })
    }
}

#[derive(Debug)]
pub struct ChunkMeta {
    pub index: u32,
    pub hash: [u8; 32],
    pub size: u32,
    /// Actual bytes uploaded to storage (after compression+encryption). 0 if deduplicated.
    pub uploaded_size: u32,
    /// Whether this chunk was skipped because it already existed in storage.
    pub deduplicated: bool,
}

/// Events streamed out of `backup_file` as it progresses. `VersionCreated` is
/// always sent first (before any chunk work starts) so the caller learns the
/// server-assigned file version id/number to record in the local index.
#[derive(Debug)]
pub enum BackupFileEvent {
    VersionCreated { file_version_id: Uuid, version: u32 },
    Chunk(ChunkMeta),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub algorithm: EncryptionAlgorithm,
}

impl From<EncryptedData> for Base64EncryptedData {
    fn from(value: EncryptedData) -> Self {
        use base64::{Engine, engine::general_purpose::STANDARD};
        Self {
            nonce: STANDARD.encode(&value.nonce),
            ciphertext: STANDARD.encode(&value.ciphertext),
            algorithm: value.algorithm,
        }
    }
}

impl TryFrom<Base64EncryptedData> for EncryptedData {
    type Error = AppError;

    fn try_from(value: Base64EncryptedData) -> Result<Self, Self::Error> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let nonce = STANDARD
            .decode(&value.nonce)
            .map_err(|e| AppError::Internal {
                message: "failed to decode base64 nonce".into(),
                source: Some(e.into()),
            })?;
        let ciphertext = STANDARD
            .decode(&value.ciphertext)
            .map_err(|e| AppError::Internal {
                message: "failed to decode base64 ciphertext".into(),
                source: Some(e.into()),
            })?;
        Ok(Self {
            nonce,
            ciphertext,
            algorithm: value.algorithm,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedKey {
    pub version: u32,
    pub algorithm: EncryptionAlgorithm,
    pub encrypted_key: Vec<u8>,
    pub salt: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessedData {
    Raw(Vec<u8>),
    Compressed(Vec<u8>),
    Encrypted(EncryptedData),
}

impl ProcessedData {
    pub fn bytes(&self) -> &[u8] {
        match self {
            ProcessedData::Raw(data) => data,
            ProcessedData::Compressed(data) => data,
            ProcessedData::Encrypted(data) => &data.ciphertext,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedChunk {
    pub index: u32,
    pub hash: [u8; 32],
    pub size: u32,
    pub data: ProcessedData,
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct EncryptedBlob {
//     pub algorithm: EncryptionAlgorithm,
//     pub nonce: Vec<u8>,
//     pub ciphertext: Vec<u8>,
// }
