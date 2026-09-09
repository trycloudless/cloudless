use jsonwebtoken::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::core::CoreResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
}

#[derive(Clone)]
pub struct JwtService {
    secret: String,
    pub access_ttl_secs: u64,
}

impl JwtService {
    pub fn new(secret: String, access_ttl_secs: u64) -> Self {
        Self {
            secret,
            access_ttl_secs,
        }
    }

    pub fn create_access_token(&self, user_id: Uuid) -> CoreResult<String> {
        let exp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + self.access_ttl_secs;

        let claims = Claims {
            sub: user_id,
            exp: exp as usize,
        };

        Ok(encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )?)
    }

    pub fn verify(&self, token: &str) -> CoreResult<Claims> {
        Ok(decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )?
        .claims)
    }
}
