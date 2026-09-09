use rand::{TryRngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

use crate::core::CoreResult;

pub fn generate_refresh_token() -> CoreResult<String> {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes)?;
    Ok(hex::encode(bytes))
}

pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
