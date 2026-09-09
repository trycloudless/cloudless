use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PasswordResetResponse {
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PasswordResetConfirmRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PasswordResetConfirmResponse {
    pub message: String,
}
