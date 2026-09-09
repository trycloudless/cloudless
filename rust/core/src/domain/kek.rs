use argon2::Argon2;
use argon2::password_hash::SaltString;
use zeroize::Zeroizing;

use crate::model::base::AppResult;

pub struct Kek {
    pub key: Zeroizing<[u8; 32]>,
}

impl Kek {
    pub fn derive(password: &str, salt: &SaltString) -> AppResult<Self> {
        let mut key = Zeroizing::new([0u8; 32]);

        build_argon2().hash_password_into(
            password.as_bytes(),
            salt.as_str().as_bytes(),
            &mut key[..],
        )?;

        Ok(Self { key })
    }
}

/// Builds the Argon2 hasher with appropriate parameters.
///
/// In normal builds, uses the default (secure) parameters.
/// In e2e test builds, uses minimal parameters so that key derivation
/// completes quickly in unoptimized debug binaries.
fn build_argon2() -> Argon2<'static> {
    #[cfg(feature = "e2e")]
    tracing::info!("Using lightweight argon2 params for e2e testing");

    #[cfg(feature = "e2e")]
    {
        use argon2::{Algorithm, Params, Version};
        let params = Params::new(
            Params::MIN_M_COST, // 8 KiB (vs default 19 MiB)
            1,                  // 1 iteration (vs default 2)
            1,                  // 1 lane (vs default 1)
            Some(32),
        )
        .expect("valid argon2 params");
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    }

    #[cfg(not(feature = "e2e"))]
    {
        Argon2::default()
    }
}
