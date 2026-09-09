use api_types::email_template::{
    CreateEmailTemplateRequest, DeleteEmailTemplateResponse, EmailTemplateListResponse,
    EmailTemplateResponse, UpdateEmailTemplateRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait EmailTemplateApiPort: Send + Sync {
    async fn list_all(&self) -> ApiResult<EmailTemplateListResponse>;
    async fn get_by_id(&self, id: Uuid) -> ApiResult<EmailTemplateResponse>;
    async fn create(&self, req: CreateEmailTemplateRequest) -> ApiResult<EmailTemplateResponse>;
    async fn update(&self, req: UpdateEmailTemplateRequest) -> ApiResult<EmailTemplateResponse>;
    async fn delete(&self, id: Uuid) -> ApiResult<DeleteEmailTemplateResponse>;
}
