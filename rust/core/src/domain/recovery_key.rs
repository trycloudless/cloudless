/// Recovery key domain: generation, encoding, HKDF derivation, and DEK encryption.
///
/// The recovery key allows users to decrypt their DEK if they forget their password.
///
/// Flow:
///   RS (Recovery Secret) = random 256-bit
///   RK (Recovery Key)    = HKDF-SHA256(RS, info="cloudless-recovery-v1")
///   encrypted_DEK        = AES-256-GCM(RK, DEK)
///
/// Display format: CLRK-XXXX-XXXX-... (Base32 with 2-byte SHA-256 checksum)
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use api_types::common::EncryptionAlgorithm;
use data_encoding::BASE32_NOPAD;
use hkdf::Hkdf;
use rand::{TryRngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    domain::dek::Dek,
    model::base::{AppResult, EncryptedData},
};

const RECOVERY_INFO: &[u8] = b"cloudless-recovery-v1";
const RECOVERY_PREFIX: &str = "CLRK";
const CHECKSUM_LEN: usize = 2;

/// The raw 256-bit secret from which the recovery key is derived.
/// This is what the user sees (encoded as a Base32 string with checksum).
pub struct RecoverySecret {
    pub bytes: Zeroizing<[u8; 32]>,
}

/// The derived 256-bit key used to encrypt/decrypt the DEK.
/// Never shown to the user — only exists transiently in memory.
pub struct RecoveryKey {
    pub key: Zeroizing<[u8; 32]>,
}

impl RecoverySecret {
    /// Generate a new random 256-bit recovery secret using OS entropy.
    pub fn generate() -> AppResult<Self> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        OsRng.try_fill_bytes(&mut bytes[..])?;
        Ok(Self { bytes })
    }

    /// Encode the recovery secret as a human-readable display string.
    ///
    /// Format: `CLRK-XXXX-XXXX-...` where the payload is Base32(RS || checksum).
    /// Checksum = first 2 bytes of SHA-256(RS).
    pub fn to_display_string(&self) -> String {
        let checksum = compute_checksum(&self.bytes[..]);
        let mut payload = Vec::with_capacity(32 + CHECKSUM_LEN);
        payload.extend_from_slice(&self.bytes[..]);
        payload.extend_from_slice(&checksum);

        let encoded = BASE32_NOPAD.encode(&payload);
        format_display_string(&encoded)
    }

    /// Parse a display string back into a RecoverySecret, verifying the checksum.
    pub fn from_display_string(s: &str) -> AppResult<Self> {
        let cleaned: String = s
            .chars()
            .filter(|c| *c != '-' && !c.is_whitespace())
            .collect::<String>()
            .to_uppercase();

        if !cleaned.starts_with(RECOVERY_PREFIX) {
            return Err(crate::model::app_error::AppError::Internal {
                message: "Recovery key must start with CLRK prefix".to_string(),
                source: None,
            });
        }

        let base32_part = &cleaned[RECOVERY_PREFIX.len()..];
        let decoded = BASE32_NOPAD.decode(base32_part.as_bytes()).map_err(|e| {
            crate::model::app_error::AppError::Internal {
                message: format!("Invalid Base32 encoding in recovery key: {e}"),
                source: None,
            }
        })?;

        if decoded.len() != 32 + CHECKSUM_LEN {
            return Err(crate::model::app_error::AppError::Internal {
                message: format!(
                    "Invalid recovery key length: expected {} bytes, got {}",
                    32 + CHECKSUM_LEN,
                    decoded.len()
                ),
                source: None,
            });
        }

        let (rs_bytes, checksum_bytes) = decoded.split_at(32);
        let expected_checksum = compute_checksum(rs_bytes);
        if checksum_bytes != expected_checksum {
            return Err(crate::model::app_error::AppError::Internal {
                message: "Recovery key checksum verification failed".to_string(),
                source: None,
            });
        }

        let mut bytes = Zeroizing::new([0u8; 32]);
        bytes.copy_from_slice(rs_bytes);
        Ok(Self { bytes })
    }
}

impl RecoveryKey {
    /// Derive a recovery key from a recovery secret using HKDF-SHA256.
    pub fn derive(rs: &RecoverySecret) -> AppResult<Self> {
        let hkdf = Hkdf::<Sha256>::new(None, &rs.bytes[..]);

        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(RECOVERY_INFO, &mut key[..]).map_err(|e| {
            crate::model::app_error::AppError::Internal {
                message: format!("HKDF expand failed for recovery key: {e}"),
                source: None,
            }
        })?;

        Ok(Self { key })
    }

    /// Encrypt a DEK with this recovery key using AES-256-GCM.
    pub fn encrypt_dek(&self, dek: &Dek) -> AppResult<EncryptedData> {
        let cipher = Aes256Gcm::new_from_slice(&self.key[..])?;

        let mut nonce = [0u8; 12];
        OsRng.try_fill_bytes(&mut nonce)?;

        let ciphertext = cipher.encrypt(&Nonce::from_slice(&nonce), dek.key.as_ref())?;

        Ok(EncryptedData {
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            nonce: nonce.to_vec(),
            ciphertext,
        })
    }

    /// Decrypt a DEK from an encrypted blob using this recovery key.
    pub fn decrypt_dek(&self, blob: &EncryptedData) -> AppResult<Dek> {
        let cipher = Aes256Gcm::new_from_slice(&self.key[..])?;

        let plaintext = cipher.decrypt(Nonce::from_slice(&blob.nonce), blob.ciphertext.as_ref())?;

        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&plaintext);

        Ok(Dek { key })
    }
}

/// Compute 2-byte checksum: first 2 bytes of SHA-256(data).
fn compute_checksum(data: &[u8]) -> [u8; CHECKSUM_LEN] {
    let hash = Sha256::digest(data);
    let mut checksum = [0u8; CHECKSUM_LEN];
    checksum.copy_from_slice(&hash[..CHECKSUM_LEN]);
    checksum
}

/// Format a Base32 string with CLRK prefix and hyphen-separated 4-char groups.
fn format_display_string(base32: &str) -> String {
    let mut result =
        String::with_capacity(RECOVERY_PREFIX.len() + 1 + base32.len() + base32.len() / 4);
    result.push_str(RECOVERY_PREFIX);

    for (i, ch) in base32.chars().enumerate() {
        if i % 4 == 0 {
            result.push('-');
        }
        result.push(ch);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_32_bytes() {
        let rs = RecoverySecret::generate().unwrap();
        assert_eq!(rs.bytes.len(), 32);
    }

    #[test]
    fn display_string_round_trip() {
        let rs = RecoverySecret::generate().unwrap();
        let display = rs.to_display_string();
        let parsed = RecoverySecret::from_display_string(&display).unwrap();
        assert_eq!(rs.bytes[..], parsed.bytes[..]);
    }

    #[test]
    fn display_string_has_clrk_prefix() {
        let rs = RecoverySecret::generate().unwrap();
        let display = rs.to_display_string();
        assert!(display.starts_with("CLRK-"));
    }

    #[test]
    fn invalid_checksum_rejected() {
        let rs = RecoverySecret::generate().unwrap();
        let mut display = rs.to_display_string();

        // Flip last character to corrupt the checksum
        let last = display.pop().unwrap();
        let replacement = if last == 'A' { 'B' } else { 'A' };
        display.push(replacement);

        let result = RecoverySecret::from_display_string(&display);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_prefix_rejected() {
        let rs = RecoverySecret::generate().unwrap();
        let display = rs.to_display_string();
        let bad = display.replacen("CLRK", "XXXX", 1);

        let result = RecoverySecret::from_display_string(&bad);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_base32_rejected() {
        let result = RecoverySecret::from_display_string("CLRK-!@#$-5678");
        assert!(result.is_err());
    }

    #[test]
    fn too_short_rejected() {
        let result = RecoverySecret::from_display_string("CLRK-ABCD");
        assert!(result.is_err());
    }

    #[test]
    fn derive_is_deterministic() {
        let rs = RecoverySecret {
            bytes: Zeroizing::new([42u8; 32]),
        };
        let rk1 = RecoveryKey::derive(&rs).unwrap();
        let rk2 = RecoveryKey::derive(&rs).unwrap();
        assert_eq!(rk1.key[..], rk2.key[..]);
    }

    #[test]
    fn different_rs_different_rk() {
        let rs1 = RecoverySecret {
            bytes: Zeroizing::new([1u8; 32]),
        };
        let rs2 = RecoverySecret {
            bytes: Zeroizing::new([2u8; 32]),
        };
        let rk1 = RecoveryKey::derive(&rs1).unwrap();
        let rk2 = RecoveryKey::derive(&rs2).unwrap();
        assert_ne!(rk1.key[..], rk2.key[..]);
    }

    #[test]
    fn encrypt_decrypt_dek_round_trip() {
        let dek = Dek::generate().unwrap();
        let rs = RecoverySecret::generate().unwrap();
        let rk = RecoveryKey::derive(&rs).unwrap();

        let encrypted = rk.encrypt_dek(&dek).unwrap();
        let decrypted = rk.decrypt_dek(&encrypted).unwrap();

        assert_eq!(dek.key[..], decrypted.key[..]);
    }

    #[test]
    fn wrong_rk_fails_decrypt() {
        let dek = Dek::generate().unwrap();
        let rs1 = RecoverySecret::generate().unwrap();
        let rs2 = RecoverySecret::generate().unwrap();
        let rk1 = RecoveryKey::derive(&rs1).unwrap();
        let rk2 = RecoveryKey::derive(&rs2).unwrap();

        let encrypted = rk1.encrypt_dek(&dek).unwrap();
        let result = rk2.decrypt_dek(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn recovery_key_is_32_bytes() {
        let rs = RecoverySecret::generate().unwrap();
        let rk = RecoveryKey::derive(&rs).unwrap();
        assert_eq!(rk.key.len(), 32);
    }
}
