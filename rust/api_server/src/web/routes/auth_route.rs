use crate::core::auth::application;
use crate::core::email_verification::application as verification_app;
use crate::core::user::application as user_application;
use crate::core::{
    CoreError, email_verification::env::EmailVerificationEnv, ports::EncryptedDekRepo,
};
use crate::web::web_app_error::WebAppResult;
use api_types::auth::*;
use api_types::email_verification::*;
use api_types::encrypted_dek::DekKeyType;
use api_types::user::{UserCreateRequest, UserCreateResponse};
use axum::{Json, Router, extract::State, routing::post};

use crate::app_env::AppEnv;

pub async fn login_handler(
    State(env): State<AppEnv>,
    Json(req): Json<LoginRequest>,
) -> WebAppResult<Json<LoginResponse>> {
    use crate::core::ports::AuthRepo;

    // Check if user is verified before allowing login
    let user = EmailVerificationEnv::auth_repo(&env)
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid credentials"))?;

    if !user.email_verified {
        return Err(CoreError::Validation {
            message: "Email not verified. Please check your inbox for the verification code."
                .to_string(),
            source: None,
        });
    }

    let mut response = application::login(&env, req).await?;

    let encryption_setup_required = match env
        .encrypted_dek_repo
        .get(response.user_id, DekKeyType::Password)
        .await
    {
        Ok(_) => false,
        Err(CoreError::NotFound { .. }) => true,
        Err(e) => return Err(e),
    };
    response.encryption_setup_required = encryption_setup_required;

    Ok(Json(response))
}

pub async fn refresh_handler(
    State(env): State<AppEnv>,
    Json(req): Json<RefreshRequest>,
) -> WebAppResult<Json<Tokens>> {
    let tokens = application::refresh(&env, req).await?;
    Ok(Json(tokens))
}

pub async fn signup_handler(
    State(env): State<AppEnv>,
    Json(req): Json<UserCreateRequest>,
) -> WebAppResult<Json<UserCreateResponse>> {
    use crate::core::ports::UserRepo;

    let name = req.name.clone();
    let email = req.email.clone();
    let user = user_application::create_user(env.clone(), req).await?;

    let email_verified = if env.email_sending_enabled {
        // Production: send verification email
        if let Err(e) = verification_app::send_verification_code(&env, user.id, &email, &name).await
        {
            tracing::error!(user_id = %user.id, "Failed to send verification email: {:?}", e);
            // Don't fail signup if email sending fails — user can resend
        }
        false
    } else {
        // Dev/test: auto-verify since emails can't be sent
        if let Err(e) = env.user_repo.set_email_verified(user.id, true).await {
            tracing::error!(user_id = %user.id, "Failed to auto-verify email: {:?}", e);
        }
        tracing::info!(user_id = %user.id, "Auto-verified email (email sending disabled)");
        true
    };

    Ok(Json(UserCreateResponse {
        email_verified,
        ..user
    }))
}

async fn verify_email_handler(
    State(env): State<AppEnv>,
    Json(req): Json<VerifyEmailRequest>,
) -> WebAppResult<Json<VerifyEmailResponse>> {
    let resp = verification_app::verify_email(&env, req).await?;
    Ok(Json(resp))
}

async fn resend_verification_handler(
    State(env): State<AppEnv>,
    Json(req): Json<ResendVerificationRequest>,
) -> WebAppResult<Json<ResendVerificationResponse>> {
    let resp = verification_app::resend_verification(&env, req).await?;
    Ok(Json(resp))
}

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/login", post(login_handler))
        .route("/refresh", post(refresh_handler))
        .route("/signup", post(signup_handler))
        .route("/verify-email", post(verify_email_handler))
        .route("/resend-verification", post(resend_verification_handler))
}
