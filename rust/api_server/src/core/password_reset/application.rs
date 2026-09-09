use crate::core::{
    CoreError, CoreResult,
    auth::refresh::{generate_refresh_token, hash_refresh_token},
    password_reset::env::PasswordResetEnv,
    ports::{AuthRepo, EmailPort, EmailTemplateRepo, PasswordResetTokenRepo, UserRepo},
};
use api_types::{email_template::EmailTemplateType, password_reset::*};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{Duration, Utc};
use tera::Context;

/// Handles a password reset request. Generates a token, finds the latest
/// PasswordReset email template, renders it with Tera, and sends via EmailPort.
/// Always returns success to prevent email enumeration.
pub async fn request_reset<E: PasswordResetEnv>(
    env: &E,
    req: PasswordResetRequest,
) -> CoreResult<PasswordResetResponse> {
    let result = do_request_reset(env, req).await;
    if let Err(e) = &result {
        tracing::warn!("Password reset request failed (suppressed): {:?}", e);
    }
    Ok(PasswordResetResponse {
        message: "If that email exists, a reset link has been sent.".to_string(),
    })
}

async fn do_request_reset<E: PasswordResetEnv>(
    env: &E,
    req: PasswordResetRequest,
) -> CoreResult<()> {
    // 1. Find user by email
    let user = env
        .auth_repo()
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| CoreError::not_found("User"))?;

    // 2. Generate token (reusing refresh token pattern)
    let raw_token = generate_refresh_token()?;
    let token_hash = hash_refresh_token(&raw_token);
    let expires_at = Utc::now() + Duration::hours(1);

    // 3. Store hashed token
    env.password_reset_token_repo()
        .insert(user.id, token_hash, expires_at)
        .await?;

    // 4. Find latest password_reset email template
    let template = env
        .email_template_repo()
        .get_latest_by_type(EmailTemplateType::PasswordReset)
        .await?
        .ok_or_else(|| CoreError::internal("No password reset template configured"))?;

    // 5. Render template with Tera
    let reset_link = format!(
        "{}/reset-password?token={}",
        env.frontend_base_url(),
        raw_token
    );
    let mut context = Context::new();
    context.insert("reset_link", &reset_link);
    context.insert("user_name", &user.name);

    let rendered_subject = tera::Tera::one_off(&template.subject, &context, false)
        .map_err(|e| CoreError::internal_with_source("Template rendering failed", e))?;
    let rendered_body = tera::Tera::one_off(&template.body_html, &context, false)
        .map_err(|e| CoreError::internal_with_source("Template rendering failed", e))?;

    // 6. Send email
    env.email_adapter()
        .send_email(&user.email, &rendered_subject, &rendered_body)
        .await?;

    Ok(())
}

/// Validates a reset token and updates the user's password.
/// Token must be valid (exists, not expired, not used).
pub async fn confirm_reset<E: PasswordResetEnv>(
    env: &E,
    req: PasswordResetConfirmRequest,
) -> CoreResult<PasswordResetConfirmResponse> {
    let token_hash = hash_refresh_token(&req.token);

    // 1. Find valid (unexpired, unused) token
    let record = env
        .password_reset_token_repo()
        .find_valid(&token_hash)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid or expired reset token"))?;

    // 2. Check expiration
    if record.expires_at < Utc::now() {
        return Err(CoreError::authentication("Reset token has expired"));
    }

    // 3. Hash new password with Argon2
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.new_password.as_bytes(), &salt)?
        .to_string();

    // 4. Update user password
    env.user_repo()
        .update_password_hash(record.user_id, password_hash)
        .await?;

    // 5. Mark token as used
    env.password_reset_token_repo().mark_used(record.id).await?;

    Ok(PasswordResetConfirmResponse {
        message: "Password has been reset successfully.".to_string(),
    })
}
