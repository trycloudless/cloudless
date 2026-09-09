use crate::{
    app_env::AppEnv,
    core::encrypted_dek::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::encrypted_dek::{
    GetEncryptedDekRequest, GetEncryptedDekResponse, StoreEncryptedDekRequest,
    StoreEncryptedDekResponse,
};
use axum::{Extension, Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/store", post(store_encrypted_dek))
        .route("/get", post(get_encrypted_dek))
}

async fn store_encrypted_dek(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<StoreEncryptedDekRequest>,
) -> WebAppResult<Json<StoreEncryptedDekResponse>> {
    let response = application::store(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn get_encrypted_dek(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetEncryptedDekRequest>,
) -> WebAppResult<Json<GetEncryptedDekResponse>> {
    let response = application::get(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}
