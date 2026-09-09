use crate::{
    app_env::AppEnv,
    core::remote_storage::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::remote_storage::{
    CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageRequest,
    GetRemoteStorageResponse, ListRemoteStoragesResponse, ReauthRemoteStorageRequest,
    ReauthRemoteStorageResponse, UpdateRemoteStorageStatusRequest,
    UpdateRemoteStorageStatusResponse,
};
use axum::{Extension, Json, Router, extract::State, routing::post};
pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_remote_storage))
        .route("/get", post(get_remote_storage))
        .route("/list", post(list_remote_storages))
        .route("/update_status", post(update_remote_storage_status))
        .route("/reauth", post(reauth_remote_storage))
}

async fn create_remote_storage(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateRemoteStorageRequest>,
) -> WebAppResult<Json<CreateRemoteStorageResponse>> {
    let user = application::create_remote_storage(env, ctx.user_id, payload).await?;
    Ok(Json(user))
}

async fn get_remote_storage(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetRemoteStorageRequest>,
) -> WebAppResult<Json<GetRemoteStorageResponse>> {
    let response = application::get_by_id(env, ctx.user_id, payload.id).await?;
    Ok(Json(response))
}

async fn list_remote_storages(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<ListRemoteStoragesResponse>> {
    let response = application::list_by_user(env, ctx.user_id).await?;
    Ok(Json(response))
}

/// Updates the status of a remote storage (e.g. mark as AuthTokenExpired).
async fn update_remote_storage_status(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<UpdateRemoteStorageStatusRequest>,
) -> WebAppResult<Json<UpdateRemoteStorageStatusResponse>> {
    let response = application::update_status(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

/// Reauth a remote storage with new encrypted credentials.
/// Identity verification is done client-side before this endpoint is called.
async fn reauth_remote_storage(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ReauthRemoteStorageRequest>,
) -> WebAppResult<Json<ReauthRemoteStorageResponse>> {
    let response = application::reauth(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}
