use api_types::subscription::{SubscriptionStatus, SubscriptionTier};
use api_types::user::{AdminUserInfo, UserCreateRequest, UserCreateResponse, UserInfo};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{CoreError, CoreResult, ports::UserRepo};

use super::{map_sqlx_error, user_role_type::PgUserRole};

#[derive(Clone)]
pub struct PgUserRepo {
    pub pool: PgPool,
}

impl PgUserRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepo for PgUserRepo {
    async fn create_user(&self, user: UserCreateRequest) -> CoreResult<UserCreateResponse> {
        let id = uuid::Uuid::now_v7();
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(user.password.as_bytes(), &salt)?
            .to_string();

        let row = sqlx::query!(
            r#"
            INSERT INTO users (id, name, email, password_hash)
            VALUES ($1, $2, $3, $4)
            RETURNING id, name, email
            "#,
            id,
            user.name,
            user.email,
            password_hash
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db_err) if db_err.constraint() == Some("users_email_key") => {
                CoreError::validation("An account with this email address already exists.")
            }
            _ => map_sqlx_error(e),
        })?;

        Ok(UserCreateResponse {
            id: row.id,
            name: row.name,
            email: row.email,
            email_verified: false,
        })
    }

    async fn find_by_id(&self, id: Uuid) -> CoreResult<UserInfo> {
        let row = sqlx::query!(
            r#"SELECT id, name, email, role as "role: PgUserRole" FROM users WHERE id = $1"#,
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(UserInfo {
            id: row.id,
            name: row.name,
            email: row.email,
            role: row.role.into(),
        })
    }

    async fn list_all_paginated(
        &self,
        page: u32,
        per_page: u32,
    ) -> CoreResult<(Vec<AdminUserInfo>, u64)> {
        let limit = per_page as i64;
        let offset = ((page - 1) as i64) * limit;

        let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?
            .unwrap_or(0);

        let rows = sqlx::query!(
            r#"
            SELECT
                u.id,
                u.name,
                u.email,
                u.created_at,
                u.email_verified,
                COALESCE(s.tier::text, 'free')   AS "tier!: String",
                COALESCE(s.status::text, 'active') AS "status!: String"
            FROM users u
            LEFT JOIN subscriptions s ON s.user_id = u.id
            ORDER BY u.created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let users = rows
            .into_iter()
            .map(|r| AdminUserInfo {
                id: r.id,
                name: r.name,
                email: r.email,
                created_at: r.created_at,
                email_verified: r.email_verified,
                plan: r.tier.parse::<SubscriptionTier>().unwrap_or_default(),
                plan_status: r.status.parse::<SubscriptionStatus>().unwrap_or_default(),
            })
            .collect();

        Ok((users, total as u64))
    }

    async fn update_password_hash(&self, user_id: Uuid, password_hash: String) -> CoreResult<()> {
        sqlx::query!(
            "UPDATE users SET password_hash = $2 WHERE id = $1",
            user_id,
            password_hash,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn set_email_verified(&self, user_id: Uuid, verified: bool) -> CoreResult<()> {
        sqlx::query!(
            "UPDATE users SET email_verified = $2 WHERE id = $1",
            user_id,
            verified,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(())
    }
}
