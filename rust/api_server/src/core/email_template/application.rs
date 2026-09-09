use crate::core::{
    CoreError, CoreResult, email_template::env::EmailTemplateEnv, ports::EmailTemplateRepo,
};
use api_types::email_template::*;
use uuid::Uuid;

pub async fn create_template<E: EmailTemplateEnv>(
    env: &E,
    req: CreateEmailTemplateRequest,
) -> CoreResult<EmailTemplateResponse> {
    env.email_template_repo().create(req).await
}

pub async fn update_template<E: EmailTemplateEnv>(
    env: &E,
    req: UpdateEmailTemplateRequest,
) -> CoreResult<EmailTemplateResponse> {
    env.email_template_repo().update(req).await
}

pub async fn delete_template<E: EmailTemplateEnv>(env: &E, id: Uuid) -> CoreResult<()> {
    env.email_template_repo().delete(id).await
}

pub async fn get_by_id<E: EmailTemplateEnv>(
    env: &E,
    id: Uuid,
) -> CoreResult<EmailTemplateResponse> {
    env.email_template_repo()
        .get_by_id(id)
        .await?
        .ok_or_else(|| CoreError::not_found(format!("Email template: {}", id)))
}

pub async fn list_all<E: EmailTemplateEnv>(env: &E) -> CoreResult<Vec<EmailTemplateSummary>> {
    env.email_template_repo().list_all().await
}
