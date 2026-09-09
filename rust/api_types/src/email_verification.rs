use serde::{Deserialize, Serialize};

/// Request to verify an email address using a 6-digit code.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyEmailRequest {
    pub email: String,
    pub code: String,
}

/// Response after successful email verification.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyEmailResponse {
    pub message: String,
    pub verified: bool,
}

/// Request to resend the verification code.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResendVerificationRequest {
    pub email: String,
}

/// Response after requesting a verification code resend.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResendVerificationResponse {
    pub message: String,
}
