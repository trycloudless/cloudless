use api_types::email_template::*;
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{CoreResult, ports::EmailTemplateRepo};

use super::map_sqlx_error;

#[derive(Clone)]
pub struct PgEmailTemplateRepo {
    pub pool: PgPool,
}

impl PgEmailTemplateRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmailTemplateRepo for PgEmailTemplateRepo {
    async fn create(&self, req: CreateEmailTemplateRequest) -> CoreResult<EmailTemplateResponse> {
        let id = Uuid::now_v7();
        let template_type_str = req.template_type.to_string();
        let row = sqlx::query!(
            r#"
            INSERT INTO email_templates (id, name, subject, body_html, template_type)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, subject, body_html, template_type, created_at, updated_at
            "#,
            id,
            req.name,
            req.subject,
            req.body_html,
            template_type_str,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(EmailTemplateResponse {
            id: row.id,
            name: row.name,
            subject: row.subject,
            body_html: row.body_html,
            template_type: EmailTemplateType::from_str(&row.template_type),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn update(&self, req: UpdateEmailTemplateRequest) -> CoreResult<EmailTemplateResponse> {
        let template_type_str = req.template_type.map(|t| t.to_string());
        let row = sqlx::query!(
            r#"
            UPDATE email_templates
            SET name = COALESCE($2, name),
                subject = COALESCE($3, subject),
                body_html = COALESCE($4, body_html),
                template_type = COALESCE($5, template_type),
                updated_at = now()
            WHERE id = $1
            RETURNING id, name, subject, body_html, template_type, created_at, updated_at
            "#,
            req.id,
            req.name,
            req.subject,
            req.body_html,
            template_type_str,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(EmailTemplateResponse {
            id: row.id,
            name: row.name,
            subject: row.subject,
            body_html: row.body_html,
            template_type: EmailTemplateType::from_str(&row.template_type),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn delete(&self, id: Uuid) -> CoreResult<()> {
        sqlx::query!("DELETE FROM email_templates WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> CoreResult<Option<EmailTemplateResponse>> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, subject, body_html, template_type, created_at, updated_at
            FROM email_templates
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| EmailTemplateResponse {
            id: r.id,
            name: r.name,
            subject: r.subject,
            body_html: r.body_html,
            template_type: EmailTemplateType::from_str(&r.template_type),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn list_all(&self) -> CoreResult<Vec<EmailTemplateSummary>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, name, subject, template_type, updated_at
            FROM email_templates
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|r| EmailTemplateSummary {
                id: r.id,
                name: r.name,
                subject: r.subject,
                template_type: EmailTemplateType::from_str(&r.template_type),
                updated_at: r.updated_at,
            })
            .collect())
    }

    async fn get_latest_by_type(
        &self,
        template_type: EmailTemplateType,
    ) -> CoreResult<Option<EmailTemplateResponse>> {
        let template_type_str = template_type.to_string();
        let row = sqlx::query!(
            r#"
            SELECT id, name, subject, body_html, template_type, created_at, updated_at
            FROM email_templates
            WHERE template_type = $1
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            template_type_str,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| EmailTemplateResponse {
            id: r.id,
            name: r.name,
            subject: r.subject,
            body_html: r.body_html,
            template_type: EmailTemplateType::from_str(&r.template_type),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }
}
