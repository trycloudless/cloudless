use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::subscription::{SubscriptionStatus, SubscriptionTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRole {
    #[default]
    User,
    SuperAdmin,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::User => write!(f, "USER"),
            UserRole::SuperAdmin => write!(f, "SUPER_ADMIN"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserCreateRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserCreateResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub email_verified: bool,
}

pub type UserResponse = UserCreateResponse;

pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: UserRole,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: UserRole,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
}

/// Richer user record returned exclusively by the admin list endpoint.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AdminUserInfo {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub email_verified: bool,
    pub plan: SubscriptionTier,
    pub plan_status: SubscriptionStatus,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AdminUserListResponse {
    pub users: Vec<AdminUserInfo>,
    pub total_count: u64,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct UserListQueryParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}
