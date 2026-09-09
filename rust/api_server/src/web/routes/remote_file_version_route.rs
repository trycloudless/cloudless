use crate::{
    app_env::AppEnv,
    core::remote_file_version::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::remote_file_version::{
    CreateFileVersionRequest, CreateFileVersionResponse, ListAllVersionsRequest,
    ListAllVersionsResponse, ListBackedUpFilesRequest, ListBackedUpFilesResponse,
    ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
    MoveVersionToBinRequest, RestoreVersionFromBinRequest, UpdateFileVersionStatusRequest,
};
use axum::{Extension, Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_remote_file_version))
        .route("/update_status", post(update_status_handler))
        .route("/list_backed_up_files", post(list_backed_up_files_handler))
        .route("/list_all_versions", post(list_all_versions_handler))
        .route("/move_to_bin", post(move_to_bin_handler))
        .route("/move_all_to_bin", post(move_all_to_bin_handler))
        .route("/restore_from_bin", post(restore_from_bin_handler))
        .route("/list_bin", post(list_bin_handler))
}

async fn create_remote_file_version(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateFileVersionRequest>,
) -> WebAppResult<Json<CreateFileVersionResponse>> {
    let response = application::create_remote_file_version(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn list_backed_up_files_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<ListBackedUpFilesRequest>,
) -> WebAppResult<Json<ListBackedUpFilesResponse>> {
    let response = application::list_backed_up_files(env, ctx.user_id, request).await?;
    Ok(Json(response))
}

async fn list_all_versions_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<ListAllVersionsRequest>,
) -> WebAppResult<Json<ListAllVersionsResponse>> {
    let response = application::list_all_versions(env, ctx.user_id, request).await?;
    Ok(Json(response))
}

async fn update_status_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Json(request): Json<UpdateFileVersionStatusRequest>,
) -> WebAppResult<Json<()>> {
    application::update_file_version_status(env, request).await?;
    Ok(Json(()))
}

async fn move_to_bin_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<MoveVersionToBinRequest>,
) -> WebAppResult<Json<()>> {
    application::move_version_to_bin(env, ctx.user_id, request).await?;
    Ok(Json(()))
}

async fn move_all_to_bin_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<MoveAllVersionsToBinRequest>,
) -> WebAppResult<Json<()>> {
    application::move_all_versions_to_bin(env, ctx.user_id, request).await?;
    Ok(Json(()))
}

async fn restore_from_bin_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<RestoreVersionFromBinRequest>,
) -> WebAppResult<Json<()>> {
    application::restore_version_from_bin(env, ctx.user_id, request).await?;
    Ok(Json(()))
}

async fn list_bin_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<ListBinVersionsRequest>,
) -> WebAppResult<Json<ListBinVersionsResponse>> {
    let response = application::list_bin_versions(env, ctx.user_id, request).await?;
    Ok(Json(response))
}
