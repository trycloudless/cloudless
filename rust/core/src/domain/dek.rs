use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use api_types::common::EncryptionAlgorithm;
use rand::{TryRngCore, rngs::OsRng};
use zeroize::Zeroizing;

use crate::{
    domain::kek::Kek,
    model::base::{AppResult, EncryptedData},
};

#[derive(Debug, Clone)]
pub struct Dek {
    pub key: Zeroizing<[u8; 32]>,
}

impl Dek {
    pub fn generate() -> AppResult<Self> {
        let mut key = Zeroizing::new([0u8; 32]);
        OsRng.try_fill_bytes(&mut key[..])?;
        Ok(Self { key })
    }

    pub fn encrypt_dek(&self, kek: &Kek) -> AppResult<EncryptedData> {
        let cipher = Aes256Gcm::new_from_slice(&kek.key[..])?;

        let mut nonce = [0u8; 12];
        OsRng.try_fill_bytes(&mut nonce)?;

        let ciphertext = cipher.encrypt(&Nonce::from_slice(&nonce), self.key.as_ref())?;

        Ok(EncryptedData {
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            nonce: nonce.to_vec(),
            ciphertext,
        })
    }

    pub fn decrypt_dek(kek: &Kek, blob: &EncryptedData) -> AppResult<Self> {
        let cipher = Aes256Gcm::new_from_slice(&kek.key[..])?;

        let plaintext = cipher.decrypt(Nonce::from_slice(&blob.nonce), blob.ciphertext.as_ref())?;

        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&plaintext);

        Ok(Self { key })
    }

    /// Encrypt DEK with a raw 256-bit key (e.g. biometric key).
    /// Unlike `encrypt_dek` which takes a `Kek` (Argon2id-derived), this accepts
    /// any high-entropy 32-byte key directly — no KDF is applied.
    pub fn encrypt_dek_with_raw_key(&self, raw_key: &[u8; 32]) -> AppResult<EncryptedData> {
        let cipher = Aes256Gcm::new_from_slice(raw_key)?;

        let mut nonce = [0u8; 12];
        OsRng.try_fill_bytes(&mut nonce)?;

        let ciphertext = cipher.encrypt(&Nonce::from_slice(&nonce), self.key.as_ref())?;

        Ok(EncryptedData {
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            nonce: nonce.to_vec(),
            ciphertext,
        })
    }

    /// Decrypt DEK with a raw 256-bit key (e.g. biometric key retrieved from OS keychain).
    pub fn decrypt_dek_with_raw_key(raw_key: &[u8; 32], blob: &EncryptedData) -> AppResult<Self> {
        let cipher = Aes256Gcm::new_from_slice(raw_key)?;

        let plaintext = cipher.decrypt(Nonce::from_slice(&blob.nonce), blob.ciphertext.as_ref())?;

        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&plaintext);

        Ok(Self { key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_dek_with_raw_key_round_trip() {
        let dek = Dek::generate().unwrap();
        let mut raw_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut raw_key).unwrap();

        let blob = dek.encrypt_dek_with_raw_key(&raw_key).unwrap();
        let recovered = Dek::decrypt_dek_with_raw_key(&raw_key, &blob).unwrap();

        assert_eq!(dek.key[..], recovered.key[..]);
    }

    #[test]
    fn test_wrong_raw_key_fails_decrypt() {
        let dek = Dek::generate().unwrap();
        let mut raw_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut raw_key).unwrap();

        let blob = dek.encrypt_dek_with_raw_key(&raw_key).unwrap();

        // Use a different key for decryption
        let mut wrong_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut wrong_key).unwrap();

        let result = Dek::decrypt_dek_with_raw_key(&wrong_key, &blob);
        assert!(result.is_err());
    }

    #[test]
    fn test_raw_key_encrypt_produces_different_ciphertext() {
        let dek = Dek::generate().unwrap();
        let mut raw_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut raw_key).unwrap();

        let blob1 = dek.encrypt_dek_with_raw_key(&raw_key).unwrap();
        let blob2 = dek.encrypt_dek_with_raw_key(&raw_key).unwrap();

        // Different random nonces produce different ciphertext
        assert_ne!(blob1.nonce, blob2.nonce);
        assert_ne!(blob1.ciphertext, blob2.ciphertext);
    }
}
