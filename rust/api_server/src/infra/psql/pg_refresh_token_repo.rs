use api_types::user::RefreshToken;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::RefreshTokenRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgRefreshTokenRepo {
    pub pool: PgPool,
}

impl PgRefreshTokenRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepo for PgRefreshTokenRepo {
    async fn insert(&self, token: RefreshToken) -> CoreResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, revoked)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            token.id,
            token.user_id,
            token.token_hash,
            token.expires_at,
            token.revoked
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn find_valid(&self, token_hash: &str) -> CoreResult<Option<RefreshToken>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, token_hash, expires_at, revoked
            FROM refresh_tokens
            WHERE token_hash = $1 AND revoked = false AND expires_at > now()
            "#,
            token_hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| RefreshToken {
            id: r.id,
            user_id: r.user_id,
            token_hash: r.token_hash,
            expires_at: r.expires_at.into(),
            revoked: r.revoked,
        }))
    }

    async fn revoke(&self, id: Uuid) -> CoreResult<()> {
        sqlx::query!("UPDATE refresh_tokens SET revoked = true WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }
}
