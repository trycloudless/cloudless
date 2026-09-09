use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::subscription::SubscriptionResponse;
use crate::user::UserRole;

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub role: UserRole,
    /// Subscription info for the authenticated user.
    pub subscription: SubscriptionResponse,
    /// True when the user has never completed encryption setup (no DEK stored).
    /// The client should route directly to the encryption setup wizard instead of UnlockEncryption.
    pub encryption_setup_required: bool,
}
