use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{
    CoreResult,
    ports::{PasswordResetTokenRecord, PasswordResetTokenRepo},
};

use super::map_sqlx_error;

#[derive(Clone)]
pub struct PgPasswordResetTokenRepo {
    pub pool: PgPool,
}

impl PgPasswordResetTokenRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PasswordResetTokenRepo for PgPasswordResetTokenRepo {
    async fn insert(
        &self,
        user_id: Uuid,
        token_hash: String,
        expires_at: DateTime<Utc>,
    ) -> CoreResult<()> {
        let id = Uuid::now_v7();
        sqlx::query!(
            r#"
            INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
            id,
            user_id,
            token_hash,
            expires_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn find_valid(&self, token_hash: &str) -> CoreResult<Option<PasswordResetTokenRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, token_hash, expires_at, used_at, created_at
            FROM password_reset_tokens
            WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
            "#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| PasswordResetTokenRecord {
            id: r.id,
            user_id: r.user_id,
            token_hash: r.token_hash,
            expires_at: r.expires_at,
            used_at: r.used_at,
            created_at: r.created_at,
        }))
    }

    async fn mark_used(&self, id: Uuid) -> CoreResult<()> {
        sqlx::query!(
            "UPDATE password_reset_tokens SET used_at = now() WHERE id = $1",
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
