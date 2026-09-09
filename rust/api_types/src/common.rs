use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Base64EncryptedData {
    pub nonce: String,
    pub ciphertext: String,
    pub algorithm: EncryptionAlgorithm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
}
