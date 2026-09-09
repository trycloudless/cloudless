use aes_gcm::{KeyInit, aead::Aead};
use api_types::common::EncryptionAlgorithm;
use rand::RngCore;

use crate::{
    domain::dek::Dek,
    model::base::{AppResult, EncryptedData},
    ports::Encryptor,
};

pub struct AesGcmEncryptor {
    cipher: aes_gcm::Aes256Gcm,
}

impl AesGcmEncryptor {
    pub fn new(dek: &Dek) -> AppResult<Self> {
        Ok(Self {
            cipher: aes_gcm::Aes256Gcm::new_from_slice(&dek.key[..])?,
        })
    }
}

impl Encryptor for AesGcmEncryptor {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<EncryptedData> {
        let mut nonce = vec![0u8; 12];
        rand::rng().fill_bytes(&mut nonce);

        let ciphertext = self
            .cipher
            .encrypt(aes_gcm::Nonce::from_slice(&nonce), plaintext)?;

        Ok(EncryptedData {
            nonce,
            ciphertext,
            algorithm: EncryptionAlgorithm::Aes256Gcm,
        })
    }

    fn decrypt(&self, data: &EncryptedData) -> AppResult<Vec<u8>> {
        self.cipher
            .decrypt(
                aes_gcm::Nonce::from_slice(&data.nonce),
                data.ciphertext.as_ref(),
            )
            .map_err(Into::into)
    }
}
