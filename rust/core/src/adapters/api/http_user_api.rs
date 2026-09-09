use api_types::{
    auth::{LoginRequest, LoginResponse, RefreshRequest, Tokens},
    email_verification::{
        ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
        VerifyEmailResponse,
    },
    user::{AdminUserListResponse, UserCreateRequest, UserCreateResponse, UserInfo},
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, user_api_port::UserApiPort},
};

#[derive(Clone)]
pub struct HttpUserApi {
    api: HttpApi,
}

impl HttpUserApi {
    pub fn new(api: HttpApi) -> Self {
        HttpUserApi { api }
    }
}

#[async_trait]
impl UserApiPort for HttpUserApi {
    async fn create_user(&self, request: UserCreateRequest) -> ApiResult<UserCreateResponse> {
        let url = self.api.get_url("auth/signup")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send(request).await
    }

    async fn login(&self, request: LoginRequest) -> ApiResult<LoginResponse> {
        self.api.login(request).await
    }

    async fn refresh(&self, refresh_token: &str) -> ApiResult<Tokens> {
        let url = self.api.get_url("auth/refresh")?;
        let request = self.api.client.post(url).json(&RefreshRequest {
            refresh_token: refresh_token.to_string(),
        });
        self.api.send(request).await
    }

    async fn get_me(&self, token: &str) -> ApiResult<UserInfo> {
        let url = self.api.get_url("api/user/me")?;
        let request = self.api.client.get(url).bearer_auth(token);
        self.api.send(request).await
    }

    async fn list_all_users(
        &self,
        token: &str,
        page: u32,
        per_page: u32,
    ) -> ApiResult<AdminUserListResponse> {
        let url = self.api.get_url("api/user/list")?;
        let request = self
            .api
            .client
            .get(url)
            .bearer_auth(token)
            .query(&[("page", page), ("per_page", per_page)]);
        self.api.send(request).await
    }

    async fn verify_email(&self, request: VerifyEmailRequest) -> ApiResult<VerifyEmailResponse> {
        let url = self.api.get_url("auth/verify-email")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send(request).await
    }

    async fn resend_verification(
        &self,
        request: ResendVerificationRequest,
    ) -> ApiResult<ResendVerificationResponse> {
        let url = self.api.get_url("auth/resend-verification")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send(request).await
    }
}
