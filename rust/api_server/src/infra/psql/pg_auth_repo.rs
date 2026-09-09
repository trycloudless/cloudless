use api_types::user::User;
use async_trait::async_trait;
use sqlx::PgPool;

use crate::{
    core::{CoreResult, ports::AuthRepo},
    infra::psql::{map_sqlx_error, user_role_type::PgUserRole},
};

#[derive(Clone)]
pub struct PgAuthRepo {
    pub pool: PgPool,
}

impl PgAuthRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthRepo for PgAuthRepo {
    async fn find_by_email(&self, email: &str) -> CoreResult<Option<User>> {
        let row = sqlx::query!(
            r#"SELECT id, name, email, password_hash, role as "role: PgUserRole", email_verified FROM users WHERE email = $1"#,
            email
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| User {
            id: r.id,
            name: r.name,
            email: r.email,
            password_hash: r.password_hash,
            role: r.role.into(),
            email_verified: r.email_verified,
        }))
    }
}
