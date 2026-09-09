use crate::{
    app_env::AppEnv,
    core::gc::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::gc::{
    ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectRequest,
    GcCollectResponse, GcRunDetailResponse, GetGcRunDetailRequest, GetRetentionSettingsResponse,
    ListGcRunsRequest, ListGcRunsResponse, UpdateRetentionSettingsRequest,
};
use axum::{Extension, Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/collect", post(collect_handler))
        .route(
            "/confirm_chunk_deletions",
            post(confirm_chunk_deletions_handler),
        )
        .route("/list_runs", post(list_runs_handler))
        .route("/run_detail", post(run_detail_handler))
        .route("/retention_settings", post(get_retention_settings_handler))
        .route(
            "/update_retention_settings",
            post(update_retention_settings_handler),
        )
}

async fn collect_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(_request): Json<GcCollectRequest>,
) -> WebAppResult<Json<GcCollectResponse>> {
    let response = application::collect(env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn confirm_chunk_deletions_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<ConfirmChunkDeletionsRequest>,
) -> WebAppResult<Json<ConfirmChunkDeletionsResponse>> {
    let response = application::confirm_chunk_deletions(env, ctx.user_id, request).await?;
    Ok(Json(response))
}

async fn list_runs_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<ListGcRunsRequest>,
) -> WebAppResult<Json<ListGcRunsResponse>> {
    let response = application::list_gc_runs(env, ctx.user_id, request).await?;
    Ok(Json(response))
}

async fn run_detail_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<GetGcRunDetailRequest>,
) -> WebAppResult<Json<GcRunDetailResponse>> {
    let response = application::get_gc_run_detail(env, ctx.user_id, request).await?;
    Ok(Json(response))
}

async fn get_retention_settings_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetRetentionSettingsResponse>> {
    let response = application::get_retention_settings(env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn update_retention_settings_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(request): Json<UpdateRetentionSettingsRequest>,
) -> WebAppResult<Json<()>> {
    application::update_retention_settings(env, ctx.user_id, request).await?;
    Ok(Json(()))
}
