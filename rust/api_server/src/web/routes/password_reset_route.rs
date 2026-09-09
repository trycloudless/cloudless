use crate::{app_env::AppEnv, core::password_reset::application, web::web_app_error::WebAppResult};
use api_types::password_reset::*;
use axum::{Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/request", post(request_handler))
        .route("/confirm", post(confirm_handler))
}

async fn request_handler(
    State(env): State<AppEnv>,
    Json(req): Json<PasswordResetRequest>,
) -> WebAppResult<Json<PasswordResetResponse>> {
    let response = application::request_reset(&env, req).await?;
    Ok(Json(response))
}

async fn confirm_handler(
    State(env): State<AppEnv>,
    Json(req): Json<PasswordResetConfirmRequest>,
) -> WebAppResult<Json<PasswordResetConfirmResponse>> {
    let response = application::confirm_reset(&env, req).await?;
    Ok(Json(response))
}
