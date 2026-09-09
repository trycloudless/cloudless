use crate::core::{
    CoreError, CoreResult,
    auth::refresh::hash_refresh_token,
    email_verification::env::EmailVerificationEnv,
    ports::{AuthRepo, EmailPort, UserRepo, VerificationCodeRepo},
};
use api_types::email_verification::*;
use chrono::{Duration, Utc};
use rand::Rng;

/// Generates a cryptographically random 6-digit code (100000..999999).
fn generate_verification_code() -> String {
    let code: u32 = rand::rng().random_range(100_000..1_000_000);
    code.to_string()
}

/// Sends a verification code to the user's email.
///
/// Invalidates any previous unused codes for the user before creating a new one.
/// The code expires after 15 minutes.
pub async fn send_verification_code<E: EmailVerificationEnv>(
    env: &E,
    user_id: uuid::Uuid,
    email: &str,
    user_name: &str,
) -> CoreResult<()> {
    // Invalidate previous codes
    env.verification_code_repo().invalidate_all(user_id).await?;

    // Generate and store new code
    let code = generate_verification_code();
    let code_hash = hash_refresh_token(&code);
    let expires_at = Utc::now() + Duration::minutes(15);

    env.verification_code_repo()
        .insert(user_id, code_hash, expires_at)
        .await?;

    // Send email with inline template
    let subject = "CloudLess — Verify Your Email";
    let body = format!(
        r#"<!DOCTYPE html>
<html>
<body style="font-family: system-ui, sans-serif; background: #0f172a; color: #f8fafc; padding: 2rem;">
  <div style="max-width: 480px; margin: 0 auto; background: #1e293b; border-radius: 12px; padding: 2rem;">
    <h2 style="margin-top: 0;">Welcome to CloudLess, {name}!</h2>
    <p>Your verification code is:</p>
    <div style="background: #0f172a; border: 1px solid rgba(255,255,255,0.1); border-radius: 8px; padding: 1rem; text-align: center; font-size: 2rem; font-family: monospace; letter-spacing: 0.3em; font-weight: bold;">
      {code}
    </div>
    <p style="color: #94a3b8; font-size: 0.875rem; margin-top: 1rem;">
      This code expires in 15 minutes. If you didn't sign up for CloudLess, you can ignore this email.
    </p>
  </div>
</body>
</html>"#,
        name = user_name,
        code = code,
    );

    env.email_adapter()
        .send_email(email, subject, &body)
        .await?;

    tracing::info!(user_id = %user_id, "Verification code sent to {}", email);

    Ok(())
}

/// Verifies a user's email using the 6-digit code.
///
/// On success, marks the user as email-verified and invalidates the code.
pub async fn verify_email<E: EmailVerificationEnv>(
    env: &E,
    req: VerifyEmailRequest,
) -> CoreResult<VerifyEmailResponse> {
    // Find user by email
    let user = env
        .auth_repo()
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid email or code"))?;

    if user.email_verified {
        return Ok(VerifyEmailResponse {
            message: "Email is already verified.".to_string(),
            verified: true,
        });
    }

    // Hash the provided code and look it up
    let code_hash = hash_refresh_token(&req.code);
    let record = env
        .verification_code_repo()
        .find_valid(user.id, &code_hash)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid or expired verification code"))?;

    // Check expiry (belt-and-suspenders; query already filters)
    if record.expires_at < Utc::now() {
        return Err(CoreError::authentication("Verification code has expired"));
    }

    // Mark code as used
    env.verification_code_repo().mark_used(record.id).await?;

    // Mark user as verified
    env.user_repo().set_email_verified(user.id, true).await?;

    tracing::info!(user_id = %user.id, "Email verified for {}", req.email);

    Ok(VerifyEmailResponse {
        message: "Email verified successfully.".to_string(),
        verified: true,
    })
}

/// Resends the verification code to the user's email.
pub async fn resend_verification<E: EmailVerificationEnv>(
    env: &E,
    req: ResendVerificationRequest,
) -> CoreResult<ResendVerificationResponse> {
    let user = env
        .auth_repo()
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| CoreError::authentication("User not found"))?;

    if user.email_verified {
        return Ok(ResendVerificationResponse {
            message: "Email is already verified.".to_string(),
        });
    }

    send_verification_code(env, user.id, &user.email, &user.name).await?;

    Ok(ResendVerificationResponse {
        message: "Verification code sent. Check your email.".to_string(),
    })
}
