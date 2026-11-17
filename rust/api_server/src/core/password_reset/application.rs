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

/// Subject used when no `password_reset` template exists in the database.
pub const DEFAULT_RESET_SUBJECT: &str = "Reset your CloudLess password";

/// Body used when no `password_reset` template exists in the database.
/// Rendered with Tera; receives `user_name` and `reset_link`.
pub const DEFAULT_RESET_BODY_HTML: &str = r#"<p>Hi {{ user_name }},</p>
<p>We received a request to reset your CloudLess password. Click the link below to choose a new one. The link expires in 1 hour.</p>
<p><a href="{{ reset_link }}">Reset password</a></p>
<p>If you didn't request this, you can safely ignore this email.</p>
<p>— The CloudLess team</p>"#;

/// Renders a subject/body pair with Tera. Autoescaping is off because the
/// body is admin-authored HTML.
fn render_template(
    subject: &str,
    body_html: &str,
    context: &Context,
) -> CoreResult<(String, String)> {
    let subject = tera::Tera::one_off(subject, context, false)
        .map_err(|e| CoreError::internal_with_source("Template rendering failed", e))?;
    let body = tera::Tera::one_off(body_html, context, false)
        .map_err(|e| CoreError::internal_with_source("Template rendering failed", e))?;
    Ok((subject, body))
}

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

    // 4. Find latest password_reset email template, falling back to the
    //    built-in default so a fresh install can send reset emails before an
    //    admin has created a custom template.
    let (subject, body_html) = match env
        .email_template_repo()
        .get_latest_by_type(EmailTemplateType::PasswordReset)
        .await?
    {
        Some(t) => (t.subject, t.body_html),
        None => {
            tracing::info!("No password reset template configured; using built-in default");
            (
                DEFAULT_RESET_SUBJECT.to_string(),
                DEFAULT_RESET_BODY_HTML.to_string(),
            )
        }
    };

    // 5. Render template with Tera
    let reset_link = format!(
        "{}/reset-password?token={}",
        env.frontend_base_url(),
        raw_token
    );
    let mut context = Context::new();
    context.insert("reset_link", &reset_link);
    context.insert("user_name", &user.name);

    let (rendered_subject, rendered_body) = render_template(&subject, &body_html, &context)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let mut c = Context::new();
        c.insert(
            "reset_link",
            "https://example.test/reset-password?token=abc",
        );
        c.insert("user_name", "Ada");
        c
    }

    /// The built-in default renders both variables and leaves no Tera syntax behind.
    #[test]
    fn default_template_renders_variables() {
        let (subject, body) =
            render_template(DEFAULT_RESET_SUBJECT, DEFAULT_RESET_BODY_HTML, &ctx())
                .expect("render");
        assert_eq!(subject, "Reset your CloudLess password");
        assert!(body.contains("Hi Ada,"));
        assert!(body.contains(r#"href="https://example.test/reset-password?token=abc""#));
        assert!(!body.contains("{{"));
    }

    /// Custom (admin-authored) templates render the same variables.
    #[test]
    fn custom_template_renders_variables() {
        let (subject, body) = render_template(
            "Reset for {{ user_name }}",
            "<a href=\"{{ reset_link }}\">go</a>",
            &ctx(),
        )
        .expect("render");
        assert_eq!(subject, "Reset for Ada");
        assert_eq!(
            body,
            r#"<a href="https://example.test/reset-password?token=abc">go</a>"#
        );
    }

    /// Invalid Tera syntax surfaces as an error instead of sending a broken email.
    #[test]
    fn invalid_template_is_an_error() {
        assert!(render_template("{{ unclosed", "body", &ctx()).is_err());
    }
}
