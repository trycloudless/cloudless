/// File path encryption and blind index computation for zero-trust metadata.
///
/// Encrypts file paths with AES-256-GCM using the metadata key, and computes
/// HMAC-SHA256 blind indexes using the index key for server-side equality lookup
/// without revealing plaintext file names.
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hmac::{Hmac, Mac};
use rand::{TryRngCore, rngs::OsRng};
use sha2::Sha256;

use crate::model::app_error::AppError;
use crate::model::base::AppResult;

type HmacSha256 = Hmac<Sha256>;

/// Encrypted file path with its nonce and blind index.
#[derive(Debug, Clone)]
pub struct EncryptedMetadata {
    pub encrypted_name: Vec<u8>,
    pub nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
}

/// Encrypts a file path with the metadata key (AES-256-GCM) and computes
/// a blind index with the index key (HMAC-SHA256).
pub fn encrypt_file_path(
    path: &str,
    metadata_key: &[u8; 32],
    index_key: &[u8; 32],
) -> AppResult<EncryptedMetadata> {
    let cipher = Aes256Gcm::new_from_slice(metadata_key)?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.try_fill_bytes(&mut nonce_bytes)?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted_name = cipher.encrypt(nonce, path.as_bytes())?;
    let blind_index = compute_blind_index(path, index_key);

    Ok(EncryptedMetadata {
        encrypted_name,
        nonce: nonce_bytes.to_vec(),
        blind_index,
    })
}

/// Decrypts an encrypted file path using the metadata key.
pub fn decrypt_file_path(
    encrypted: &EncryptedMetadata,
    metadata_key: &[u8; 32],
) -> AppResult<String> {
    let cipher = Aes256Gcm::new_from_slice(metadata_key)?;
    let nonce = Nonce::from_slice(&encrypted.nonce);

    let plaintext = cipher.decrypt(nonce, encrypted.encrypted_name.as_ref())?;

    String::from_utf8(plaintext).map_err(|e| AppError::Internal {
        message: format!("Decrypted file path is not valid UTF-8: {e}"),
        source: None,
    })
}

/// Encrypts arbitrary bytes with AES-256-GCM using the metadata key.
/// Returns `(ciphertext, nonce)`. Use this for blobs that do not need a blind index
/// (e.g. encrypted exclusion config).
pub fn encrypt_bytes(data: &[u8], metadata_key: &[u8; 32]) -> AppResult<(Vec<u8>, Vec<u8>)> {
    let cipher = Aes256Gcm::new_from_slice(metadata_key)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.try_fill_bytes(&mut nonce_bytes)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, data)?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decrypts bytes previously encrypted with [`encrypt_bytes`].
pub fn decrypt_bytes(
    ciphertext: &[u8],
    nonce: &[u8],
    metadata_key: &[u8; 32],
) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(metadata_key)?;
    let nonce = Nonce::from_slice(nonce);
    Ok(cipher.decrypt(nonce, ciphertext)?)
}

/// Computes a blind index (HMAC-SHA256) for server-side equality lookup.
pub fn compute_blind_index(path: &str, index_key: &[u8; 32]) -> Vec<u8> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(index_key).expect("HMAC accepts any key size");
    mac.update(path.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    use crate::domain::dek::Dek;
    use crate::domain::derived_keys::DerivedKeys;

    fn test_keys() -> DerivedKeys {
        let dek = Dek {
            key: Zeroizing::new([42u8; 32]),
        };
        DerivedKeys::derive(&dek).unwrap()
    }

    #[test]
    fn round_trip_encrypt_decrypt() {
        let keys = test_keys();
        let path = "/home/user/documents/report.pdf";

        let encrypted = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        let decrypted = decrypt_file_path(&encrypted, &keys.metadata_key).unwrap();

        assert_eq!(decrypted, path);
    }

    #[test]
    fn blind_index_is_deterministic() {
        let keys = test_keys();
        let path = "/home/user/documents/report.pdf";

        let index1 = compute_blind_index(path, &keys.index_key);
        let index2 = compute_blind_index(path, &keys.index_key);

        assert_eq!(index1, index2);
    }

    #[test]
    fn different_paths_produce_different_blind_indexes() {
        let keys = test_keys();

        let index1 = compute_blind_index("/path/a.txt", &keys.index_key);
        let index2 = compute_blind_index("/path/b.txt", &keys.index_key);

        assert_ne!(index1, index2);
    }

    #[test]
    fn different_keys_produce_different_blind_indexes() {
        let dek1 = Dek {
            key: Zeroizing::new([1u8; 32]),
        };
        let dek2 = Dek {
            key: Zeroizing::new([2u8; 32]),
        };
        let keys1 = DerivedKeys::derive(&dek1).unwrap();
        let keys2 = DerivedKeys::derive(&dek2).unwrap();
        let path = "/same/path.txt";

        let index1 = compute_blind_index(path, &keys1.index_key);
        let index2 = compute_blind_index(path, &keys2.index_key);

        assert_ne!(index1, index2);
    }

    #[test]
    fn nonce_uniqueness_across_encryptions() {
        let keys = test_keys();
        let path = "/same/file.txt";

        let enc1 = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        let enc2 = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();

        // Different nonces → different ciphertexts
        assert_ne!(enc1.nonce, enc2.nonce);
        assert_ne!(enc1.encrypted_name, enc2.encrypted_name);
        // But same blind index
        assert_eq!(enc1.blind_index, enc2.blind_index);
    }

    #[test]
    fn blind_index_is_32_bytes() {
        let keys = test_keys();
        let index = compute_blind_index("/any/path", &keys.index_key);
        assert_eq!(index.len(), 32);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let keys = test_keys();
        let wrong_key = [0u8; 32];
        let path = "/secret/file.txt";

        let encrypted = encrypt_file_path(path, &keys.metadata_key, &keys.index_key).unwrap();
        let result = decrypt_file_path(&encrypted, &wrong_key);

        assert!(result.is_err());
    }
}
