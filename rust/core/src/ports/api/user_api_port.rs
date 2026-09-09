use api_types::{
    auth::{LoginRequest, LoginResponse, Tokens},
    email_verification::{
        ResendVerificationRequest, ResendVerificationResponse, VerifyEmailRequest,
        VerifyEmailResponse,
    },
    user::{AdminUserListResponse, UserCreateRequest, UserCreateResponse, UserInfo},
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait UserApiPort: Send + Sync {
    async fn create_user(
        &self,
        user_create_request: UserCreateRequest,
    ) -> ApiResult<UserCreateResponse>;
    async fn login(&self, request: LoginRequest) -> ApiResult<LoginResponse>;
    async fn get_me(&self, token: &str) -> ApiResult<UserInfo>;
    /// Exchanges a valid refresh token for a new access + refresh token pair.
    /// The old refresh token is revoked server-side (token rotation).
    async fn refresh(&self, refresh_token: &str) -> ApiResult<Tokens>;
    async fn list_all_users(
        &self,
        token: &str,
        page: u32,
        per_page: u32,
    ) -> ApiResult<AdminUserListResponse>;
    async fn verify_email(&self, request: VerifyEmailRequest) -> ApiResult<VerifyEmailResponse>;
    async fn resend_verification(
        &self,
        request: ResendVerificationRequest,
    ) -> ApiResult<ResendVerificationResponse>;
}
