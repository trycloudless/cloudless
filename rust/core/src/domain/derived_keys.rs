/// HKDF-SHA256 key derivation for separating content, metadata, and index keys.
///
/// Derives three purpose-specific 256-bit keys from the master DEK using
/// HKDF with distinct info strings to ensure cryptographic domain separation.
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{domain::dek::Dek, model::base::AppResult};

const CONTENT_INFO: &[u8] = b"cloudless-content-v1";
const METADATA_INFO: &[u8] = b"cloudless-metadata-v1";
const INDEX_INFO: &[u8] = b"cloudless-index-v1";

/// Holds three derived keys for different cryptographic purposes.
///
/// - `content_key`: encrypts file chunk data (AES-256-GCM)
/// - `metadata_key`: encrypts file names/paths (AES-256-GCM)
/// - `index_key`: computes blind indexes for server-side equality lookup (HMAC-SHA256)
#[derive(Debug, Clone)]
pub struct DerivedKeys {
    pub content_key: Zeroizing<[u8; 32]>,
    pub metadata_key: Zeroizing<[u8; 32]>,
    pub index_key: Zeroizing<[u8; 32]>,
}

impl DerivedKeys {
    /// Derives content, metadata, and index keys from the master DEK using HKDF-SHA256.
    pub fn derive(dek: &Dek) -> AppResult<Self> {
        let hkdf = Hkdf::<Sha256>::new(None, &dek.key[..]);

        let mut content_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(CONTENT_INFO, &mut content_key[..])
            .map_err(|e| crate::model::app_error::AppError::Internal {
                message: format!("HKDF expand failed for content key: {e}"),
                source: None,
            })?;

        let mut metadata_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(METADATA_INFO, &mut metadata_key[..])
            .map_err(|e| crate::model::app_error::AppError::Internal {
                message: format!("HKDF expand failed for metadata key: {e}"),
                source: None,
            })?;

        let mut index_key = Zeroizing::new([0u8; 32]);
        hkdf.expand(INDEX_INFO, &mut index_key[..]).map_err(|e| {
            crate::model::app_error::AppError::Internal {
                message: format!("HKDF expand failed for index key: {e}"),
                source: None,
            }
        })?;

        Ok(Self {
            content_key,
            metadata_key,
            index_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dek() -> Dek {
        Dek {
            key: Zeroizing::new([42u8; 32]),
        }
    }

    #[test]
    fn derive_is_deterministic() {
        let dek = test_dek();
        let keys1 = DerivedKeys::derive(&dek).unwrap();
        let keys2 = DerivedKeys::derive(&dek).unwrap();

        assert_eq!(keys1.content_key[..], keys2.content_key[..]);
        assert_eq!(keys1.metadata_key[..], keys2.metadata_key[..]);
        assert_eq!(keys1.index_key[..], keys2.index_key[..]);
    }

    #[test]
    fn derived_keys_are_independent() {
        let dek = test_dek();
        let keys = DerivedKeys::derive(&dek).unwrap();

        assert_ne!(keys.content_key[..], keys.metadata_key[..]);
        assert_ne!(keys.content_key[..], keys.index_key[..]);
        assert_ne!(keys.metadata_key[..], keys.index_key[..]);
    }

    #[test]
    fn derived_keys_are_32_bytes() {
        let dek = test_dek();
        let keys = DerivedKeys::derive(&dek).unwrap();

        assert_eq!(keys.content_key.len(), 32);
        assert_eq!(keys.metadata_key.len(), 32);
        assert_eq!(keys.index_key.len(), 32);
    }

    #[test]
    fn different_deks_produce_different_keys() {
        let dek1 = Dek {
            key: Zeroizing::new([1u8; 32]),
        };
        let dek2 = Dek {
            key: Zeroizing::new([2u8; 32]),
        };
        let keys1 = DerivedKeys::derive(&dek1).unwrap();
        let keys2 = DerivedKeys::derive(&dek2).unwrap();

        assert_ne!(keys1.content_key[..], keys2.content_key[..]);
        assert_ne!(keys1.metadata_key[..], keys2.metadata_key[..]);
        assert_ne!(keys1.index_key[..], keys2.index_key[..]);
    }
}
