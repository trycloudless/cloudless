use crate::{
    app_env::AppEnv,
    core::backup_config::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::backup_config::{
    CreateBackupConfigRequest, CreateBackupConfigResponse, GetBackupConfigRequest,
    GetBackupConfigResponse, ListAllBackupConfigsResponse,
    ListBackupConfigWithRemoteStorageRequest, ListBackupConfigWithRemoteStorageResponse,
    RenameBackupConfigRequest, RenameBackupConfigResponse, ToggleBackupConfigRequest,
    ToggleBackupConfigResponse, UpdateCleanupTypeRequest, UpdateCleanupTypeResponse,
    UpdateExclusionConfigRequest, UpdateExclusionConfigResponse,
};
use axum::{
    Extension, Json, Router,
    extract::State,
    routing::{get, post},
};
pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_backup_config))
        .route("/get", post(get_backup_config))
        .route("/list_config_with_storage", post(list_config_with_storage))
        .route("/list_all", get(list_all_backup_configs))
        .route("/toggle-active", post(toggle_active))
        .route("/rename", post(rename_backup_config))
        .route("/update-cleanup-type", post(update_cleanup_type))
        .route("/update-exclusions", post(update_exclusion_config_handler))
}

async fn create_backup_config(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateBackupConfigRequest>,
) -> WebAppResult<Json<CreateBackupConfigResponse>> {
    let response = application::create(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn get_backup_config(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetBackupConfigRequest>,
) -> WebAppResult<Json<GetBackupConfigResponse>> {
    let response = application::get_by_id(env, ctx.user_id, payload.id).await?;
    Ok(Json(response))
}

async fn list_config_with_storage(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ListBackupConfigWithRemoteStorageRequest>,
) -> WebAppResult<Json<ListBackupConfigWithRemoteStorageResponse>> {
    let response =
        application::list_config_with_storage(env, ctx.user_id, payload.physical_device_id).await?;
    Ok(Json(response))
}

async fn list_all_backup_configs(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<ListAllBackupConfigsResponse>> {
    let response = application::list_all(env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn toggle_active(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ToggleBackupConfigRequest>,
) -> WebAppResult<Json<ToggleBackupConfigResponse>> {
    let response = application::set_active(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn rename_backup_config(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<RenameBackupConfigRequest>,
) -> WebAppResult<Json<RenameBackupConfigResponse>> {
    let response = application::rename(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn update_cleanup_type(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<UpdateCleanupTypeRequest>,
) -> WebAppResult<Json<UpdateCleanupTypeResponse>> {
    let response = application::update_cleanup_type(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn update_exclusion_config_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<UpdateExclusionConfigRequest>,
) -> WebAppResult<Json<UpdateExclusionConfigResponse>> {
    let response = application::update_exclusion_config(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}
