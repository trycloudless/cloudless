use api_types::{auth::*, user::RefreshToken};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::core::{
    CoreError, CoreResult,
    auth::{
        env::AuthEnv,
        refresh::{generate_refresh_token, hash_refresh_token},
    },
    ports::{AuthRepo, RefreshTokenRepo},
    subscription::{application as subscription_app, env::{PaymentEnv, SubscriptionEnv}},
};

pub async fn login<E: AuthEnv + SubscriptionEnv + PaymentEnv>(
    env: &E,
    req: LoginRequest,
) -> CoreResult<LoginResponse> {
    let user = env
        .auth_repo()
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid credentials"))?;

    let hash = PasswordHash::new(&user.password_hash)?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &hash)
        .map_err(|_| CoreError::authentication("Invalid credentials"))?;

    let access_token = env.jwt_service().create_access_token(user.id)?;
    let refresh_token = generate_refresh_token()?;
    let refresh_hash = hash_refresh_token(&refresh_token);

    env.refresh_token_repo()
        .insert(RefreshToken {
            id: Uuid::new_v4(),
            user_id: user.id,
            token_hash: refresh_hash,
            expires_at: Utc::now() + Duration::days(30),
            revoked: false,
        })
        .await?;

    let subscription = subscription_app::get_subscription(env, user.id).await?;

    Ok(LoginResponse {
        access_token,
        refresh_token,
        user_id: user.id,
        name: user.name,
        email: user.email,
        role: user.role,
        subscription,
        encryption_setup_required: false, // overridden by the route handler after DEK check
    })
}

pub async fn refresh<E: AuthEnv>(env: &E, req: RefreshRequest) -> CoreResult<Tokens> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    let stored = env
        .refresh_token_repo()
        .find_valid(&token_hash)
        .await?
        .ok_or_else(|| CoreError::authentication("Invalid refresh token"))?;

    env.refresh_token_repo().revoke(stored.id).await?;

    let access_token = env.jwt_service().create_access_token(stored.user_id)?;
    let new_refresh = generate_refresh_token()?;
    let new_hash = hash_refresh_token(&new_refresh);

    env.refresh_token_repo()
        .insert(RefreshToken {
            id: uuid::Uuid::new_v4(),
            user_id: stored.user_id,
            token_hash: new_hash,
            expires_at: stored.expires_at,
            revoked: false,
        })
        .await?;

    Ok(Tokens {
        access_token,
        refresh_token: new_refresh,
    })
}
