use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{
    CoreResult,
    ports::{VerificationCodeRecord, VerificationCodeRepo},
};

use super::map_sqlx_error;

#[derive(Clone)]
pub struct PgVerificationCodeRepo {
    pub pool: PgPool,
}

impl PgVerificationCodeRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VerificationCodeRepo for PgVerificationCodeRepo {
    async fn insert(
        &self,
        user_id: Uuid,
        code_hash: String,
        expires_at: DateTime<Utc>,
    ) -> CoreResult<()> {
        let id = Uuid::now_v7();
        sqlx::query!(
            r#"
            INSERT INTO email_verification_codes (id, user_id, code_hash, expires_at)
            VALUES ($1, $2, $3, $4)
            "#,
            id,
            user_id,
            code_hash,
            expires_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn find_valid(
        &self,
        user_id: Uuid,
        code_hash: &str,
    ) -> CoreResult<Option<VerificationCodeRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, code_hash, expires_at, used_at
            FROM email_verification_codes
            WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL AND expires_at > NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            user_id,
            code_hash,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| VerificationCodeRecord {
            id: r.id,
            user_id: r.user_id,
            code_hash: r.code_hash,
            expires_at: r.expires_at,
            used_at: r.used_at,
        }))
    }

    async fn mark_used(&self, id: Uuid) -> CoreResult<()> {
        sqlx::query!(
            "UPDATE email_verification_codes SET used_at = NOW() WHERE id = $1",
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn invalidate_all(&self, user_id: Uuid) -> CoreResult<()> {
        sqlx::query!(
            "UPDATE email_verification_codes SET used_at = NOW() WHERE user_id = $1 AND used_at IS NULL",
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }
}
