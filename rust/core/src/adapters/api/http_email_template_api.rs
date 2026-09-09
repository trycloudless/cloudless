use api_types::email_template::{
    CreateEmailTemplateRequest, DeleteEmailTemplateResponse, EmailTemplateListResponse,
    EmailTemplateResponse, UpdateEmailTemplateRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, email_template_api_port::EmailTemplateApiPort},
};

#[derive(Clone)]
pub struct HttpEmailTemplateApi {
    api: HttpApi,
}

impl HttpEmailTemplateApi {
    pub fn new(api: HttpApi) -> Self {
        HttpEmailTemplateApi { api }
    }
}

#[async_trait]
impl EmailTemplateApiPort for HttpEmailTemplateApi {
    async fn list_all(&self) -> ApiResult<EmailTemplateListResponse> {
        let url = self.api.get_url("api/email_template")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn get_by_id(&self, id: Uuid) -> ApiResult<EmailTemplateResponse> {
        let url = self.api.get_url(&format!("api/email_template/{}", id))?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn create(&self, req: CreateEmailTemplateRequest) -> ApiResult<EmailTemplateResponse> {
        let url = self.api.get_url("api/email_template")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn update(&self, req: UpdateEmailTemplateRequest) -> ApiResult<EmailTemplateResponse> {
        let url = self
            .api
            .get_url(&format!("api/email_template/{}", req.id))?;
        let request = self.api.client.put(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn delete(&self, id: Uuid) -> ApiResult<DeleteEmailTemplateResponse> {
        let url = self.api.get_url(&format!("api/email_template/{}", id))?;
        let request = self.api.client.delete(url);
        self.api.send_with_auth(request).await
    }
}
