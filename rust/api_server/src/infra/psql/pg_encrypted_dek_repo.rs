use api_types::{
    common::{Base64EncryptedData, EncryptionAlgorithm},
    encrypted_dek::{
        DekKeyType, GetEncryptedDekResponse, StoreEncryptedDekRequest, StoreEncryptedDekResponse,
    },
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::EncryptedDekRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgEncryptedDekRepo {
    pub pool: PgPool,
}

impl PgEncryptedDekRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EncryptedDekRepo for PgEncryptedDekRepo {
    async fn store(
        &self,
        user_id: Uuid,
        request: StoreEncryptedDekRequest,
    ) -> CoreResult<StoreEncryptedDekResponse> {
        let id = Uuid::now_v7();
        let key_type_str = request.key_type.to_string();
        let algorithm_str =
            serde_json::to_string(&request.encrypted_key.algorithm).map_err(|e| {
                crate::core::CoreError::internal_with_source("Failed to serialize algorithm", e)
            })?;

        use base64::{Engine, engine::general_purpose::STANDARD};
        let encrypted_key_bytes =
            STANDARD
                .decode(&request.encrypted_key.ciphertext)
                .map_err(|e| {
                    crate::core::CoreError::internal_with_source(
                        "Failed to decode encrypted key",
                        e,
                    )
                })?;
        let nonce_bytes = STANDARD.decode(&request.encrypted_key.nonce).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to decode nonce", e)
        })?;

        let row = sqlx::query!(
            r#"
            INSERT INTO encrypted_deks (id, user_id, key_type, encrypted_key, nonce, salt, algorithm)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id, key_type) DO UPDATE SET
                encrypted_key = EXCLUDED.encrypted_key,
                nonce = EXCLUDED.nonce,
                salt = EXCLUDED.salt,
                algorithm = EXCLUDED.algorithm,
                updated_at = now()
            RETURNING id
            "#,
            id,
            user_id,
            key_type_str,
            encrypted_key_bytes,
            nonce_bytes,
            request.salt,
            algorithm_str,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(StoreEncryptedDekResponse { id: row.id })
    }

    async fn get(
        &self,
        user_id: Uuid,
        key_type: DekKeyType,
    ) -> CoreResult<GetEncryptedDekResponse> {
        let key_type_str = key_type.to_string();

        let row = sqlx::query!(
            r#"
            SELECT encrypted_key, nonce, salt, algorithm
            FROM encrypted_deks
            WHERE user_id = $1 AND key_type = $2
            "#,
            user_id,
            key_type_str,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| crate::core::CoreError::not_found("encryption setup"))?;

        use base64::{Engine, engine::general_purpose::STANDARD};
        let algorithm: EncryptionAlgorithm = serde_json::from_str(&row.algorithm).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to deserialize algorithm", e)
        })?;

        Ok(GetEncryptedDekResponse {
            encrypted_key: Base64EncryptedData {
                ciphertext: STANDARD.encode(&row.encrypted_key),
                nonce: STANDARD.encode(&row.nonce),
                algorithm,
            },
            salt: row.salt,
        })
    }
}
